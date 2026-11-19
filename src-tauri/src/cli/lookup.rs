//! What a command's words name: a session by its key, its id, the start of either, `me` or a
//! link to it (`resolve_session`, and `resolve_any_session` where it may be on another of your
//! Macs), and a worktree by its path or `.` (`resolve_worktree`). `all_sessions` is every session
//! on a board with the worktree it's listed under.
//!
//! A handler takes what it was given through these before it acts, so every command names things
//! the same way and says the same when a name fits none or several. Todos and PRs are named in
//! their own files (`resolve_todo`, `resolve_pr`).

use std::collections::HashSet;
use std::path::Path;

use anyhow::{anyhow, bail, Result};

use crate::model::{Board, Lane, Session};
use crate::start;
use crate::util::{expand, tilde};

use super::*;

/// Every session once, with the lane it's listed under as its own (not as a visitor).
pub(super) fn all_sessions(board: &Board) -> Vec<(&Session, &Lane)> {
    all_in(&board.lanes)
}

/// `all_sessions` of these worktrees: one listed only where it visited, under that one.
pub(super) fn all_in(lanes: &[Lane]) -> Vec<(&Session, &Lane)> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for lane in lanes {
        for s in lane.sessions.iter().filter(|s| s.visitor.is_none()) {
            if seen.insert(s.key.clone()) {
                out.push((s, lane));
            }
        }
    }
    for lane in lanes {
        for s in lane.sessions.iter().filter(|s| s.visitor.is_some()) {
            if seen.insert(s.key.clone()) {
                out.push((s, lane));
            }
        }
    }
    out
}

pub(super) fn resolve_worktree(c: &mut Client, w: &str) -> Result<String> {
    let path = if w == "." { std::env::current_dir()? } else { expand(w) };
    let board = c.loaded_board()?;
    match start::lane_at(board, &path) {
        // A folder inside a worktree is that worktree's only when it's there: a path mistyped
        // (a new worktree's name) would otherwise start in the checkout around it.
        Some(l) if Path::new(&l.id) != path && !path.exists() => bail!("{} isn't there.", tilde(&path)),
        Some(l) if start::unlisted_checkout(board, &path) => bail!("{} is in a checkout the board doesn't list yet, inside {}: a worktree just made shows in Codealong a few seconds on. Try again once `codealong worktrees` lists it.", tilde(&path), tilde(Path::new(&l.path))),
        Some(l) => Ok(l.id.clone()),
        None if board.loading => bail!("Codealong is still reading its worktrees after a start, so whether {} is one can't be said yet: try again in a moment.", tilde(&path)),
        None => bail!("{} isn't a worktree the board knows. `codealong worktrees` lists them.", tilde(&path)),
    }
}

pub(super) fn resolve_session(c: &mut Client, k: &str) -> Result<String> {
    if k == "me" {
        let loading = c.loaded_board()?.loading;
        return find_me(c)?.0.ok_or_else(|| match loading {
            true => anyhow!("Codealong is still reading its sessions after a start, so which one runs this command can't be said yet: try again in a moment."),
            false => anyhow!("This shell isn't running inside a session the board knows, so `me` names nothing. Give a session key."),
        });
    }
    if k.trim().is_empty() {
        bail!("Give a session key.");
    }
    session_named(c.loaded_board()?, k)
}

/// `resolve_session`, then among your other Macs' sessions (`session_named_anywhere`), for a
/// command whose request goes to the Mac the session is on (actions.rs `run_as`).
pub(super) fn resolve_any_session(c: &mut Client, k: &str) -> Result<String> {
    if k == "me" || k.trim().is_empty() {
        return resolve_session(c, k);
    }
    let elsewhere = c.elsewhere().to_vec();
    session_named_anywhere(c.loaded_board()?, &elsewhere, k)
}

/// What a link to a session or a todo names (`codealong://session/<key>`, `kind` its `session`
/// or `todo`), as the skill has an agent write one and a window draws it
/// (src/codealongLinks.ts), else the text as it is: a link read in a conversation names what the
/// key in it does.
pub(super) fn named_by<'a>(text: &'a str, kind: &str) -> &'a str {
    let t = text.trim();
    let scheme = format!("codealong://{kind}/");
    match t.get(..scheme.len()) {
        Some(start) if start.eq_ignore_ascii_case(&scheme) => t[scheme.len()..].split(['?', '#']).next().unwrap_or_default().trim_end_matches('/'),
        _ => text,
    }
}

/// What names a session in `k`: the text, or what a link to a session holds (`named_by`). A
/// link that holds nothing names none: every key starts with nothing, and a board with one
/// session would have it be that one.
pub(super) fn session_in(k: &str) -> Result<&str> {
    let named = named_by(k, "session");
    if named.trim().is_empty() {
        bail!("Give a session key.");
    }
    Ok(named)
}

