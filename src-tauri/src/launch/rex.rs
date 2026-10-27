//! Rex (superlogical.com/rex), a terminal and multiplexer in one, through its command, `rex`. A
//! server of Rex's own runs every terminal and keeps it running when the app quits; the app is
//! a client of it. What the server holds: sessions, each a group of windows (which the app shows
//! as tabs), each a layout of blocks, a terminal each. Every block's program runs under the
//! server on a TTY of its own (rex → login → zsh), so a session's block is found by its
//! process's TTY, as iTerm2's and Terminal's tabs are, and is then addressed by its session and
//! block ids. Codealong asks the server through Lua scripts run with `rex do`, one call each.
//!
//! A Codealong window is a Rex session, and a tab a window in one: `tab` goes into the session
//! the app shows (`shown_session`), `project` into the one with terminals in the nearest folders,
//! and otherwise a session of its own, named for its folder. The server holds a session nobody
//! looks at and starts one while the screen is locked, so tmux has nothing to add
//! (`holds_sessions`). Bringing a tab forward asks the app to show its session
//! (`session.select`), which the app does only with Remote Control on in its settings; typing,
//! keys, starting and closing go to the server and need nothing.
//! See .claude/rules/terminals.md.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{anyhow, bail, Result};
use serde::Deserialize;
use serde_json::{json, Value};

use super::{app_active, app_installed, close_tty_terminals, focus_on_tty, launcher, pasted, placement, tty_of, type_on_tty, Closing, HostKind, Key, Placement, Submit, Tab, Terminal, TerminalHost, Typed};
use crate::os::open::{self, App};
use crate::procs;
use crate::util::{home, truncate};

const APP: &str = "Rex";
const BUNDLE: &str = "com.superlogical.rex";

/// The app's names: the beta's, and the one a release is expected to have.
const APPS: [&str; 2] = ["Rex Beta.app", "Rex.app"];

/// How long one `rex` call gets. The server answers in milliseconds; asking the app to show a
/// session waits up to 10 s for its answer.
const TIMEOUT: Duration = Duration::from_secs(30);

/// How long the app gets to connect to its server once Codealong opened it (`show`); a moment
/// on the test stage.
const APP_WAIT: Duration = if cfg!(test) { Duration::from_secs(1) } else { Duration::from_secs(10) };

/// The longest label Rex takes for a session or a window.
const LABEL_CHARS: usize = 128;

/// What Rex says when its app won't take an action, the way Codealong says it.
const NO_REMOTE_CONTROL: &str = "Rex is in front, but it goes to this session's tab only with Remote Control on (Rex › Settings › Rex Server).";

pub struct Rex;

impl Terminal for Rex {
    fn kind(&self) -> HostKind {
        HostKind::Rex
    }

    fn name(&self) -> &'static str {
        APP
    }

    fn profile(&self) -> Option<&'static str> {
        Some("rex")
    }

    fn bundle(&self) -> Option<&'static str> {
        Some(BUNDLE)
    }

    fn installed(&self) -> bool {
        APPS.iter().any(|app| app_installed(app))
    }

    /// Rex's server starts each block's program, under `login` for a shell, and carries on when
    /// the app quits.
    fn runs_as(&self, process: &str) -> bool {
        process == "rex"
    }

    /// Its server keeps every terminal running with no window showing it, and makes them with no
    /// display to draw in, so while the screen is locked too.
    fn holds_sessions(&self) -> bool {
        true
    }

    fn reachable(&self) -> bool {
        cli().is_some()
    }

    fn run(&self, place: Placement, dir: &Path, input: &str, env: &[(String, String)]) -> Result<()> {
        open_in(place, dir, input, env)
    }

    fn tabs(&self) -> Result<Vec<Tab>> {
        Ok(blocks()?.iter().map(Block::tab).collect())
    }

    fn focus(&self, at: &str) -> Result<bool> {
        let Some(b) = block_on(at)? else { return Ok(false) };
        show(json!({ "session": b.session, "window": b.window, "block": b.block }))?;
        Ok(true)
    }

    fn focus_session(&self, _profile: &str, _pid: u32, tty: &Path, _title: &str, _cwd: &Path) -> Result<Option<TerminalHost>> {
        focus_on_tty(&Rex, tty)
    }

    fn type_session(&self, pid: u32, _cwd: &Path, text: &str, clear: bool, submit: Submit, _passes: bool) -> Result<Typed> {
        type_on_tty(APP, pid, text, clear.then_some(press), |tty, text| type_text(tty, text, submit))
    }

    fn presses_keys(&self) -> bool {
        true
    }

    fn key_target(&self, pid: u32, _cwd: &Path) -> Result<Option<String>> {
        Ok(tty_of(pid).map(|t| t.to_string_lossy().into_owned()))
    }

    fn press(&self, at: &str, key: Key) -> Result<()> {
        press(at, key)
    }

    fn closes_tabs(&self) -> bool {
        true
    }

    fn close_tabs(&self, tabs: &[&Closing]) -> Result<Vec<Result<()>>> {
        close_tty_terminals(tabs, APP, ttys, in_front, close)
    }
}

