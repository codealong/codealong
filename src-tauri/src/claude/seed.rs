//! What a new Claude Code account takes from another as Settings › Accounts adds it: what you
//! made yourself in the other's config folder, the one `CLAUDE_CONFIG_DIR` names (read from
//! 2.1.327's code, 2026-11-17). That's `settings.json` (with the plugins it turns on, which
//! Claude Code installs from the marketplaces it lists the first time the account runs),
//! `CLAUDE.md`, `keybindings.json`, the folders of skills, commands, agents, output styles,
//! rules and themes, and the MCP servers added for every folder (its state file's
//! `mcpServers`). Its sign-in, sessions, history and caches stay with it, and so do the skills
//! and plugins claude.ai syncs into it (`skills/synced`, `plugins/synced`), which belong to its
//! sign-in. Nothing already in the new folder is replaced, so a folder that's been used keeps
//! what it has.
//!
//! A file in the folder that `settings.json` names by its path (a status line's script, a
//! hook's) is copied too, and the copy of `settings.json` names the new folder's, so editing one
//! account's script leaves the other's alone. A path Codealong can't follow (one with `\`s, as
//! Windows writes them, or into a folder that isn't copied) still names the other folder's file,
//! which works as long as it's there.

use std::path::Path;

use anyhow::{Context, Result};
use serde_json::Value;

use super::trust::{state_file, StateLock};
use crate::jsonedit::Node;

/// Files of yours that Claude Code reads from the folder, besides `settings.json`.
const FILES: &[&str] = &["CLAUDE.md", "keybindings.json"];

/// Folders of yours that Claude Code reads from the folder.
const FOLDERS: &[&str] = &["skills", "commands", "agents", "output-styles", "rules", "themes"];

/// What claude.ai syncs into `skills/` for the account's sign-in.
const SYNCED: &str = "synced";

/// Copies what you made yourself in the account folder `from` into `to`, a new account's,
/// making `to` when it isn't there. Nothing already in `to` is replaced.
pub fn copy(from: &Path, to: &Path) -> Result<()> {
    copy_in(from, to, &crate::util::home())
}

/// `copy`, with `user_home` as the home folder `~` and `$HOME` name in `settings.json`.
fn copy_in(from: &Path, to: &Path, user_home: &Path) -> Result<()> {
    if !from.is_dir() {
        anyhow::bail!("{} isn't there", crate::util::tilde(from));
    }
    if dunce::canonicalize(from).ok() == dunce::canonicalize(to).ok() {
        anyhow::bail!("{} is the folder it would copy into", crate::util::tilde(from));
    }
    // Claude Code makes its folder for you alone.
    crate::os::files::dir_mode(std::fs::DirBuilder::new().recursive(true), 0o700).create(to).with_context(|| format!("making {}", crate::util::tilde(to)))?;
    copy_settings(from, to, user_home)?;
    for name in FILES {
        let src = from.join(name);
        if src.is_file() {
            copy_new(&src, &to.join(name))?;
        }
    }
    for name in FOLDERS {
        let src = from.join(name);
        if src.is_dir() {
            copy_folder(&src, &to.join(name), name == &"skills")?;
        }
    }
    copy_mcp_servers(&state_file(from), &state_file(to))
}

/// `settings.json`, and the files of `from`'s that it names, with those paths turned to `to`'s.
fn copy_settings(from: &Path, to: &Path, user_home: &Path) -> Result<()> {
    let (src, dst) = (from.join("settings.json"), to.join("settings.json"));
    if dst.exists() {
        return Ok(());
    }
    let Ok(text) = std::fs::read_to_string(&src) else { return Ok(()) };
    let names: Vec<String> = std::fs::read_dir(from)?
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n != "settings.json")
        .collect();
    let (text, named) = follow_paths(&text, &ways(from, to, user_home), &names);
    for name in &named {
        copy_new(&from.join(name), &to.join(name))?;
    }
    write_new(&dst, &text)
}

