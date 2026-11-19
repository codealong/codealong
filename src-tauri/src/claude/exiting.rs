//! Claude Code's "Exiting worktree session" dialog: what `/exit` asks before it exits a session
//! that holds a worktree it made for itself (`claude -w`), when removing the worktree would lose
//! something (commits on its branch, uncommitted files, a name you gave the session). Mark as
//! done types `/exit` into such a session so its worktree goes with it (idle.rs `exit`), so it's
//! Codealong that brings the dialog up more often than not. Of a worktree a `WorktreeCreate` hook
//! made it asks whatever is in it, and Mark as done then answers Remove itself where that's all it
//! says (`asks_blindly`) and Codealong's own check finds nothing would be lost (`remove`, idle.rs
//! `answered_blindly`). From 2.1.322's code, and in tmux (2026-11-10):
//!
//! - While it's up, the session's registry entry says `status: "waiting"`, `waitingFor: "dialog
//!   open"`, as it does for any command's dialog (`/model`, `/config`…), and its transcript says
//!   nothing: `/exit` is written down only once its dialog closes, and not at all when it's
//!   called off. So which dialog is up is read off its terminal (`launch::screen_of`), or, where
//!   that can't be read, known from Codealong having typed the `/exit` (`idle::exit_asked`)
//!   before the session began to wait.
//! - It shows its title, what removing would lose in a sentence that wraps, and its choices
//!   numbered, each followed by what it does, the cursor `❯` on Keep, then "Enter to confirm ·
//!   Esc to cancel":
//!
//!   ```text
//!      Exiting worktree session
//!      You have 1 commit on worktree-kestrel. The branch will be deleted if you remove the worktree.
//!      ❯ 1. Keep worktree    Stays at /Users/me/projects/northwind-app/.claude/worktrees/kestrel
//!        2. Remove worktree  All changes and commits will be lost.
//!      Enter to confirm · Esc to cancel
//!   ```
//!
//!   One started with `--tmux` offers three: "Keep worktree and tmux session", "Keep worktree,
//!   end tmux session" and "Remove worktree and tmux session".
//! - A choice's digit picks it at once, without Return: it keeps the worktree or removes it
//!   ("Removing worktree…" for as long as deleting its files takes), then exits. Escape calls the
//!   exit off: the session is back at its prompt, idle.

use std::path::Path;

use anyhow::{anyhow, bail, Result};
use codealong_peer::api::Choice;
use serde_json::Value;

use crate::harness::{Ctx, Pending, Question};
use crate::launch::Key;
use crate::model::Session;

use super::dialog::Dialog;
use super::registry::{RegistryEntry, DIALOG_OPEN};

/// The dialog's title, which starts it on the screen.
const TITLE: &str = "Exiting worktree session";

/// How the id of the dialog's question starts: its process and when it began waiting follow, so
/// an answer never goes to a dialog that came up after the one it was meant for.
pub const EXIT_ID: &str = "exit-worktree:";

/// What it says of a worktree a `WorktreeCreate` hook made, as its only reason to ask: it has no
/// commit the worktree started from to count what was added since (2.1.326's code; the words a
/// terminal showed on 2026-11-18). Anything else it would say comes beside it.
const BLIND: &str = "A WorktreeCreate hook made this worktree, so Claude Code can't tell whether it has unpushed commits.";

/// The dialog at the foot of a terminal's text (`launch::screen_of`), when that's what's up, read
/// from under its title as any of Claude Code's dialogs is (dialog.rs `titled`).
pub fn read(screen: &str) -> Option<Dialog> {
    super::dialog::titled(screen, TITLE)
}

/// The dialog as Codealong knows it without reading it, where its terminal can't be read and
/// Codealong typed the `/exit`: what one not started with `--tmux` offers. Its terminal says
/// what removing would lose. In one started with it, a 2 keeps the worktree.
fn assumed(worktree: &str) -> Dialog {
    let name = crate::util::file_name(Path::new(worktree));
    Dialog {
        title: TITLE.into(),
        says: format!("Claude Code asks whether to keep its worktree, {name}, before it exits: removing it would lose something, which its terminal says."),
        choices: vec![
            Choice { label: "Keep worktree".into(), description: Some(format!("Stays at {worktree}")) },
            Choice { label: "Remove worktree".into(), description: Some("Its folder and branch are deleted, with what's only there.".into()) },
        ],
        on: 0,
    }
}