/// The key of the session `k` names on `board`: its key or id, or the start of only one's, as
/// they are or in a link to it (`session_in`).
fn session_named(board: &Board, k: &str) -> Result<String> {
    let k = session_in(k)?;
    let hits = named(all_sessions(board).into_iter().map(|(s, _)| s), k);
    match hits.len() {
        1 => Ok(hits[0].key.clone()),
        0 => Err(no_session(board, k)),
        n => bail!("{n} sessions start with {k}; give more of the id."),
    }
}

/// `session_named`, and where `board` has none that `k` names, the one among your other Macs'
/// sessions (`elsewhere`) that it names: by its key there (`@<short>:…`), its id or their start.
/// One here goes first, so a start that names several here never names one there instead.
fn session_named_anywhere(board: &Board, elsewhere: &[Lane], k: &str) -> Result<String> {
    let k = session_in(k)?;
    if !named(all_sessions(board).into_iter().map(|(s, _)| s), k).is_empty() {
        return session_named(board, k);
    }
    let there = named(all_in(elsewhere).into_iter().map(|(s, _)| s), k);
    match there.len() {
        0 => Err(no_session(board, k)),
        1 => Ok(there[0].key.clone()),
        n => bail!("{n} sessions on your other machines start with {k}; give more of the id."),
    }
}

/// The sessions `k` names among `all`: the one whose key or id it is, else those whose id or key
/// starts with it, each once.
fn named<'a>(all: impl Iterator<Item = &'a Session>, k: &str) -> Vec<&'a Session> {
    let all: Vec<&Session> = all.collect();
    if let Some(s) = all.iter().find(|s| s.key == k || s.id == k) {
        return vec![*s];
    }
    let mut seen = HashSet::new();
    all.into_iter().filter(|s| (s.id.starts_with(k) || s.key.starts_with(k) || s.short_id == k) && seen.insert(s.key.as_str())).collect()
}