/// The ways `settings.json` can write the folder `from`, each with the same way of writing `to`:
/// whole, and from the home folder as `~`, `$HOME` and `${HOME}` when `from` is in there (`to`
/// whole when it isn't). Each with `/`s, as a command line takes them.
fn ways(from: &Path, to: &Path, user_home: &Path) -> Vec<(String, String)> {
    let whole = |dir: &Path| dir.to_string_lossy().into_owned();
    let in_home = |dir: &Path| crate::util::tilde_in(dir, user_home).strip_prefix("~/").map(str::to_string);
    let mut ways = vec![(whole(from), whole(to))];
    if let Some(rest) = in_home(from) {
        let to_rest = in_home(to);
        for prefix in ["~/", "$HOME/", "${HOME}/"] {
            ways.push((format!("{prefix}{rest}"), to_rest.as_ref().map_or_else(|| whole(to), |r| format!("{prefix}{r}"))));
        }
    }
    ways
}

/// `text` with each path that names a file of `names` in the folder, written one of `ways`,
/// turned to the same file in the other folder written the same way, and the names it turned.
/// A path counts only whole: not inside a longer one, nor a longer name.
fn follow_paths(text: &str, ways: &[(String, String)], names: &[String]) -> (String, Vec<String>) {
    let in_path = |c: char| c.is_alphanumeric() || "/._-~$}".contains(c);
    let in_name = |c: char| c.is_alphanumeric() || "._-".contains(c);
    let mut out = text.to_string();
    let mut named = Vec::new();
    for name in names {
        for (f, t) in ways {
            let (path, with) = (format!("{f}/{name}"), format!("{t}/{name}"));
            let mut at = 0;
            while let Some(found) = out[at..].find(&path).map(|i| at + i) {
                let end = found + path.len();
                let whole = !out[..found].chars().next_back().is_some_and(in_path) && !out[end..].chars().next().is_some_and(in_name);
                if whole {
                    out.replace_range(found..end, &with);
                    at = found + with.len();
                    if !named.contains(name) {
                        named.push(name.clone());
                    }
                } else {
                    at = end;
                }
            }
        }
    }
    (out, named)
}

/// A folder of yours, copied entry by entry into `to`, over what's there without replacing any
/// of it. What's hidden at its top (a `.trash`, Finder's `.DS_Store`) stays behind, and in
/// `skills` what claude.ai synced for the other account's sign-in.
fn copy_folder(from: &Path, to: &Path, skills: bool) -> Result<()> {
    std::fs::create_dir_all(to).with_context(|| format!("making {}", crate::util::tilde(to)))?;
    for entry in std::fs::read_dir(from)?.flatten() {
        let name = entry.file_name();
        let hidden = name.to_string_lossy().starts_with('.');
        if hidden || (skills && name == SYNCED) {
            continue;
        }
        copy_entry(&entry.path(), &to.join(&name))?;
    }
    Ok(())
}

/// A file, a link or a folder and all it holds, copied to `to`, which nothing there is
/// replaced by.
fn copy_entry(from: &Path, to: &Path) -> Result<()> {
    let kind = std::fs::symlink_metadata(from)?.file_type();
    if kind.is_dir() {
        std::fs::create_dir_all(to)?;
        for entry in std::fs::read_dir(from)?.flatten() {
            copy_entry(&entry.path(), &to.join(entry.file_name()))?;
        }
        return Ok(());
    }
    if std::fs::symlink_metadata(to).is_ok() {
        return Ok(());
    }
    if kind.is_symlink() {
        // A link you made (to a skill kept elsewhere) goes where it went.
        crate::os::files::symlink(std::fs::read_link(from)?, to).with_context(|| format!("linking {}", crate::util::tilde(to)))?;
        return Ok(());
    }
    copy_new(from, to)
}