/// Whether `dialog`, read off its terminal, asks only because a hook made the worktree at `own`:
/// it says `BLIND` and nothing more (a name you gave the session, say, would be said too), and
/// its Keep leaves that worktree where it is, not another the session went on to. Lines that
/// wrapped were joined with a space wherever they broke, mid-path too, so a path with no space
/// of its own is matched without the spaces; one with spaces only as it is. One started with
/// `--tmux` never asks this way: its choices are about its tmux session too.
fn asks_blindly(dialog: &Dialog, own: &str) -> bool {
    let squeezed = |text: &str| text.split_whitespace().collect::<String>();
    let stays = format!("Stays at {own}");
    let names_own = |d: &str| d == stays || (!own.contains(char::is_whitespace) && squeezed(d) == squeezed(&stays));
    let keeps_own = |c: &Choice| c.label == "Keep worktree" && c.description.as_deref().is_some_and(names_own);
    squeezed(&dialog.says) == squeezed(BLIND) && dialog.choices.iter().any(keeps_own)
}

/// The dialog up in `s` and its registry entry, when it asks only because a hook made `s`'s own
/// worktree (`asks_blindly`). Only one read off its terminal: one Codealong couldn't read
/// (`assumed`) has choices it only supposes.
fn up_blindly(home: &Path, s: &Session) -> Option<(Dialog, RegistryEntry)> {
    let own = s.own_worktree.as_deref()?;
    let (reg, screen) = super::dialog::open(home, s)?;
    let dialog = read(screen.as_deref()?)?;
    asks_blindly(&dialog, own).then_some((dialog, reg))
}

/// Whether the dialog up in `s` asks only because a hook made its worktree (`asks_blindly`).
pub fn blindly(ctx: &Ctx, s: &Session) -> bool {
    up_blindly(&ctx.account.home_path(), s).is_some()
}

/// When the dialog the registry entry `reg` says is open came up, if it says one is.
fn dialog_since(reg: &RegistryEntry) -> Option<i64> {
    reg.status_updated_at.filter(|_| reg.waits_on(DIALOG_OPEN))
}

/// The account's registry as the board reads it: what the sessions Codealong typed an exit into
/// wait on now, so an exit's question is known by the dialog it brought up and no other
/// (`idle::exit_dialog_seen`).
pub fn saw(account: &str, entries: &[RegistryEntry]) {
    let now = crate::util::now_ms();
    for reg in entries {
        let key = Session::key_for(crate::model::Harness::Claude, account, &reg.session_id);
        if crate::idle::exit_asked(&key).is_some_and(|a| a.pid == reg.pid) {
            crate::idle::exit_dialog_seen(&key, dialog_since(reg), now);
        }
    }
}

/// The dialog up in the live session `s`, whose registry entry `reg` says a dialog is open
/// (`dialog::open`, which leaves out a background job: one is stopped rather than exited): what
/// its terminal shows (`screen`), or, where that can't be read, the one the `/exit` Codealong
/// typed brought up.
fn up(s: &Session, reg: &RegistryEntry, screen: Option<&str>) -> Option<Dialog> {
    if let Some(text) = screen {
        return read(text);
    }
    let since = dialog_since(reg)?;
    // The registry as read now, should the board not have read it since it changed.
    crate::idle::exit_dialog_seen(&s.key, Some(since), crate::util::now_ms());
    crate::idle::exit_asked(&s.key).filter(|a| a.pid == reg.pid && a.dialog == Some(since))?;
    Some(assumed(s.own_worktree.as_deref()?))
}

/// The id an answer names the dialog up in its registry entry `reg` by.
fn id_of(reg: &RegistryEntry) -> String {
    format!("{EXIT_ID}{}:{}", reg.pid, reg.status_updated_at.unwrap_or(0))
}

fn question_of(dialog: &Dialog) -> Question {
    let says = if dialog.says.is_empty() { "Keep its worktree?".to_string() } else { dialog.says.clone() };
    Question { question: says, header: Some("Exiting".into()), options: dialog.choices.clone(), multi_select: false }
}

/// The dialog up in `s`, as a question to answer with one of its choices (`Pending::exiting`):
/// `reg` and `screen` as `dialog::open` read them.
pub fn pending(ctx: &Ctx, s: &Session, reg: &RegistryEntry, screen: Option<&str>) -> Option<Pending> {
    let dialog = up(s, reg, screen)?;
    Some(Pending {
        id: id_of(reg),
        label: TITLE.into(),
        input: None,
        cannot: super::cannot_press(ctx, reg),
        always: false,
        no_once: false,
        plan: false,
        message: false,
        from_key: None,
        always_rule: None,
        questions: vec![question_of(&dialog)],
        unwritten: false,
        terminal: None,
        exiting: true,
        dialog: false,
    })
}