/// Every terminal block in every session: its ids, its program's pid (`login`, or the command
/// it runs), the folder its foreground process is in, its title, and whether it's the one with
/// focus in its session's active window. A script hears a session only once attached to it.
const LIST: &str = r#"
local terminal = "com.superlogical.terminal"
local out = {}
local listed = rex.call("session.list") or {}
for _, s in ipairs(listed.sessions or {}) do
  local sid = s.session_id
  rex.session.attach{ session_id = sid }
  local view = rex.session.view{ session_id = sid } or {}
  local focused = ""
  for _, w in ipairs(view.windows or {}) do
    if w.window_id == view.active_window_id then focused = w.focused_block_id or "" end
  end
  local blocks = rex.session.list_blocks{ session_id = sid } or {}
  for _, b in ipairs(blocks.blocks or {}) do
    if b.creator_name == terminal then
      local at = { session_id = sid, block_id = b.block_id }
      local p = rex.block.call(terminal, "process", at) or {}
      local t = rex.block.call(terminal, "title", at) or {}
      local title = t.title or ""
      if title == "" then title = b.label or "" end
      table.insert(out, {
        session = sid, window = b.window_id or "", block = b.block_id,
        pid = (p.child and p.child.pid) or 0, cwd = (p.foreground and p.foreground.cwd) or "",
        title = title, focused = b.block_id == focused,
      })
    end
  end
end
return out
"#;

/// `{cwd, input, command?, session, session_label?, label?, focus}`: a window holding a terminal
/// in `cwd` that types `input` once its program starts, in `session`, or with "" or a session
/// that's gone since it was seen, in a new session named `session_label`. `command` runs in place
/// of the login shell, with no shell around it. Returns its `{session, window, block}`.
const NEW: &str = r#"
local a = rex.args
local options = { cwd = a.cwd, initial_input = a.input }
if a.command then
  options.command = a.command
  options.shell = "none"
end
local layout = { block = { flavor = "com.superlogical.terminal.shell", options = options } }
if a.session ~= "" and rex.session.attach{ session_id = a.session } == nil then
  a.session = ""
end
if a.session == "" then
  local made, err = rex.call("session.create", { label = a.session_label, initial_windows = { { layout = layout, window_label = a.label } } })
  if not made then error(err, 0) end
  local w = made.initial_windows[1]
  return { session = made.session_id, window = w.window_id, block = w.block_ids[1] }
end
local made, err = rex.session.new_window{ session_id = a.session, layout = layout, window_label = a.label, focus = a.focus }
if not made then error(err, 0) end
return { session = a.session, window = made.window_id, block = made.block_ids[1] }
"#;

/// `{session, window, block}`: focuses the block, which makes its window the session's active
/// one, then asks each Rex app on this Mac to show that session. Returns `{apps, shown, refused,
/// failed}`: how many apps there are and how many showed it, and what one said that refused
/// (Remote Control off) or failed otherwise.
const SHOW: &str = r#"
local a = rex.args
rex.session.attach{ session_id = a.session }
local _, err = rex.session.focus_block{ session_id = a.session, block_id = a.block }
if err then error(err, 0) end
local apps, shown, refused, failed = 0, 0, "", ""
local clients = rex.call("client.list") or {}
for _, c in ipairs(clients.clients or {}) do
  if c.info and c.info.kind == "app" and c.principal and c.principal.transport == "unix" then
    apps = apps + 1
    local _, e = rex.call("client.perform", { client_id = c.client_id, action = "session.select", args = { session_id = a.session, window_id = a.window } })
    if not e then
      shown = shown + 1
    elseif string.find(e, "code=refused", 1, true) then
      refused = e
    else
      failed = e
    end
  end
