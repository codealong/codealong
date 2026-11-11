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

/// The name kept beside a transcript (`name_transcript`), for when its `custom-title` line is too
/// far back to read. Claude Code's `/resume` goes by the end of the transcript, then this (2.1.319).
pub fn kept_name(transcript: &Path) -> Option<String> {
    let (dir, id) = (transcript.parent()?, transcript.file_stem()?);
    let kept: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join(id).join("custom-title.json")).ok()?).ok()?;
    kept["customTitle"].as_str().map(str::trim).filter(|name| !name.is_empty()).map(String::from)
}

/// The file's last line has no newline yet.
fn ends_mid_line(path: &Path) -> bool {
    let Ok(mut f) = File::open(path) else { return false };
    let mut last = [0u8; 1];
    f.seek(SeekFrom::End(-1)).is_ok() && f.read_exact(&mut last).is_ok() && last[0] != b'\n'
}

#[cfg(test)]
mod tests {
    use super::name_transcript;
    use crate::claude::ClaudeCode;
    use crate::claude::transcript::{parse_lines, parse_transcript, HEAD};
    use crate::model::{Harness, Session};
    use std::io::Write;

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
        let account = crate::config::AccountCfg { name: "claude".into(), harness: "claude".into(), home: home.to_string_lossy().into(), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false, chrome_profile: None };
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

    /// As a scheduled run went on 2.1.319: named just after its first prompt, then written to so
    /// fast that the process never read the line, so it never stamped it again, and stamped its own
    /// title instead. On a first read the line is beyond the tail, and the name still stands.
    #[test]
    fn a_name_the_session_never_took_up_outlives_the_tail() {
        let dir = crate::fixtures::scratch("kept-name").join("projects/-x");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("25b09e0e.jsonl");
        let own = |title: &str| format!("{{\"type\":\"ai-title\",\"aiTitle\":\"{title}\"}}\n{{\"type\":\"agent-name\",\"agentName\":\"{title}\"}}\n");
        let filler = format!("{{\"type\":\"attachment\",\"note\":\"{}\"}}\n", "x".repeat(300 * 1024));
        let append = |text: &str| std::fs::OpenOptions::new().append(true).open(&path).unwrap().write_all(text.as_bytes()).unwrap();
        let read = || parse_transcript(&path, 1, None).unwrap().1;
        std::fs::write(&path, format!("{HEAD}{}", own("Sync the northwind reports"))).unwrap();
        name_transcript(&path, "Nightly digest").unwrap();
        // Its line is the last for now, then far behind what the process goes on writing.
        append(&format!("{filler}{}", own("Northwind report sync")));
        let info = read();
        assert_eq!((info.title.as_deref(), info.agent_name.as_deref(), info.ai_title.as_deref()), (Some("Nightly digest"), Some("Northwind report sync"), Some("Northwind report sync")));
        // A name later in the transcript is the latest, as `/resume` has it.
        append("{\"type\":\"custom-title\",\"customTitle\":\"Weekly digest\"}\n");
        assert_eq!(read().title.as_deref(), Some("Weekly digest"));
        // With no name kept and none in reach, there's only its own.
        append(&filler);
        std::fs::remove_file(dir.join("25b09e0e/custom-title.json")).unwrap();
        append(&own("Northwind report sync"));
        let info = read();
        assert_eq!((info.title, info.agent_name.as_deref()), (None, Some("Northwind report sync")));
    }
}