/// Why `board` has no session `k`: there's none, or the board is the app's before its first scan
/// after a start has ended, which lists none yet. An agent told the key is wrong drops the
/// session, where it should try again.
pub(super) fn no_session(board: &Board, k: &str) -> anyhow::Error {
    match board.loading {
        true => anyhow!("Codealong is still reading its sessions after a start, so whether there's a session {k} can't be said yet: try again in a moment."),
        false => anyhow!("There's no session {k}. `codealong sessions` lists them."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::lane;
    use serde_json::{json, Value};
    use crate::config::Config;

    /// The command names your other Macs' sessions as the windows show them, by their key there or
    /// their id, after this Mac's own, for the commands whose request goes to the session's Mac.
    #[test]
    fn a_session_on_another_mac_is_named_after_this_macs() {
        let with_id = |key: &str, id: &str| Session { id: id.into(), short_id: id[..4].into(), ..crate::fixtures::session(key) };
        let here = Board { lanes: vec![lane("/work/atlas", vec![with_id("claude:claude:abc123", "abc123")])], ..Board::default() };
        let visiting = Session { visitor: Some("@a1b2c3d4e5:/work/home".into()), ..with_id("@a1b2c3d4e5:claude:claude:vvv222", "vvv222") };
        let there = vec![lane("@a1b2c3d4e5:/work/kestrel", vec![with_id("@a1b2c3d4e5:claude:claude:abd999", "abd999"), with_id("@a1b2c3d4e5:codex:codex:fff111", "fff111"), visiting])];
        let mut c = Client { app: None, verbose: false, cfg: None, board: Some(here), from_app: false, started: false, said: false, me: Some((None, None)), elsewhere: Some(there), veiled: None };
        assert_eq!(resolve_any_session(&mut c, "@a1b2c3d4e5:claude:claude:abd999").unwrap(), "@a1b2c3d4e5:claude:claude:abd999");
        assert_eq!(resolve_any_session(&mut c, "fff1").unwrap(), "@a1b2c3d4e5:codex:codex:fff111");
        // One here goes first: a start that names this Mac's never names another's instead.
        assert_eq!(resolve_any_session(&mut c, "ab").unwrap(), "claude:claude:abc123");
        assert_eq!(resolve_any_session(&mut c, "@a1b2c3d4e5:").unwrap_err().to_string(), "3 sessions on your other machines start with @a1b2c3d4e5:; give more of the id.");
        // One listed there only under a worktree it visited is named all the same.
        assert_eq!(resolve_any_session(&mut c, "vvv2").unwrap(), "@a1b2c3d4e5:claude:claude:vvv222");
        assert_eq!(resolve_any_session(&mut c, "nope").unwrap_err().to_string(), "There's no session nope. `codealong sessions` lists them.");
        // A link to one names it as the key in it does, here or there, by the start of an id too.
        assert_eq!(resolve_any_session(&mut c, "codealong://session/claude:claude:abc123").unwrap(), "claude:claude:abc123");
        assert_eq!(resolve_session(&mut c, " Codealong://Session/abc1/ ").unwrap(), "claude:claude:abc123");
        assert_eq!(resolve_any_session(&mut c, "codealong://session/@a1b2c3d4e5:codex:codex:fff111?x=1").unwrap(), "@a1b2c3d4e5:codex:codex:fff111");
        assert_eq!(resolve_any_session(&mut c, "codealong://session/nope").unwrap_err().to_string(), "There's no session nope. `codealong sessions` lists them.");
        // One that holds no key names none, though every key starts with nothing and this Mac
        // has one session.
        assert_eq!(resolve_session(&mut c, "codealong://session/").unwrap_err().to_string(), "Give a session key.");
        assert_eq!(resolve_any_session(&mut c, "codealong://session//?x").unwrap_err().to_string(), "Give a session key.");
        // A todo's link names no session, nor a device's link code.
        assert!(resolve_any_session(&mut c, "codealong://todo/abc123").is_err() && resolve_any_session(&mut c, "codealong://link/abc123").is_err());
        assert_eq!((named_by("codealong://todo/t-1#top", "todo"), named_by("t-1", "todo"), named_by("codealong://session/t-1", "todo")), ("t-1", "t-1", "codealong://session/t-1"));
        // The commands that work on this Mac alone don't look there.
        assert!(resolve_session(&mut c, "fff1").is_err());
        let found = c.session_anywhere("@a1b2c3d4e5:codex:codex:fff111").unwrap().map(|(s, l)| (s.key, l.id));
        assert_eq!(found, Some(("@a1b2c3d4e5:codex:codex:fff111".to_string(), "@a1b2c3d4e5:/work/kestrel".to_string())));
        let Cmd::Sessions(s) = Cli::try_parse_from(["codealong", "sessions", "--devices", "--label", "release"]).unwrap().cmd else { panic!() };
        assert!(s.filter.devices && s.filter.labels == ["release"]);
        // What `session label` says it wears: what its Mac answered, or nothing it didn't say.
        assert_eq!(labels_said(&json!({ "labels": { "claude:claude:a": ["Bug"] } }), "claude:claude:a"), Some(vec!["Bug".to_string()]));
        assert_eq!(labels_said(&json!({ "labels": { "claude:claude:a": [] } }), "claude:claude:a"), Some(vec![]));
        for unsaid in [Value::Null, json!({ "labels": null }), json!({ "labels": {} })] {
            assert_eq!(labels_said(&unsaid, "claude:claude:a"), None, "{unsaid}");
        }
    }

    /// Right after a relaunch the app's board lists nothing until its first scan ends, and an
    /// orchestrator told "There's no session" for one `codealong changes` had just listed dropped
    /// it (2026-10-16 to 10-22). A board still loading says so; a whole one says there's none.
    #[test]
    fn a_lookup_tells_the_app_still_reading_from_no_such_session() {
        let listed = Board { lanes: vec![lane("/work/atlas", vec![crate::fixtures::session("claude:claude:abc123")])], ..Board::default() };
        let client = |board: Board| Client { app: None, verbose: false, cfg: None, board: Some(board), from_app: false, started: false, said: false, me: Some((None, None)), elsewhere: None, veiled: None };
        let mut c = client(listed.clone());
        assert_eq!(resolve_session(&mut c, "claude:claude:abc").unwrap(), "claude:claude:abc123");
        assert_eq!(resolve_session(&mut c, "claude:claude:nope").unwrap_err().to_string(), "There's no session claude:claude:nope. `codealong sessions` lists them.");
        assert!(resolve_session(&mut c, "me").unwrap_err().to_string().contains("`me` names nothing"));
        assert!(resolve_worktree(&mut c, "/work/kestrel").unwrap_err().to_string().contains("isn't a worktree the board knows"));
        let mut c = client(Board { loading: true, ..Board::default() });
        for (said, by) in [
            (resolve_session(&mut c, "claude:claude:abc123"), "whether there's a session claude:claude:abc123 can't be said yet"),
            (resolve_session(&mut c, "me"), "which one runs this command can't be said yet"),
            (resolve_worktree(&mut c, "/work/atlas"), "whether /work/atlas is one can't be said yet"),
        ] {
            let said = said.unwrap_err().to_string();
            assert!(said.starts_with("Codealong is still reading its ") && said.contains(by) && said.ends_with("try again in a moment."), "{said}");
        }
        // Found is found, loading or not.
        let mut c = client(Board { loading: true, ..listed });
        assert_eq!(resolve_session(&mut c, "claude:claude:abc123").unwrap(), "claude:claude:abc123");
        // `me` from the environment names a session the board doesn't list yet.
        let mut c = Client { cfg: Some(Config::default()), me: Some((Some("claude:claude:kestrel".into()), Some("environment"))), ..client(Board { loading: true, ..Board::default() }) };
        let said = session_show(&mut c, "me", true).unwrap_err().to_string();
        assert!(said.starts_with("Codealong is still reading its sessions") && said.contains("claude:claude:kestrel"), "{said}");
    }
}