/// A file copied to `to` unless something's there, keeping whether it runs.
fn copy_new(from: &Path, to: &Path) -> Result<()> {
    if std::fs::symlink_metadata(to).is_ok() {
        return Ok(());
    }
    std::fs::copy(from, to).with_context(|| format!("copying {} to {}", crate::util::tilde(from), crate::util::tilde(to)))?;
    Ok(())
}

/// A file written at `to` unless something's there, for you alone, as Claude Code writes its
/// settings.
fn write_new(to: &Path, text: &str) -> Result<()> {
    use std::io::Write;
    let mut file = match crate::os::files::create_mode(std::fs::OpenOptions::new().write(true).create_new(true), 0o600).open(to) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => return Ok(()),
        Err(e) => return Err(e).with_context(|| format!("writing {}", crate::util::tilde(to))),
    };
    file.write_all(text.as_bytes())?;
    Ok(())
}

/// The MCP servers `from`'s state file has for every folder, into `to`'s when it has none. A
/// new account's state file isn't there until it runs: Claude Code starts from what it holds.
fn copy_mcp_servers(from: &Path, to: &Path) -> Result<()> {
    let Ok(text) = std::fs::read_to_string(from) else { return Ok(()) };
    let servers = serde_json::from_str::<Value>(&text).ok().and_then(|v| v.get("mcpServers").filter(|s| s.as_object().is_some_and(|o| !o.is_empty())).cloned());
    let Some(servers) = servers else { return Ok(()) };
    let _lock = StateLock::take(to)?;
    let text = match std::fs::read_to_string(to) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => "{}".to_string(),
        Err(e) => return Err(e.into()),
    };
    let Some(next) = with_servers(&text, &servers)? else { return Ok(()) };
    crate::util::write_whole(to, &next)
}