/// The dialog `id` names, if it's still the one up in `s`.
fn still_up(ctx: &Ctx, s: &Session, id: &str) -> Result<(Dialog, RegistryEntry)> {
    let gone = || anyhow!("It isn't asking about its worktree any more.");
    let (reg, screen) = super::dialog::open(&ctx.account.home_path(), s).ok_or_else(gone)?;
    let dialog = up(s, &reg, screen.as_deref()).ok_or_else(gone)?;
    if id_of(&reg) != id {
        bail!("It's asking something else now. Look again before answering.");
    }
    Ok((dialog, reg))
}

/// Picks the choice `answers` names (its question's, keyed by the question as `pending` gave
/// it) by its digit.
pub fn answer(ctx: &Ctx, s: &Session, id: &str, answers: &Value) -> Result<String> {
    let (dialog, reg) = still_up(ctx, s, id)?;
    let labels = || dialog.choices.iter().map(|c| c.label.as_str()).collect::<Vec<_>>().join(" or ");
    let picked = answers.as_object().and_then(|a| a.values().next()).and_then(Value::as_str).ok_or_else(|| anyhow!("Pick {} first.", labels()))?;
    let Some(i) = dialog.choices.iter().position(|c| c.label == picked) else {
        bail!("It takes one of its choices, nothing else: {}.", labels());
    };
    pick(ctx, s, &reg, i)?;
    Ok(format!("Picked {picked}."))
}

/// Answers the dialog up in `s` with Remove worktree, as Mark as done does where it asks only
/// because a hook made the worktree (`asks_blindly`, looked at again here as it's pressed) and
/// Codealong found nothing in it would be lost.
pub fn remove(ctx: &Ctx, s: &Session) -> Result<()> {
    let (dialog, reg) = up_blindly(&ctx.account.home_path(), s).ok_or_else(|| anyhow!("It isn't asking only because a hook made its worktree."))?;
    let i = dialog.choices.iter().position(|c| c.label == "Remove worktree").ok_or_else(|| anyhow!("None of its choices is Remove worktree."))?;
    pick(ctx, s, &reg, i)
}

/// Picks choice `i` of the dialog up in its registry entry `reg` by its digit.
fn pick(ctx: &Ctx, s: &Session, reg: &RegistryEntry, i: usize) -> Result<()> {
    let digit = u8::try_from(i + 1).ok().filter(|n| *n <= 9).ok_or_else(|| anyhow!("It offers more choices than Codealong picks among with a key. Answer it in its terminal."))?;
    super::press_in(&ctx.account.home_path(), s, reg, Key::Digit(digit), "Open it to answer.")?;
    Ok(())
}

