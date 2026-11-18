//! What a new Claude Code account takes from another as Settings › Accounts adds it: what you
//! made yourself in the other's config folder, the one `CLAUDE_CONFIG_DIR` names (read from
//! 2.1.327's code, 2026-11-17). That's `settings.json` (with the plugins it turns on, which
//! Claude Code installs from the marketplaces it lists the first time the account runs),
//! `CLAUDE.md`, `keybindings.json`, the folders of skills, commands, agents, output styles,
//! rules and themes, and the MCP servers added for every folder (its state file's
//! `mcpServers`). Its sign-in, sessions, history and caches stay with it, and so do the skills
//! and plugins claude.ai syncs into it (`skills/synced`, `plugins/synced`), which belong to its
//! sign-in. Nothing already in the new folder is replaced or written through: a link there (a
//! skills folder you share) is left as it is.
//!
//! What signs the other account in or pays for it stays out of the new one's settings
//! (`apiKeyHelper` and the like, and in its `env` the keys, tokens and cloud credentials, and
//! what sends it to another provider), as a second account is there to be signed in to another;
//! so does Codealong's own plugin, which install.rs puts in as the form's box says. The scripts
//! in the folder that its status line and hooks run (`~/.claude/hooks/check.sh`, a file that
//! runs or is a script by its extension) are copied, and the new settings run the copies, so
//! changing one account's leaves the other's alone. Nothing else the settings name is followed:
//! a permission rule that names a file is about that file, and a file a command reads (its
//! `history.jsonl`) is the account's own. A path Codealong can't follow (one with `\`s, as
//! Windows writes them, into a new folder whose name a command line would split, or through a
//! link there) still runs the other folder's file, which works as long as it's there.

use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::value::RawValue;
use serde_json::Value;

use super::trust::{state_file, StateLock};
use crate::jsonedit::Node;
use crate::util::tilde;

/// Files of yours that Claude Code reads from the folder, besides `settings.json`.
const FILES: &[&str] = &["CLAUDE.md", "keybindings.json"];

/// Folders of yours that Claude Code reads from the folder.
const FOLDERS: &[&str] = &["skills", "commands", "agents", "output-styles", "rules", "themes"];

/// What claude.ai syncs into `skills/` for the account's sign-in.
const SYNCED: &str = "synced";

/// Settings that say how the account signs in or what pays for it.
const SIGN_IN: &[&str] = &["apiKeyHelper", "awsAuthRefresh", "awsCredentialExport", "gcpAuthRefresh", "forceLoginMethod", "forceLoginOrgUUID"];

/// What in the settings' `env` signs the account in, or sends it to another provider than its
/// own sign-in (Bedrock's setup saves its AWS keys or profile there).
const SIGN_IN_ENV: &[&str] = &[
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "CLAUDE_CODE_OAUTH_TOKEN",
    "ANTHROPIC_BASE_URL",
    "CLAUDE_CODE_USE_BEDROCK",
    "AWS_BEARER_TOKEN_BEDROCK",
    "AWS_ACCESS_KEY_ID",
    "AWS_SECRET_ACCESS_KEY",
    "AWS_SESSION_TOKEN",
    "AWS_PROFILE",
    "CLAUDE_CODE_USE_VERTEX",
    "GOOGLE_APPLICATION_CREDENTIALS",
    "ANTHROPIC_VERTEX_PROJECT_ID",
    "CLAUDE_CODE_USE_FOUNDRY",
    "ANTHROPIC_FOUNDRY_API_KEY",
];

/// Where in the folder a command can run what isn't yours: what plugins installed, and what
/// claude.ai synced for the account's sign-in.
const NOT_YOURS: &[&[&str]] = &[&["plugins"], &["skills", SYNCED]];

/// What a status line or hook runs by an interpreter (`bash x.sh`) without it being a program.
const SCRIPTS: &[&str] = &["sh", "bash", "zsh", "fish", "py", "js", "mjs", "cjs", "ts", "rb", "pl", "ps1", "cmd", "bat"];

/// Copies what you made yourself in the account folder `from` into `to`, a new account's,
/// making `to` when it isn't there. Nothing already in `to` is replaced.
pub fn copy(from: &Path, to: &Path) -> Result<()> {
    copy_in(from, to, &crate::util::home())
}