/// `text`, a state file, with `servers` as its `mcpServers`, or None when it has some of its
/// own. Everything else keeps its text and its order.
fn with_servers(text: &str, servers: &Value) -> Result<Option<String>> {
    let has_own = serde_json::from_str::<Value>(text)?.get("mcpServers").is_some_and(|s| s.as_object().is_some_and(|o| !o.is_empty()));
    if has_own {
        return Ok(None);
    }
    let mut top = Node::entries(text)?;
    top.retain(|(k, _)| k != "mcpServers");
    top.push(("mcpServers".to_string(), Node::Raw(serde_json::value::RawValue::from_string(serde_json::to_string(servers)?)?)));
    // As Claude Code writes it: JSON.stringify(config, null, 2).
    let mut out = serde_json::to_string_pretty(&Node::Object(top))?;
    out.push('\n');
    Ok(Some(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn read(path: &Path) -> String {
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
    }

    #[test]
    fn a_new_account_takes_what_you_made_and_none_of_the_other_accounts_own() {
        let home = crate::fixtures::scratch("seed");
        let (from, to) = (home.join(".claude-a"), home.join(".claude-b"));
        let settings = r#"{
  "model": "opus",
  "statusLine": {
    "type": "command",
    "command": "bash ~/.claude-a/statusline.sh"
  },
  "hooks": {"Stop": [{"hooks": [{"type": "command", "command": "$HOME/.claude-a/done.sh && ~/.claude-a/missing.sh"}]}]},
  "enabledPlugins": {"kestrel@atlas": true}
}
"#;
        write(&from.join("settings.json"), settings);
        write(&from.join("statusline.sh"), "echo a");
        write(&from.join("done.sh"), "echo done");
        write(&from.join("CLAUDE.md"), "Be brief.");
        write(&from.join("keybindings.json"), "{}");
        write(&from.join("skills/review/SKILL.md"), "review");
        write(&from.join("skills/synced/org_user/notes/SKILL.md"), "the other sign-in's");
        write(&from.join("skills/.trash/old/SKILL.md"), "thrown away");
        write(&from.join("commands/ship.md"), "ship");
        write(&from.join("agents/tester.md"), "tester");
        write(&from.join("rules/style.md"), "style");
        // The account's own: its records, its state beyond the MCP servers, a key it keeps.
        write(&from.join("history.jsonl"), "{}");
        write(&from.join("projects/-r-northwind/s.jsonl"), "{}");
        write(&from.join("atlas-credentials.json"), "{\"key\":\"x\"}");
        write(&from.join(".claude.json"), r#"{"oauthAccount":{"emailAddress":"a@example.com"},"mcpServers":{"kestrel":{"command":"kestrel-mcp"}}}"#);
        #[cfg(unix)]
        crate::os::files::symlink(home.join("elsewhere/skill"), from.join("skills/linked")).unwrap();

        copy_in(&from, &to, &home).unwrap();

        let copied = read(&to.join("settings.json"));
        assert!(copied.contains("\"command\": \"bash ~/.claude-b/statusline.sh\""), "{copied}");
        assert!(copied.contains("$HOME/.claude-b/done.sh && ~/.claude-a/missing.sh"), "a file that isn't there stays named where it was: {copied}");
        assert!(copied.contains("\"kestrel@atlas\": true"));
        assert_eq!(read(&to.join("statusline.sh")), "echo a");
        assert_eq!(read(&to.join("done.sh")), "echo done");
        assert_eq!(read(&to.join("CLAUDE.md")), "Be brief.");
        assert_eq!(read(&to.join("keybindings.json")), "{}");
        assert_eq!(read(&to.join("skills/review/SKILL.md")), "review");
        assert_eq!(read(&to.join("commands/ship.md")), "ship");
        assert_eq!(read(&to.join("agents/tester.md")), "tester");
        assert_eq!(read(&to.join("rules/style.md")), "style");
        #[cfg(unix)]
        assert_eq!(std::fs::read_link(to.join("skills/linked")).unwrap(), home.join("elsewhere/skill"));
        for left in ["skills/synced", "skills/.trash", "history.jsonl", "projects", "atlas-credentials.json"] {
            assert!(!to.join(left).exists(), "{left} is the other account's own");
        }
        let state: Value = serde_json::from_str(&read(&to.join(".claude.json"))).unwrap();
        assert_eq!(state, serde_json::json!({"mcpServers": {"kestrel": {"command": "kestrel-mcp"}}}), "the MCP servers, not the sign-in");
    }

    #[test]
    fn what_the_new_folder_has_already_stays() {
        let home = crate::fixtures::scratch("seed-kept");
        let (from, to) = (home.join(".claude-a"), home.join(".claude-b"));
        write(&from.join("settings.json"), r#"{"statusLine":{"command":"~/.claude-a/statusline.sh"}}"#);
        write(&from.join("statusline.sh"), "echo a");
        write(&from.join("CLAUDE.md"), "Be brief.");
        write(&from.join("skills/review/SKILL.md"), "review");
        write(&from.join("skills/plan/SKILL.md"), "plan");
        write(&from.join(".claude.json"), r#"{"mcpServers":{"kestrel":{"command":"kestrel-mcp"}}}"#);
        write(&to.join("settings.json"), r#"{"model":"sonnet"}"#);
        write(&to.join("CLAUDE.md"), "Mine.");
        write(&to.join("skills/review/SKILL.md"), "my review");
        write(&to.join(".claude.json"), "{\n  \"numStartups\": 3,\n  \"mcpServers\": {\n    \"mine\": {}\n  }\n}\n");

        copy_in(&from, &to, &home).unwrap();

        assert_eq!(read(&to.join("settings.json")), r#"{"model":"sonnet"}"#);
        assert!(!to.join("statusline.sh").exists(), "only a settings.json it writes brings the files it names");
        assert_eq!(read(&to.join("CLAUDE.md")), "Mine.");
        assert_eq!(read(&to.join("skills/review/SKILL.md")), "my review");
        assert_eq!(read(&to.join("skills/plan/SKILL.md")), "plan");
        assert!(read(&to.join(".claude.json")).contains("\"mine\""));
        assert!(!read(&to.join(".claude.json")).contains("kestrel"));
    }

    #[test]
    fn a_state_file_without_servers_takes_them_and_keeps_the_rest_as_written() {
        let servers = serde_json::json!({"kestrel": {"command": "kestrel-mcp"}});
        let text = "{\n  \"numStartups\": 3,\n  \"mcpServers\": {},\n  \"theme\": \"dark\"\n}\n";
        let next = with_servers(text, &servers).unwrap().unwrap();
        assert_eq!(serde_json::from_str::<Value>(&next).unwrap(), serde_json::json!({"numStartups": 3, "theme": "dark", "mcpServers": servers}));
        assert!(next.starts_with("{\n  \"numStartups\": 3,\n  \"theme\": \"dark\""), "{next}");
        assert!(with_servers(&next, &servers).unwrap().is_none());
    }

    #[test]
    fn a_path_is_followed_only_whole() {
        let names = vec!["statusline.sh".to_string()];
        let tilde = vec![("~/.claude".to_string(), "~/.claude-work".to_string())];
        let follow = |text: &str| follow_paths(text, &tilde, &names).0;
        assert_eq!(follow("bash ~/.claude/statusline.sh"), "bash ~/.claude-work/statusline.sh");
        assert_eq!(follow("\"~/.claude/statusline.sh\" \"~/.claude/statusline.sh\""), "\"~/.claude-work/statusline.sh\" \"~/.claude-work/statusline.sh\"");
        assert_eq!(follow("~/.claude/statusline.sh.bak"), "~/.claude/statusline.sh.bak", "a longer name");
        assert_eq!(follow("~/.claude2/statusline.sh"), "~/.claude2/statusline.sh", "another folder");
        assert_eq!(follow("/x~/.claude/statusline.sh"), "/x~/.claude/statusline.sh", "inside a longer path");
        let whole = vec![("/Users/me/.claude".to_string(), "/Users/me/.claude-work".to_string())];
        let (text, named) = follow_paths("sh /Users/me/.claude/statusline.sh; /opt/Users/me/.claude/statusline.sh", &whole, &names);
        assert_eq!(text, "sh /Users/me/.claude-work/statusline.sh; /opt/Users/me/.claude/statusline.sh");
        assert_eq!(named, names);
    }

    #[test]
    fn each_way_of_writing_the_folder_goes_to_the_same_way_of_writing_the_new_one() {
        let home = Path::new("/Users/me");
        let ways = ways(&home.join(".claude"), &home.join(".claude-work"), home);
        let pairs: Vec<(&str, &str)> = ways.iter().map(|(f, t)| (f.as_str(), t.as_str())).collect();
        #[cfg(unix)]
        assert_eq!(pairs, [("/Users/me/.claude", "/Users/me/.claude-work"), ("~/.claude", "~/.claude-work"), ("$HOME/.claude", "$HOME/.claude-work"), ("${HOME}/.claude", "${HOME}/.claude-work")]);
        // A new folder outside the home folder is written whole however the old one was.
        let outside = super::ways(&home.join(".claude"), Path::new("/opt/claude-b"), home);
        assert!(outside.iter().skip(1).all(|(_, t)| t == &Path::new("/opt/claude-b").to_string_lossy()));
    }

    #[test]
    fn copies_nothing_into_itself_or_from_nowhere() {
        let home = crate::fixtures::scratch("seed-self");
        let from = home.join(".claude-a");
        std::fs::create_dir_all(&from).unwrap();
        assert!(copy_in(&from, &from, &home).is_err());
        assert!(copy_in(&home.join(".claude-none"), &home.join(".claude-b"), &home).is_err());
        assert!(!home.join(".claude-b").exists());
    }
}