end
return { apps = apps, shown = shown, refused = refused, failed = failed }
"#;

/// `{session, block, text, after}`: types `text` into the block as if typed, then `after` a
/// moment later unless it's "", as iTerm2's TYPE does: a program reads them apart.
const WRITE: &str = r#"
local a = rex.args
local terminal = "com.superlogical.terminal"
rex.session.attach{ session_id = a.session }
local _, err = rex.block.call(terminal, "write", { session_id = a.session, block_id = a.block, data = a.text })
if err then error(err, 0) end
if a.after ~= "" then
  rex.sleep(0.3)
  _, err = rex.block.call(terminal, "write", { session_id = a.session, block_id = a.block, data = a.after })
  if err then error(err, 0) end
end
return "ok"
"#;

/// `{blocks: [{session, block}]}`: closes them, ending what runs there without asking. Fails
/// with the blocks that wouldn't close.
const CLOSE: &str = r#"
local failed = {}
for _, b in ipairs(rex.args.blocks) do
  rex.session.attach{ session_id = b.session }
  local _, err = rex.call("block.close", { session_id = b.session, block_id = b.block })
  if err then table.insert(failed, err) end
end
if #failed > 0 then error(table.concat(failed, "; "), 0) end
return "ok"
"#;

/// A terminal block as LIST has it, with the TTY its program runs on.
#[derive(Debug, Clone, Deserialize)]
struct Block {
    session: String,
    window: String,
    block: String,
    pid: u32,
    cwd: String,
    title: String,
    focused: bool,
    #[serde(skip)]
    tty: String,
}

impl Block {
    fn tab(&self) -> Tab {
        Tab { id: self.tty.clone(), cwd: self.cwd.clone(), name: self.title.clone() }
    }
}

/// The `rex` that goes with this Mac's server: the server's own, which Rex keeps current, else
/// the one in the app.
fn cli() -> Option<PathBuf> {
    #[cfg(test)]
    if let Some(bin) = super::stage::rex_bin() {
        return bin;
    }
    let server = home().join("Library/Application Support/com.superlogical.rex/RexServer/Versions/Current/Rex Server.app/Contents/MacOS/rex");
    let bundled = APPS.iter().flat_map(|app| [PathBuf::from("/Applications"), home().join("Applications")].map(|dir| dir.join(app).join("Contents/Helpers/Rex Server.rex/Contents/MacOS/rex")));
    std::iter::once(server).chain(bundled).find(|p| p.is_file())
}

/// Runs a Lua script against the server with `args` and returns what it returned. `start` lets
/// `rex` start the server when it isn't running, which only a new terminal needs: with none
/// running there's nothing to find, type into or close.
fn script(code: &str, args: Value, start: bool) -> Result<Value> {
    let bin = cli().ok_or_else(|| anyhow!("Codealong couldn't find Rex's command (rex). Reinstall Rex, or pick another terminal in Settings."))?;
    let bin = bin.to_string_lossy();
    let autostart = format!("--autostart={start}");
    let out = procs::run(&bin, &[&autostart, "do", "-e", code, "--args", &args.to_string(), "--json"], None, TIMEOUT).map_err(|e| said(&bin, &e.to_string()))?;
    serde_json::from_str(&out).map_err(|e| anyhow!("Rex answered in a way Codealong doesn't read ({e})."))
}

/// What `rex` said, without the path `procs::run` puts first and the `Error:` it puts first.
fn said(bin: &str, error: &str) -> anyhow::Error {
    let words = error.strip_prefix(bin).unwrap_or(error).trim();
    let words = words.strip_prefix("Error:").unwrap_or(words).trim();
    if words.starts_with("rex server is not running") {
        return anyhow!("Rex's server isn't running.");
    }
    let first = words.lines().next().unwrap_or(words);
    anyhow!("Rex didn't do it: {first}")
}

/// Every terminal block, each with its TTY. Ones whose program ended (kept open to show how it
/// ended) have none, and aren't listed.
fn blocks() -> Result<Vec<Block>> {
    let listed = script(LIST, json!({}), false)?;
    // An empty Lua table comes back as `{}`.
    let listed: Vec<Block> = if listed.is_array() { serde_json::from_value(listed)? } else { Vec::new() };
    Ok(listed
        .into_iter()
        .filter_map(|mut b| {
            b.tty = tty_of(b.pid).filter(|_| b.pid > 0)?.to_string_lossy().into_owned();
            Some(b)
        })
        .collect())
}

