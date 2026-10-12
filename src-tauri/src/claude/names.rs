//! A session's name, where Claude Code keeps it.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// Names a session where Claude Code keeps its name: a `custom-title` line at the end of its
/// transcript, keys in Claude Code's order, and `<session>/custom-title.json` beside it, which
/// `/resume` lists sessions by.
pub fn name_transcript(transcript: &Path, name: &str) -> anyhow::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
    let (Some(dir), Some(id)) = (transcript.parent(), transcript.file_stem().and_then(|s| s.to_str())) else {
        anyhow::bail!("{} isn't a transcript.", transcript.display());
    };
    let line = format!(r#"{{"type":"custom-title","customTitle":{},"sessionId":{}}}"#, serde_json::to_string(name)?, serde_json::to_string(id)?);
    // A line cut off midway is ended first, as Claude Code does before it appends, so the name
    // isn't lost inside it. Appending leaves Claude Code's own writes whole.
    let cut = ends_mid_line(transcript);
    std::fs::OpenOptions::new().append(true).open(transcript)?.write_all(format!("{}{line}\n", if cut { "\n" } else { "" }).as_bytes())?;
    let folder = dir.join(id);
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&folder)?;
    let part = folder.join(format!(".custom-title.json.{}", std::process::id()));
    std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&part)?.write_all(serde_json::json!({ "customTitle": name }).to_string().as_bytes())?;
    std::fs::rename(&part, folder.join("custom-title.json"))?;
    Ok(())
}

/// The file's last line has no newline yet.
fn ends_mid_line(path: &Path) -> bool {
    let Ok(mut f) = File::open(path) else { return false };
    let mut last = [0u8; 1];
    f.seek(SeekFrom::End(-1)).is_ok() && f.read_exact(&mut last).is_ok() && last[0] != b'\n'
}

#[cfg(test)]
mod tests {
    use crate::claude::ClaudeCode;
    use crate::claude::transcript::{parse_lines, HEAD};
    use crate::model::{Harness, Session};

    #[test]
    fn names_a_session_where_claude_code_keeps_it() {
        use crate::harness::Adapter;
        use std::os::unix::fs::PermissionsExt;
        let home = crate::fixtures::scratch("rename");
        let dir = home.join("projects/-x");
        std::fs::create_dir_all(&dir).unwrap();
        // A job that goes on from a conversation, whose transcript ends in a line cut off midway.
        let (job, from) = ("398f3533-73b3-4089-af4e-06603a870e04", "25b09e0e");
        std::fs::write(dir.join(format!("{from}.jsonl")), format!("{HEAD}{{\"type\":\"ai-title\",\"aiTitle\":\"Pho")).unwrap();
        std::fs::write(dir.join(format!("{job}.jsonl")), format!("{{\"type\":\"ai-title\",\"aiTitle\":\"Photo tools\",\"sessionId\":\"{job}\"}}\n")).unwrap();
        let account = crate::config::AccountCfg { name: "claude".into(), harness: "claude".into(), home: home.to_string_lossy().into(), command: None, start: None, preferred: false, acp: None, monthly: None, per_token: false };
        let session = |id: &str, continues: Option<&str>| Session { live: true, title: "Photo tools".into(), cwd: "/x".into(), continues: continues.map(String::from), ..Session::new(Harness::Claude, "claude", id) };
        let ctx = crate::harness::Ctx { account: &account };
        let name = r#"Photo tools, "v2""#;
        let changed = ClaudeCode.rename(&ctx, &session(job, Some(from)), name).unwrap();
        assert_eq!(changed, vec![dir.join(format!("{job}.jsonl")), dir.join(format!("{from}.jsonl"))]);
        // The job's own transcript, for the job to take the name up; its line as Claude Code writes it.
        let own = std::fs::read_to_string(dir.join(format!("{job}.jsonl"))).unwrap();
        assert!(own.ends_with(&format!("}}\n{{\"type\":\"custom-title\",\"customTitle\":\"Photo tools, \\\"v2\\\"\",\"sessionId\":\"{job}\"}}\n")), "{own}");
        // The one the board reads it by meanwhile: the cut-off line is ended first, and the name reads back.
        let text = std::fs::read_to_string(dir.join(format!("{from}.jsonl"))).unwrap();
        assert!(text.contains("\"aiTitle\":\"Pho\n{\"type\":\"custom-title\""), "{text}");
        assert_eq!(parse_lines(HEAD, &text[HEAD.len()..]).title.as_deref(), Some(name));
        // What /resume lists it by, readable only by you.
        let sidecar = dir.join(from).join("custom-title.json");
        assert_eq!(std::fs::read_to_string(&sidecar).unwrap(), r#"{"customTitle":"Photo tools, \"v2\""}"#);
        assert_eq!(std::fs::metadata(&sidecar).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(std::fs::metadata(dir.join(from)).unwrap().permissions().mode() & 0o777, 0o700);
        // A session that hasn't written a transcript yet has nowhere to keep a name.
        assert!(ClaudeCode.rename(&ctx, &session("0f5e1d2c", None), name).is_err());
    }
}