/// Calls the exit off, as Escape does: the session is back at its prompt.
pub fn decline(ctx: &Ctx, s: &Session, id: &str) -> Result<String> {
    let (_, reg) = still_up(ctx, s, id)?;
    super::press_in(&ctx.account.home_path(), s, &reg, Key::Escape, "Open it to answer.")?;
    Ok("Called its exit off: it's back at its prompt.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The dialog as tmux showed it (2026-11-10), with the frame above it.
    const SCREEN: &str = "\
 ▐▛███▛█   Claude Code v2.1.322
▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔
   Exiting worktree session
   You have 1 commit on worktree-kestrel. The branch will be deleted if you remove the worktree.
   ❯ 1. Keep worktree    Stays at /Users/me/projects/northwind-app/.claude/worktrees/kestrel
     2. Remove worktree  All changes and commits will be lost.
   Enter to confirm · Esc to cancel

";

    #[test]
    fn reads_what_it_says_and_its_choices() {
        let d = read(SCREEN).unwrap();
        assert_eq!(d.says, "You have 1 commit on worktree-kestrel. The branch will be deleted if you remove the worktree.");
        assert_eq!(
            d.choices,
            [
                Choice { label: "Keep worktree".into(), description: Some("Stays at /Users/me/projects/northwind-app/.claude/worktrees/kestrel".into()) },
                Choice { label: "Remove worktree".into(), description: Some("All changes and commits will be lost.".into()) },
            ]
        );
    }

    #[test]
    fn joins_lines_that_wrapped() {
        let narrow = "   Exiting worktree session\n   You have 2 uncommitted files and 1 commit on\n   worktree-kestrel. All will be lost if you remove.\n   ❯ 1. Keep worktree    Stays at\n                         /Users/me/projects/northwind-app/.claude/worktrees/kestrel\n     2. Remove worktree  All changes and commits will be lost.\n   Enter to confirm · Esc to cancel\n";
        let d = read(narrow).unwrap();
        assert_eq!(d.says, "You have 2 uncommitted files and 1 commit on worktree-kestrel. All will be lost if you remove.");
        assert_eq!(d.choices[0].description.as_deref(), Some("Stays at /Users/me/projects/northwind-app/.claude/worktrees/kestrel"));
    }

    #[test]
    fn reads_the_three_choices_of_one_started_in_tmux() {
        let tmux = "Exiting worktree session\nThis session was named \"atlas\". Keep the worktree to resume it later, or remove it to clean up.\n❯ 1. Keep worktree and tmux session  Stays at /w. Reattach with: tmux attach -t atlas\n  2. Keep worktree, end tmux session  Keeps worktree at /w, terminates tmux session.\n  3. Remove worktree and tmux session  Clean up the worktree directory.\nEnter to confirm · Esc to cancel";
        let labels: Vec<_> = read(tmux).unwrap().choices.into_iter().map(|c| c.label).collect();
        assert_eq!(labels, ["Keep worktree and tmux session", "Keep worktree, end tmux session", "Remove worktree and tmux session"]);
    }

    #[test]
    fn isnt_one_that_has_gone_or_another_dialog() {
        // Scrolled up, with the session's prompt under it.
        assert_eq!(read(&format!("{SCREEN}Worktree kept.\n❯ \n")), None);
        assert_eq!(read("   Select model\n   ❯ 1. Default  Opus\n     2. Haiku  Fastest\n   Enter to confirm · Esc to cancel\n"), None);
        assert_eq!(read("   Exiting worktree session\n   Removing worktree…\n"), None);
    }

    #[test]
    fn asks_blindly_only_of_the_hooks_worktree_and_for_no_other_reason() {
        let own = "/Users/me/projects/northwind-app/.claude/worktrees/kestrel";
        // As a terminal showed it on 2026-11-18, the worktree a hook made.
        let hooked = format!("   Exiting worktree session\n   {BLIND}\n   ❯ 1. Keep worktree    Stays at {own}\n     2. Remove worktree  Any unpushed commits in the worktree may be lost.\n   Enter to confirm · Esc to cancel\n");
        let d = read(&hooked).unwrap();
        assert!(asks_blindly(&d, own));
        // Narrower, its sentence and its path wrapped.
        let narrow = format!("   Exiting worktree session\n   A WorktreeCreate hook made this worktree, so Claude Code can't tell\n   whether it has unpushed commits.\n   ❯ 1. Keep worktree    Stays at /Users/me/projects/northwind-app/.claude/\n                         worktrees/kestrel\n     2. Remove worktree  Any unpushed commits in the worktree may be lost.\n   Enter to confirm · Esc to cancel\n");
        assert!(asks_blindly(&read(&narrow).unwrap(), own));
        // Another worktree than the one Codealong checked, another reason besides, or one that
        // counted what would be lost.
        assert!(!asks_blindly(&d, "/Users/me/projects/northwind-app/.claude/worktrees/atlas"));
        let named = hooked.replace(BLIND, &format!("{BLIND} This session was named \"atlas\"."));
        assert!(!asks_blindly(&read(&named).unwrap(), own));
        assert!(!asks_blindly(&read(SCREEN).unwrap(), own));
        // A path with spaces of its own is matched as it is, not squeezed into another's.
        let spaced = "/Users/me/projects/northwind-app/.claude/worktrees/release notes";
        assert!(asks_blindly(&read(&hooked.replace(own, spaced)).unwrap(), spaced));
        assert!(!asks_blindly(&read(&hooked.replace(own, &spaced.replace(' ', ""))).unwrap(), spaced));
        // Nor one in tmux, whose answer decides its tmux session too. (One Codealong couldn't
        // read never comes here: `up_blindly` reads it off the screen or not at all.)
        let tmux = format!("Exiting worktree session\n{BLIND}\n❯ 1. Keep worktree and tmux session  Stays at {own}. Reattach with: tmux attach -t kestrel\n  2. Keep worktree, end tmux session  Keeps worktree at {own}, terminates tmux session.\n  3. Remove worktree and tmux session  Clean up the worktree directory.\nEnter to confirm · Esc to cancel");
        assert!(!asks_blindly(&read(&tmux).unwrap(), own));
    }

    #[test]
    fn says_what_it_can_where_it_cant_read_the_terminal() {
        let d = assumed("/Users/me/projects/northwind-app/.claude/worktrees/kestrel");
        assert!(d.says.contains("kestrel"));
        assert_eq!(d.choices.iter().map(|c| c.label.as_str()).collect::<Vec<_>>(), ["Keep worktree", "Remove worktree"]);
        let q = question_of(&d);
        assert_eq!((q.header.as_deref(), q.multi_select), (Some("Exiting"), false));
    }
}