fn block_on(tty: &str) -> Result<Option<Block>> {
    Ok(blocks()?.into_iter().find(|b| b.tty == tty))
}

/// Opens a terminal in `dir` and types `input` there once its program starts. With `env`, its
/// program is the launcher, which sets it and becomes the login shell; Rex's terminals take no
/// environment of their own. See the module header for which session it goes in.
fn open_in(place: Placement, dir: &Path, input: &str, env: &[(String, String)]) -> Result<()> {
    let mut listed = Vec::new();
    let (at, beside) = placement(place, || {
        listed = blocks()?;
        Ok(listed.iter().map(Block::tab).collect())
    });
    let near = beside.lines().find_map(|tty| listed.iter().find(|b| b.tty == tty)).map(|b| b.session.clone());
    // Rex has no order of tabs to put one beside its folder's in, so the front window's tab is
    // simply in the session it shows.
    let session = if at == "front" { shown_session().or(near) } else { near }.unwrap_or_default();
    let named = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| dir.to_string_lossy().into_owned());
    let mut args = json!({ "cwd": dir.to_string_lossy(), "input": input, "session": session, "session_label": truncate(&named, LABEL_CHARS), "focus": !place.behind });
    if let Some(label) = place.label.filter(|l| !l.is_empty()) {
        args["label"] = json!(truncate(label, LABEL_CHARS));
    }
    let script_path = if env.is_empty() { None } else { Some(launcher(dir, env)?) };
    if let Some(path) = &script_path {
        args["command"] = json!(["/bin/sh", path.to_string_lossy()]);
    }
    let made = match script(NEW, args, true) {
        Ok(made) => made,
        Err(e) => {
            // It never ran, so it's still there holding the prompt.
            if let Some(path) = &script_path {
                let _ = std::fs::remove_file(path);
            }
            return Err(e);
        }
    };
    if !place.behind {
        // It started either way; the app not following is only where you look.
        let _ = show(made);
    }
    Ok(())
}

/// The session the Rex app shows, as its own settings note it (`Workspace.lastFocusedSession`,
/// `{"sessionID": …}` held as data), read the way `defaults` reads them. None when it hasn't
/// noted one, or they don't read as that.
fn shown_session() -> Option<String> {
    let exported = procs::run("/usr/bin/defaults", &["export", BUNDLE, "-"], None, Duration::from_secs(5)).ok()?;
    let prefs = plist::Value::from_reader_xml(exported.as_bytes()).ok()?;
    let noted = prefs.as_dictionary()?.get("Workspace.lastFocusedSession")?.as_data()?;
    let noted: Value = serde_json::from_slice(noted).ok()?;
    noted.get("sessionID")?.as_str().map(String::from)
}

/// Shows a block, by its `{session, window, block}` ids, in the Rex app and brings the app
/// forward, opening it when it isn't. The block gets focus in its session either way; the app goes
/// to the session only with Remote Control on.
fn show(ids: Value) -> Result<()> {
    let ask = || -> Result<Shown> { Ok(serde_json::from_value(script(SHOW, ids.clone(), false)?).unwrap_or_default()) };
    let mut shown = ask()?;
    open::in_app(App::Id(BUNDLE), None, Duration::from_secs(10))?;
    // No app was connected: the one just opened is, once it has started.
    let started = std::time::Instant::now();
    while shown.apps == 0 && started.elapsed() < APP_WAIT {
        std::thread::sleep(Duration::from_millis(500));
        shown = ask()?;
    }
    if shown.shown > 0 {
        Ok(())
    } else if shown.apps == 0 {
        bail!("Rex opened, but it didn't connect to its server in time to show this session's tab.")
    } else if !shown.refused.is_empty() {
        bail!(NO_REMOTE_CONTROL)
    } else {
        bail!("Rex is in front, but it didn't go to this session's tab: {}", shown.failed)
    }
}

/// What SHOW answered.
#[derive(Default, Deserialize)]
#[serde(default)]
struct Shown {
    apps: u64,
    shown: u64,
    refused: String,
    failed: String,
}

/// Types `text` into the block on `tty` as a paste, then sends it with `submit`. `Ok(false)` when
/// no block has that TTY, so nothing was typed.
fn type_text(tty: &str, text: &str, submit: Submit) -> Result<bool> {
    let Some(b) = block_on(tty)? else { return Ok(false) };
    let after = match submit {
        Submit::Return => Key::Return.typed(),
        Submit::Keys(keys) => keys,
    };
    script(WRITE, json!({ "session": b.session, "block": b.block, "text": pasted(text), "after": after }), false)?;
    Ok(true)
}