/// `copy`, with `user_home` as the home folder `~` and `$HOME` name in `settings.json`.
fn copy_in(from: &Path, to: &Path, user_home: &Path) -> Result<()> {
    if !from.is_dir() {
        anyhow::bail!("{} isn't there", tilde(from));
    }
    let (a, b) = (dunce::canonicalize(from)?, resolved(to));
    if a == b {
        anyhow::bail!("{} is the folder it would copy into", tilde(from));
    }
    // One inside the other would copy into what it's copying, and so would a folder of yours
    // that's a link to one holding the new folder.
    if a.starts_with(&b) || b.starts_with(&a) {
        anyhow::bail!("{} and {} are one inside the other", tilde(from), tilde(to));
    }
    for name in FOLDERS {
        if let Ok(real) = dunce::canonicalize(from.join(name)) {
            if b.starts_with(&real) {
                anyhow::bail!("{} holds {}", tilde(&from.join(name)), tilde(to));
            }
        }
    }
    // Claude Code makes its folder for you alone.
    crate::os::files::dir_mode(std::fs::DirBuilder::new().recursive(true), 0o700).create(to).with_context(|| format!("making {}", tilde(to)))?;
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

/// `path` with its links resolved as far as it's there, for one that may not be yet.
fn resolved(path: &Path) -> PathBuf {
    let mut rest = Vec::new();
    for dir in path.ancestors() {
        if let Ok(real) = dunce::canonicalize(dir) {
            return rest.iter().rev().fold(real, |p: PathBuf, part| p.join(part));
        }
        rest.extend(dir.file_name());
    }
    path.to_path_buf()
}

/// `settings.json` as the new account takes it (`settings_for`), and the files it runs.
fn copy_settings(from: &Path, to: &Path, user_home: &Path) -> Result<()> {
    let (src, dst) = (from.join("settings.json"), to.join("settings.json"));
    if std::fs::symlink_metadata(&dst).is_ok() {
        return Ok(());
    }
    let text = match std::fs::read_to_string(&src) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e).with_context(|| format!("reading {}", tilde(&src))),
    };
    let (text, runs) = settings_for(&text, from, to, &ways(from, to, user_home)).with_context(|| format!("reading {}", tilde(&src)))?;
    for file in &runs {
        if own_dirs(to, file.parent().unwrap_or(Path::new("")))? {
            copy_new(&from.join(file), &to.join(file))?;
        }
    }
    write_new(&dst, &text)
}

/// `settings.json`'s text for the new account: without what signs the other in or pays for it,
/// without Codealong's plugin, and running `to`'s copies of the scripts in `from` that its
/// status line and hooks run; and those scripts, from the folder. The rest keeps its text and
/// its order.
fn settings_for(text: &str, from: &Path, to: &Path, ways: &[(String, String)]) -> Result<(String, Vec<PathBuf>)> {
    let mut top = Node::entries(text)?;
    top.retain(|(k, _)| !SIGN_IN.contains(&k.as_str()));
    drop_in(&mut top, "env", |k| SIGN_IN_ENV.contains(&k))?;
    drop_in(&mut top, "enabledPlugins", |k| k == crate::install::PLUGIN)?;
    drop_in(&mut top, "extraKnownMarketplaces", |k| k == crate::install::MARKETPLACE)?;
    let mut runs: Vec<PathBuf> = Vec::new();
    for key in ["statusLine", "hooks"] {
        let Some((_, node)) = top.iter_mut().rev().find(|(k, _)| k == key) else { continue };
        let Node::Raw(raw) = node else { continue };
        let mut value: Value = serde_json::from_str(raw.get())?;
        let mut changed = false;
        commands(&mut value, &mut |command| {
            let (next, files) = follow_paths(command, ways, from, to);
            if !files.is_empty() {
                *command = next;
                changed = true;
                for file in files {
                    if !runs.contains(&file) {
                        runs.push(file);
                    }
                }
            }
        });
        if changed {
            // One level in, as JSON.stringify(settings, null, 2) writes it.
            *node = Node::Raw(RawValue::from_string(serde_json::to_string_pretty(&value)?.replace('\n', "\n  "))?);
        }
    }
    let mut out = serde_json::to_string_pretty(&Node::Object(top))?;
    if text.ends_with('\n') {
        out.push('\n');
    }
    Ok((out, runs))
}