/// Presses one key in the block on `tty`, as if typed: `write` sends the key's bytes as they are.
fn press(tty: &str, key: Key) -> Result<()> {
    let b = block_on(tty)?.ok_or_else(|| anyhow!("That session's Rex tab is gone."))?;
    script(WRITE, json!({ "session": b.session, "block": b.block, "text": key.typed(), "after": "" }), false).map(drop)
}

fn ttys() -> Result<Vec<String>> {
    Ok(blocks()?.into_iter().map(|b| b.tty).collect())
}

/// The tabs you may be in: with Rex the active app, the focused tab of each session, since Rex
/// doesn't say which session its app shows.
fn in_front() -> Result<Vec<String>> {
    if !app_active(BUNDLE) {
        return Ok(Vec::new());
    }
    Ok(blocks()?.into_iter().filter(|b| b.focused).map(|b| b.tty).collect())
}

fn close(ttys: &[&str]) -> Result<()> {
    let listed = blocks()?;
    let closing: Vec<Value> = listed.iter().filter(|b| ttys.contains(&b.tty.as_str())).map(|b| json!({ "session": b.session, "block": b.block })).collect();
    script(CLOSE, json!({ "blocks": closing }), false).map(drop)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::NewTerminal;

    /// Waits up to 10 s for `file` to hold `text`.
    fn holds(file: &Path, text: &str) -> bool {
        (0..50).any(|_| {
            std::thread::sleep(Duration::from_millis(200));
            std::fs::read_to_string(file).is_ok_and(|t| t.contains(text))
        })
    }

    /// Against this Mac's Rex: a terminal in a session of its own, with an environment and a line
    /// typed as it starts, found again by its TTY, typed into, a key pressed there, and closed,
    /// with nothing brought forward. Every script runs on the server as written. Opt in, with Rex
    /// installed: `cargo test rex_round_trip -- --ignored`.
    #[test]
    #[ignore]
    fn rex_round_trip() {
        let dir = std::env::temp_dir().join(format!("codealong-rex-check-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // As its processes have it: /var is /private/var.
        let dir = dir.canonicalize().unwrap();
        let out = dir.join("out");
        let place = Placement { new_terminal: NewTerminal::Window, near: &[], behind: true, label: Some("codealong check") };
        let env = [("CODEALONG_CHECK".to_string(), "it's here".to_string())];
        open_in(place, &dir, "echo \"$CODEALONG_CHECK\" > out\n", &env).unwrap();
        assert!(holds(&out, "it's here\n"), "the environment and the line reached its shell");

        let block = blocks().unwrap().into_iter().find(|b| Path::new(&b.cwd) == dir).expect("its block, listed with its folder");
        assert!(Rex.tabs().unwrap().iter().any(|t| t.id == block.tty));
        assert!(type_text(&block.tty, "echo typed >> out", Submit::Return).unwrap());
        assert!(holds(&out, "typed\n"), "a paste and Return ran there");
        press(&block.tty, Key::CtrlC).unwrap();

        close(&[&block.tty]).unwrap();
        assert!(block_on(&block.tty).unwrap().is_none(), "its block closed");
        // A terminal meant for a session that's gone since gets a session of its own.
        let gone = json!({ "cwd": dir.to_string_lossy(), "input": "", "session": "session:00000000-0000-0000-0000-000000000000", "session_label": "codealong check", "focus": false });
        let made = script(NEW, gone, false).unwrap();
        let own = made["session"].as_str().unwrap_or_default().to_string();
        assert!(own.starts_with("session:") && !own.ends_with("000000000000"), "{made}");
        let bin = cli().unwrap();
        for session in [&block.session, &own] {
            let _ = procs::run(&bin.to_string_lossy(), &["--autostart=false", "session", "kill", session], None, TIMEOUT);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The session this Mac's Rex app notes as the one it shows is one of its server's. Opt in,
    /// with Rex open: `cargo test rex_notes_the_session_it_shows -- --ignored`.
    #[test]
    #[ignore]
    fn rex_notes_the_session_it_shows() {
        let shown = shown_session().expect("Rex's app notes the session it shows");
        assert!(blocks().unwrap().iter().any(|b| b.session == shown), "{shown} is one of the server's sessions");
    }
}