/// The object under `key` in `entries` without the keys `drop` picks, when there is one.
fn drop_in(entries: &mut Vec<(String, Node)>, key: &str, drop: impl Fn(&str) -> bool) -> Result<()> {
    if entries.iter().any(|(k, _)| k == key) {
        Node::object_in(entries, key)?.retain(|(k, _)| !drop(k));
    }
    Ok(())
}

/// Every command in a part of the settings (`statusLine`, `hooks`): each string under a
/// `command` key.
fn commands(value: &mut Value, each: &mut dyn FnMut(&mut String)) {
    match value {
        Value::Object(map) => {
            for (k, v) in map.iter_mut() {
                match (k.as_str(), v) {
                    ("command", Value::String(s)) => each(s),
                    (_, v) => commands(v, each),
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(|v| commands(v, each)),
        _ => {}
    }
}

/// The ways `settings.json` can write the folder `from`, each with the same way of writing `to`:
/// whole, and from the home folder as `~`, `$HOME` and `${HOME}` when `from` is in there (`to`
/// whole when it isn't). Each with `/`s, as a command line takes them. `$CLAUDE_CONFIG_DIR` is
/// each account's own folder already, so it stays as it's written, its scripts copied.
fn ways(from: &Path, to: &Path, user_home: &Path) -> Vec<(String, String)> {
    let whole = |dir: &Path| dir.to_string_lossy().into_owned();
    let in_home = |dir: &Path| crate::util::tilde_in(dir, user_home).strip_prefix("~/").map(str::to_string);
    let mut ways = vec![(whole(from), whole(to))];
    for own in ["$CLAUDE_CONFIG_DIR", "${CLAUDE_CONFIG_DIR}"] {
        ways.push((own.to_string(), own.to_string()));
    }
    if let Some(rest) = in_home(from) {
        let to_rest = in_home(to);
        for prefix in ["~/", "$HOME/", "${HOME}/"] {
            ways.push((format!("{prefix}{rest}"), to_rest.as_ref().map_or_else(|| whole(to), |r| format!("{prefix}{r}"))));
        }
    }
    ways
}

/// `command` with each path to a script of yours in the folder `from`, written one of `ways`,
/// turned to the same file in `to` written the same way, and those scripts (from the folder). A
/// path counts from where one can start, and to a file that runs (`runs`): not a hidden one, nor
/// one a plugin installed or claude.ai synced, nor one a copy can't land in `to` (through a
/// link there). A new folder whose name a command line would split isn't followed into.
fn follow_paths(command: &str, ways: &[(String, String)], from: &Path, to: &Path) -> (String, Vec<PathBuf>) {
    let in_path = |c: char| c.is_alphanumeric() || "/._-~$}".contains(c);
    let plain = |s: &str| s.chars().all(|c| c.is_alphanumeric() || "/._-~${}".contains(c));
    let yours = |rel: &Path| {
        let parts: Vec<String> = rel.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
        let named = rel.components().all(|c| matches!(c, Component::Normal(n) if !n.to_string_lossy().starts_with('.')));
        let theirs = NOT_YOURS.iter().any(|under| parts.len() > under.len() && parts.iter().zip(under.iter()).all(|(p, u)| p == u));
        !parts.is_empty() && named && !theirs && runs(&from.join(rel)) && lands(to, rel)
    };
    let mut out = command.to_string();
    let mut files: Vec<PathBuf> = Vec::new();
    for (f, t) in ways.iter().filter(|(_, t)| plain(t)) {
        let lead = format!("{f}/");
        let mut at = 0;
        while let Some(found) = out[at..].find(&lead).map(|i| at + i) {
            let start = found + lead.len();
            let len = out[start..].find(|c: char| !(c.is_alphanumeric() || "._-/".contains(c))).unwrap_or(out.len() - start);
            let rel = PathBuf::from(&out[start..start + len]);
            if !out[..found].chars().next_back().is_some_and(in_path) && yours(&rel) {
                let with = format!("{t}/{}", &out[start..start + len]);
                out.replace_range(found..start + len, &with);
                at = found + with.len();
                if !files.contains(&rel) {
                    files.push(rel);
                }
            } else {
                at = start;
            }
        }
    }
    (out, files)
}

/// Whether a file is a script a command runs: a program, or a script by its extension.
fn runs(path: &Path) -> bool {
    let script = path.extension().and_then(|e| e.to_str()).is_some_and(|e| SCRIPTS.contains(&e.to_ascii_lowercase().as_str()));
    path.is_file() && (script || crate::detect::is_executable(path))
}

/// Whether a copy of the file `rel` lands in `to` (or one is there already): every folder on the
/// way that's there is a folder of `to`'s own, not a link to one elsewhere (`own_dir`).
fn lands(to: &Path, rel: &Path) -> bool {
    let mut dir = to.to_path_buf();
    for part in rel.parent().into_iter().flat_map(Path::components) {
        dir.push(part);
        match std::fs::symlink_metadata(&dir) {
            Ok(m) if m.is_dir() => {}
            Ok(_) => return false,
            Err(_) => return true,
        }
    }
    true
}

/// Whether `path` is a folder of the new account's own to copy into, making it when it isn't
/// there. A link to a folder elsewhere (a skills folder you share) or a file isn't.
fn own_dir(path: &Path) -> Result<bool> {
    match std::fs::symlink_metadata(path) {
        Ok(m) => Ok(m.is_dir()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            std::fs::create_dir(path).with_context(|| format!("making {}", tilde(path)))?;
            Ok(true)
        }
        Err(e) => Err(e.into()),
    }
}

/// `own_dir` for each folder from `root` down to `root/rel`.
fn own_dirs(root: &Path, rel: &Path) -> Result<bool> {
    let mut dir = root.to_path_buf();
    for part in rel.components() {
        dir.push(part);
        if !own_dir(&dir)? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// A folder of yours, copied entry by entry into `to`, over what's there without replacing any
/// of it. What's hidden at its top (a `.trash`, Finder's `.DS_Store`) stays behind, and in
/// `skills` what claude.ai synced for the other account's sign-in.
fn copy_folder(from: &Path, to: &Path, skills: bool) -> Result<()> {
    if !own_dir(to)? {
        return Ok(());
    }
    for entry in std::fs::read_dir(from)?.flatten() {
        let name = entry.file_name();
        let hidden = name.to_string_lossy().starts_with('.');
        if hidden || (skills && name == SYNCED) {
            continue;
        }
        copy_entry(&entry.path(), &to.join(&name), from)?;
    }
    Ok(())
}

/// A file, a link or a folder and all it holds, copied to `to`, which nothing there is
/// replaced by; `root` is the folder of yours it's in (`skills`). What's neither (a pipe, a
/// socket) isn't copied.
fn copy_entry(from: &Path, to: &Path, root: &Path) -> Result<()> {
    let kind = std::fs::symlink_metadata(from)?.file_type();
    if kind.is_dir() {
        if own_dir(to)? {
            for entry in std::fs::read_dir(from)?.flatten() {
                copy_entry(&entry.path(), &to.join(entry.file_name()), root)?;
            }
        }
        return Ok(());
    }
    if std::fs::symlink_metadata(to).is_ok() {
        return Ok(());
    }
    if kind.is_symlink() {
        crate::os::files::symlink(link_target(from, root)?, to).with_context(|| format!("linking {}", tilde(to)))?;
        return Ok(());
    }
    if kind.is_file() {
        copy_new(from, to)?;
    }
    Ok(())
}

/// Where a copy of the link `link` goes: a link you made to a skill kept elsewhere goes where it
/// went, named whole, as a relative one went from where it was; one to something in `root`
/// keeps going to it in the copy, as it's written.
fn link_target(link: &Path, root: &Path) -> Result<PathBuf> {
    let target = std::fs::read_link(link)?;
    let Some(dir) = link.parent().filter(|_| target.is_relative()) else { return Ok(target) };
    let whole = lexical(&dir.join(&target));
    Ok(if whole.starts_with(lexical(root)) { target } else { whole })
}

/// `path` with its `.` and `..` worked out from its names alone, as a link's target is read
/// before what it names may be there.
fn lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

/// A file copied to `to` unless something's there, keeping whether it runs. One that couldn't
/// be copied whole isn't left behind, where a second try would take it for yours.
fn copy_new(from: &Path, to: &Path) -> Result<()> {
    let mut src = std::fs::File::open(from).with_context(|| format!("reading {}", tilde(from)))?;
    let permissions = src.metadata()?.permissions();
    let mut dst = match std::fs::OpenOptions::new().write(true).create_new(true).open(to) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => return Ok(()),
        Err(e) => return Err(e).with_context(|| format!("copying {} to {}", tilde(from), tilde(to))),
    };
    let copied = std::io::copy(&mut src, &mut dst).and_then(|_| dst.set_permissions(permissions));
    if let Err(e) = copied {
        drop(dst);
        let _ = std::fs::remove_file(to);
        return Err(e).with_context(|| format!("copying {} to {}", tilde(from), tilde(to)));
    }
    Ok(())
}

/// A file written at `to` unless something's there, for you alone, as Claude Code writes its
/// settings; one that couldn't be written whole isn't left behind.
fn write_new(to: &Path, text: &str) -> Result<()> {
    use std::io::Write;
    let mut file = match crate::os::files::create_mode(std::fs::OpenOptions::new().write(true).create_new(true), 0o600).open(to) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => return Ok(()),
        Err(e) => return Err(e).with_context(|| format!("writing {}", tilde(to))),
    };
    if let Err(e) = file.write_all(text.as_bytes()) {
        drop(file);
        let _ = std::fs::remove_file(to);
        return Err(e).with_context(|| format!("writing {}", tilde(to)));
    }
    Ok(())
}

/// The MCP servers `from`'s state file has for every folder, into `to`'s when it has none. A
/// new account's state file isn't there until it runs: Claude Code starts from what it holds.
fn copy_mcp_servers(from: &Path, to: &Path) -> Result<()> {
    let text = match std::fs::read_to_string(from) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e).with_context(|| format!("reading {}", tilde(from))),
    };
    let state: Value = serde_json::from_str(&text).with_context(|| format!("reading {}", tilde(from)))?;
    let Some(servers) = state.get("mcpServers").filter(|s| s.as_object().is_some_and(|o| !o.is_empty())) else { return Ok(()) };
    // A state file that's a link is another's, which write_whole would write through.
    if std::fs::symlink_metadata(to).is_ok_and(|m| m.file_type().is_symlink()) {
        return Ok(());
    }
    let _lock = StateLock::take(to)?;
    let text = match std::fs::read_to_string(to) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => "{}".to_string(),
        Err(e) => return Err(e.into()),
    };
    let Some(next) = with_servers(&text, servers)? else { return Ok(()) };
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
    top.push(("mcpServers".to_string(), Node::Raw(RawValue::from_string(serde_json::to_string(servers)?)?)));
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
  "apiKeyHelper": "~/bin/atlas-key",
  "env": {
    "ANTHROPIC_API_KEY": "sk-atlas",
    "CLAUDE_CODE_USE_BEDROCK": "1",
    "AWS_PROFILE": "atlas",
    "CLAUDE_CODE_ENABLE_TELEMETRY": "1"
  },
  "permissions": {
    "deny": ["Read(~/.claude-a/.credentials.json)", "Read(~/.claude-a/atlas-credentials.json)"]
  },
  "statusLine": {
    "type": "command",
    "command": "bash ~/.claude-a/statusline.sh ~/.claude-a/history.jsonl"
  },
  "hooks": {
    "Stop": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "$HOME/.claude-a/hooks/done.sh && ~/.claude-a/missing.sh && ~/.claude-a/plugins/cache/kestrel/hook.sh && ~/.claude-a/skills/synced/org_user/notes/run.sh"
          },
          {
            "type": "command",
            "command": "$CLAUDE_CONFIG_DIR/hooks/note.py"
          }
        ]
      }
    ]
  },
  "enabledPlugins": {
    "kestrel@atlas": true,
    "codealong@codealong": true
  },
  "extraKnownMarketplaces": {
    "atlas": {"source": {"source": "github", "repo": "atlas/plugins"}},
    "codealong": {"source": {"source": "directory", "path": "/opt/codealong/plugin-marketplace"}}
  }
}
"#;
        write(&from.join("settings.json"), settings);
        write(&from.join("statusline.sh"), "echo a");
        write(&from.join("hooks/done.sh"), "echo done");
        write(&from.join("hooks/note.py"), "print()");
        write(&from.join("skills/synced/org_user/notes/run.sh"), "the other sign-in's");
        write(&from.join("plugins/cache/kestrel/hook.sh"), "a plugin's");
        write(&from.join("CLAUDE.md"), "Be brief.");
        write(&from.join("keybindings.json"), "{}");
        write(&from.join("skills/review/SKILL.md"), "review");
        write(&from.join("skills/synced/org_user/notes/SKILL.md"), "the other sign-in's");
        write(&from.join("skills/.trash/old/SKILL.md"), "thrown away");
        write(&from.join("commands/ship.md"), "ship");
        write(&from.join("agents/tester.md"), "tester");
        write(&from.join("rules/style.md"), "style");
        // The account's own: its records, its sign-in, its state beyond the MCP servers, a key.
        write(&from.join("history.jsonl"), "{}");
        write(&from.join("projects/-r-northwind/s.jsonl"), "{}");
        write(&from.join(".credentials.json"), "{\"token\":\"x\"}");
        write(&from.join("atlas-credentials.json"), "{\"key\":\"x\"}");
        write(&from.join(".claude.json"), r#"{"oauthAccount":{"emailAddress":"a@example.com"},"mcpServers":{"kestrel":{"command":"kestrel-mcp"}}}"#);
        #[cfg(unix)]
        crate::os::files::symlink(home.join("elsewhere/skill"), from.join("skills/linked")).unwrap();
        #[cfg(unix)]
        crate::os::files::symlink("../../shared/plan", from.join("skills/plan")).unwrap();
        #[cfg(unix)]
        crate::os::files::symlink("review/SKILL.md", from.join("skills/also.md")).unwrap();

        copy_in(&from, &to, &home).unwrap();

        let copied = read(&to.join("settings.json"));
        let v: Value = serde_json::from_str(&copied).unwrap();
        assert_eq!(v["statusLine"]["command"], "bash ~/.claude-b/statusline.sh ~/.claude-a/history.jsonl", "a script, not a file it reads");
        assert_eq!(
            v["hooks"]["Stop"][0]["hooks"][0]["command"],
            "$HOME/.claude-b/hooks/done.sh && ~/.claude-a/missing.sh && ~/.claude-a/plugins/cache/kestrel/hook.sh && ~/.claude-a/skills/synced/org_user/notes/run.sh",
            "only scripts of yours that are there"
        );
        assert_eq!(v["hooks"]["Stop"][0]["hooks"][1]["command"], "$CLAUDE_CONFIG_DIR/hooks/note.py", "each account's own folder");
        assert_eq!(read(&to.join("hooks/note.py")), "print()");
        assert_eq!(v["permissions"]["deny"][0], "Read(~/.claude-a/.credentials.json)", "a permission's file is about that file");
        assert!(v.get("apiKeyHelper").is_none(), "{copied}");
        assert_eq!(v["env"], serde_json::json!({"CLAUDE_CODE_ENABLE_TELEMETRY": "1"}));
        assert_eq!(v["enabledPlugins"], serde_json::json!({"kestrel@atlas": true}));
        assert_eq!(v["extraKnownMarketplaces"].as_object().unwrap().keys().collect::<Vec<_>>(), ["atlas"]);
        assert!(copied.starts_with("{\n  \"model\": \"opus\",\n  \"env\": {"), "in its order: {copied}");
        assert!(copied.contains("\n  \"hooks\": {\n    \"Stop\": [\n"), "one level in: {copied}");
        assert_eq!(read(&to.join("statusline.sh")), "echo a");
        assert_eq!(read(&to.join("hooks/done.sh")), "echo done");
        assert_eq!(read(&to.join("CLAUDE.md")), "Be brief.");
        assert_eq!(read(&to.join("keybindings.json")), "{}");
        assert_eq!(read(&to.join("skills/review/SKILL.md")), "review");
        assert_eq!(read(&to.join("commands/ship.md")), "ship");
        assert_eq!(read(&to.join("agents/tester.md")), "tester");
        assert_eq!(read(&to.join("rules/style.md")), "style");
        #[cfg(unix)]
        {
            assert_eq!(std::fs::read_link(to.join("skills/linked")).unwrap(), home.join("elsewhere/skill"));
            assert_eq!(std::fs::read_link(to.join("skills/plan")).unwrap(), home.join("shared/plan"), "a relative link out goes where it went");
            assert_eq!(std::fs::read_link(to.join("skills/also.md")).unwrap(), Path::new("review/SKILL.md"), "one within goes to the copy");
        }
        for left in ["skills/synced", "skills/.trash", "history.jsonl", "projects", "plugins", ".credentials.json", "atlas-credentials.json"] {
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
        write(&from.join("commands/ship.md"), "ship");
        write(&from.join(".claude.json"), r#"{"mcpServers":{"kestrel":{"command":"kestrel-mcp"}}}"#);
        write(&to.join("settings.json"), r#"{"model":"sonnet"}"#);
        write(&to.join("CLAUDE.md"), "Mine.");
        write(&to.join("skills/review/SKILL.md"), "my review");
        write(&to.join(".claude.json"), "{\n  \"numStartups\": 3,\n  \"mcpServers\": {\n    \"mine\": {}\n  }\n}\n");
        // A commands folder shared with other accounts, by a link, isn't written into, nor is a
        // state file that's a link to another.
        write(&home.join("shared-commands/mine.md"), "mine");
        write(&home.join("other-state.json"), "{}");
        #[cfg(unix)]
        {
            crate::os::files::symlink(home.join("shared-commands"), to.join("commands")).unwrap();
            std::fs::remove_file(to.join(".claude.json")).unwrap();
            crate::os::files::symlink(home.join("other-state.json"), to.join(".claude.json")).unwrap();
        }

        copy_in(&from, &to, &home).unwrap();

        assert_eq!(read(&to.join("settings.json")), r#"{"model":"sonnet"}"#);
        assert!(!to.join("statusline.sh").exists(), "only a settings.json it writes brings the files it runs");
        assert_eq!(read(&to.join("CLAUDE.md")), "Mine.");
        assert_eq!(read(&to.join("skills/review/SKILL.md")), "my review");
        assert_eq!(read(&to.join("skills/plan/SKILL.md")), "plan");
        #[cfg(not(unix))]
        assert!(read(&to.join(".claude.json")).contains("\"mine\"") && !read(&to.join(".claude.json")).contains("kestrel"));
        #[cfg(unix)]
        {
            assert!(!home.join("shared-commands/ship.md").exists(), "nothing goes through a link");
            assert_eq!(read(&home.join("other-state.json")), "{}");
        }
    }

    #[test]
    fn a_new_folder_a_command_line_would_split_runs_the_old_ones_files() {
        let home = crate::fixtures::scratch("seed-space");
        let (from, to) = (home.join(".claude-a"), home.join(".claude b"));
        write(&from.join("settings.json"), r#"{"statusLine":{"command":"bash ~/.claude-a/statusline.sh"}}"#);
        write(&from.join("statusline.sh"), "echo a");
        copy_in(&from, &to, &home).unwrap();
        assert!(read(&to.join("settings.json")).contains("bash ~/.claude-a/statusline.sh"));
        assert!(!to.join("statusline.sh").exists());
    }

    #[cfg(unix)]
    #[test]
    fn a_script_that_cant_land_in_the_new_folder_is_run_from_the_old_one() {
        let home = crate::fixtures::scratch("seed-shared-hooks");
        let (from, to) = (home.join(".claude-a"), home.join(".claude-b"));
        write(&from.join("settings.json"), r#"{"hooks":{"Stop":[{"hooks":[{"command":"~/.claude-a/hooks/done.sh"}]}]}}"#);
        write(&from.join("hooks/done.sh"), "echo done");
        std::fs::create_dir_all(home.join("shared-hooks")).unwrap();
        std::fs::create_dir_all(&to).unwrap();
        crate::os::files::symlink(home.join("shared-hooks"), to.join("hooks")).unwrap();
        copy_in(&from, &to, &home).unwrap();
        assert!(read(&to.join("settings.json")).contains("~/.claude-a/hooks/done.sh"));
        assert!(!home.join("shared-hooks/done.sh").exists());
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
    fn a_path_is_followed_only_whole_and_only_to_a_file_of_yours() {
        let from = crate::fixtures::scratch("seed-paths").join(".claude");
        write(&from.join("statusline.sh"), "echo");
        write(&from.join("hooks/check.sh"), "echo");
        write(&from.join(".credentials.json"), "{}");
        write(&from.join("history.jsonl"), "{}");
        let to = from.with_file_name(".claude-work");
        let tilde = vec![("~/.claude".to_string(), "~/.claude-work".to_string())];
        let follow = |text: &str| follow_paths(text, &tilde, &from, &to).0;
        assert_eq!(follow("bash ~/.claude/statusline.sh"), "bash ~/.claude-work/statusline.sh");
        assert_eq!(follow("\"~/.claude/statusline.sh\" \"~/.claude/statusline.sh\""), "\"~/.claude-work/statusline.sh\" \"~/.claude-work/statusline.sh\"");
        assert_eq!(follow("~/.claude/hooks/check.sh --all"), "~/.claude-work/hooks/check.sh --all");
        assert_eq!(follow("~/.claude/statusline.sh.bak"), "~/.claude/statusline.sh.bak", "a longer name");
        assert_eq!(follow("~/.claude2/statusline.sh"), "~/.claude2/statusline.sh", "another folder");
        assert_eq!(follow("/x~/.claude/statusline.sh"), "/x~/.claude/statusline.sh", "inside a longer path");
        assert_eq!(follow("cat ~/.claude/.credentials.json"), "cat ~/.claude/.credentials.json", "a hidden file");
        assert_eq!(follow("~/.claude/hooks/../statusline.sh"), "~/.claude/hooks/../statusline.sh", "a way out and back");
        assert_eq!(follow("tail ~/.claude/history.jsonl"), "tail ~/.claude/history.jsonl", "a file it reads");
        let whole = vec![(from.to_string_lossy().into_owned(), "/Users/me/.claude-work".to_string())];
        let text = format!("sh {0}/statusline.sh; /opt{0}/statusline.sh", from.display());
        let (text, files) = follow_paths(&text, &whole, &from, &to);
        assert_eq!(text, format!("sh /Users/me/.claude-work/statusline.sh; /opt{}/statusline.sh", from.display()));
        assert_eq!(files, [PathBuf::from("statusline.sh")]);
    }

    #[test]
    fn each_way_of_writing_the_folder_goes_to_the_same_way_of_writing_the_new_one() {
        let home = Path::new("/Users/me");
        let ways = ways(&home.join(".claude"), &home.join(".claude-work"), home);
        let pairs: Vec<(&str, &str)> = ways.iter().map(|(f, t)| (f.as_str(), t.as_str())).collect();
        #[cfg(unix)]
        assert_eq!(
            pairs,
            [
                ("/Users/me/.claude", "/Users/me/.claude-work"),
                ("$CLAUDE_CONFIG_DIR", "$CLAUDE_CONFIG_DIR"),
                ("${CLAUDE_CONFIG_DIR}", "${CLAUDE_CONFIG_DIR}"),
                ("~/.claude", "~/.claude-work"),
                ("$HOME/.claude", "$HOME/.claude-work"),
                ("${HOME}/.claude", "${HOME}/.claude-work")
            ]
        );
        // A new folder outside the home folder is written whole however the old one was.
        let outside = super::ways(&home.join(".claude"), Path::new("/opt/claude-b"), home);
        assert!(outside.iter().skip(3).all(|(_, t)| t == &Path::new("/opt/claude-b").to_string_lossy()));
    }

    #[test]
    fn copies_nothing_into_itself_inside_itself_or_from_nowhere() {
        let home = crate::fixtures::scratch("seed-self");
        let from = home.join(".claude-a");
        write(&from.join("skills/review/SKILL.md"), "review");
        assert!(copy_in(&from, &from, &home).is_err());
        assert!(copy_in(&from, &from.join("skills/b"), &home).is_err(), "it would copy what it copies");
        assert!(copy_in(&home.join(".claude-none"), &home.join(".claude-b"), &home).is_err());
        assert!(!home.join(".claude-b").exists());
        assert!(!from.join("skills/b").exists());
        // A skills folder that's a link to one holding the new account.
        #[cfg(unix)]
        {
            let linked = home.join(".claude-c");
            write(&home.join("shared/review/SKILL.md"), "review");
            std::fs::create_dir_all(&linked).unwrap();
            crate::os::files::symlink(home.join("shared"), linked.join("skills")).unwrap();
            assert!(copy_in(&linked, &home.join("shared/new"), &home).is_err());
            assert!(!home.join("shared/new").exists());
        }
    }

    #[test]
    fn settings_it_cant_read_are_said_not_skipped() {
        let home = crate::fixtures::scratch("seed-unreadable");
        let (from, to) = (home.join(".claude-a"), home.join(".claude-b"));
        std::fs::create_dir_all(&from).unwrap();
        std::fs::write(from.join("settings.json"), [0xff, 0xfe, b'{']).unwrap();
        assert!(copy_in(&from, &to, &home).is_err());
        assert!(!to.join("settings.json").exists());
    }
}
