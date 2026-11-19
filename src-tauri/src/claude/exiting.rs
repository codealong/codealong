//! Claude Code's "Exiting worktree session" dialog: what `/exit` asks before it exits a session
//! that holds a worktree it made for itself (`claude -w`), when it counts something removing the
//! worktree might lose (commits on its branch, uncommitted files, a name you gave the session).
//! Mark as done types `/exit` into such a session so its worktree goes with it (idle.rs `exit`),
//! so it's Codealong that brings the dialog up more often than not. What it counts doesn't say
//! whether anything would be lost: commits are counted from the one the worktree started at,
//! pushed and landed ones too (a branch rebased onto a moved `main` has dozens), and a worktree a
//! `WorktreeCreate` hook made has no such commit, so it asks of that one whatever is in it. So
//! Mark as done answers Remove itself where all it gives as its reason is one of those
//! (`answerable`) and Codealong's own check finds nothing would be lost (`remove`, idle.rs
//! `answered_by_check`): asked 2026-11-18, after a session that had landed everything asked about
//! its 68 commits. Its other reasons (files saved from connectors, submodules it couldn't check, a
//! detached HEAD, a branch other than the one it deletes) are about what that check doesn't look
//! at, so they stay yours. From 2.1.322's code, and in tmux (2026-11-10):
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

/// The choice that removes the worktree, in a session not started with `--tmux`.
const REMOVE: &str = "Remove worktree";

/// How the id of the dialog's question starts: its process and when it began waiting follow, so
/// an answer never goes to a dialog that came up after the one it was meant for.
pub const EXIT_ID: &str = "exit-worktree:";

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
            Choice { label: REMOVE.into(), description: Some("Its folder and branch are deleted, with what's only there.".into()) },
        ],
        on: 0,
    }
}

/// What it says of a worktree a `WorktreeCreate` hook made: it has no commit the worktree started
/// from to count what was added since (2.1.326's code; the words a terminal showed on 2026-11-18).
/// Its Remove runs the repo's `WorktreeRemove` hook, which Codealong's own check runs too.
const HOOKED: &str = "A WorktreeCreate hook made this worktree, so Claude Code can't tell whether it has unpushed commits.";

/// What follows `HOOKED` about a name you gave the session, around the name (2.1.328's code).
const NAMED: (&str, &str) = (" This session was named \"", "\". Keep the worktree to resume it later.");

/// What follows the branch its commits are on, where HEAD is on the branch the worktree was made
/// with, or it didn't look (2.1.328's code). Its Remove deletes that branch with `git branch -D`.
const BRANCH_GOES: &str = ". The branch will be deleted if you remove the worktree.";

/// Why the dialog asks, where that's only something Codealong's own check stands for (2.1.328's
/// code composes it): a count of commits on a branch, which it deletes, or a worktree a hook made,
/// which it can't count, a name you gave the session aside. Anything more it says (uncommitted
/// files, files saved from connectors, submodules it couldn't check, the branch HEAD is on now, a
/// detached HEAD, commits it couldn't count) and it's None.
#[derive(Debug, PartialEq)]
enum Asks {
    /// Commits counted on this branch, which Remove deletes.
    Commits(String),
    Hooked,
}

fn asks(says: &str) -> Option<Asks> {
    // Lines that wrapped were joined with a space wherever they broke.
    let says = says.split_whitespace().collect::<Vec<_>>().join(" ");
    if let Some(rest) = says.strip_prefix(HOOKED) {
        let named = || rest.strip_prefix(NAMED.0).and_then(|r| r.strip_suffix(NAMED.1)).is_some_and(|name| !name.contains('"'));
        return (rest.is_empty() || named()).then_some(Asks::Hooked);
    }
    let (count, rest) = says.strip_prefix("You have ")?.split_once(' ')?;
    count.parse::<u32>().ok().filter(|n| *n > 0)?;
    let rest = rest.strip_prefix("commits on ").or_else(|| rest.strip_prefix("commit on "))?;
    // A branch's name has no whitespace of its own: what's there is a line that broke mid-name.
    let branch = rest.strip_suffix(BRANCH_GOES)?.split_whitespace().collect::<String>();
    (!branch.is_empty()).then_some(Asks::Commits(branch))
}

/// Whether Codealong's own check answers `dialog`, read off its terminal: it asks whether to keep
/// the worktree at `own` only for what that check stands for (`asks`), and the branch it would
/// delete is the one HEAD is on there (`head`, read only then), which is the branch the check
/// looks at. Its Keep leaves that worktree where it is, not another the session went on to, and it
/// offers Remove worktree. Lines that wrapped were joined with a space wherever they broke,
/// mid-path too, so a path with no space of its own is matched without the spaces; one with
/// spaces only as it is. One started with `--tmux` never asks this way: its choices are about its
/// tmux session too.
fn answerable(dialog: &Dialog, own: &str, head: impl FnOnce() -> Option<String>) -> bool {
    let squeezed = |text: &str| text.split_whitespace().collect::<String>();
    let stays = format!("Stays at {own}");
    let names_own = |d: &str| d == stays || (!own.contains(char::is_whitespace) && squeezed(d) == squeezed(&stays));
    let keeps_own = |c: &Choice| c.label == "Keep worktree" && c.description.as_deref().is_some_and(names_own);
    if !dialog.choices.iter().any(keeps_own) || !dialog.choices.iter().any(|c| c.label == REMOVE) {
        return false;
    }
    match asks(&dialog.says) {
        Some(Asks::Commits(branch)) => head().is_some_and(|h| h == branch),
        Some(Asks::Hooked) => true,
        None => false,
    }
}

/// The dialog up in `s` and its registry entry, when Codealong's own check answers it
/// (`answerable`). Only one read off its terminal: one Codealong couldn't read (`assumed`) has
/// choices it only supposes.
fn up_for_check(home: &Path, s: &Session) -> Option<(Dialog, RegistryEntry)> {
    let own = s.own_worktree.as_deref()?;
    let (reg, screen) = super::dialog::open(home, s)?;
    let dialog = read(screen.as_deref()?)?;
    answerable(&dialog, own, || crate::git::branch(Path::new(own))).then_some((dialog, reg))
}

/// Whether the dialog up in `s` is one Codealong's own check answers (`answerable`).
pub fn check_answers(ctx: &Ctx, s: &Session) -> bool {
    up_for_check(&ctx.account.home_path(), s).is_some()
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

/// Answers the dialog up in `s` with Remove worktree, as Mark as done does where Codealong's own
/// check answers it (`answerable`, looked at again here as it's pressed) and found nothing in the
/// worktree would be lost.
pub fn remove(ctx: &Ctx, s: &Session) -> Result<()> {
    let (dialog, reg) = up_for_check(&ctx.account.home_path(), s).ok_or_else(|| anyhow!("It isn't asking only what Codealong's check of its worktree answers."))?;
    let i = dialog.choices.iter().position(|c| c.label == REMOVE).ok_or_else(|| anyhow!("None of its choices is {REMOVE}."))?;
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
    fn reads_only_the_reasons_codealongs_check_stands_for() {
        let commits = |b: &str| Some(Asks::Commits(b.into()));
        // As a terminal showed it on 2026-11-18, a branch whose HEAD had landed.
        assert_eq!(asks("You have 68 commits on worktree-kestrel. The branch will be deleted if you remove the worktree."), commits("worktree-kestrel"));
        assert_eq!(asks("You have 1 commit on worktree-kestrel. The branch will be deleted if you\nremove the worktree."), commits("worktree-kestrel"));
        // A terminal narrower than the branch's name breaks it.
        assert_eq!(asks("You have 1 commit on worktree-northwind-kes\ntrel. The branch will be deleted if you remove the worktree."), commits("worktree-northwind-kestrel"));
        assert_eq!(asks(HOOKED), Some(Asks::Hooked));
        assert_eq!(asks(&format!("{HOOKED} This session was named \"atlas\". Keep the worktree to resume it later.")), Some(Asks::Hooked));
        // What else 2.1.328 may say: each is about what the check doesn't look at, or deletes a
        // branch it didn't.
        for says in [
            "You have 2 commits on worktree-kestrel. The branch will be deleted if you remove the worktree. The worktree's connector-downloads folder holds 1 item saved from connectors. Git doesn't track that folder, so removing the worktree deletes it.",
            "You have 2 commits on worktree-kestrel. The branch will be deleted if you remove the worktree. Claude Code could not check the worktree's connector-downloads folder, which may hold files saved from connectors. Removing the worktree would delete them.",
            "You have 2 commits on worktree-kestrel. The branch will be deleted if you remove the worktree. The worktree is now on release, which is kept either way.",
            "You have 2 commits on worktree-kestrel. The branch will be deleted if you remove the worktree. The worktree's submodules could not be checked for uncommitted work (vendor/lib); anything uncommitted inside one is lost if you remove the worktree.",
            "You have 2 commits on worktree-kestrel or at the detached HEAD. They may be lost if you remove the worktree.",
            "You have 2 commits on worktree-kestrel that release doesn't have. Branch worktree-kestrel will be deleted if you remove the worktree.",
            "The branch worktree-kestrel may hold commits that could not be counted. The branch will be deleted if you remove.",
            "You have 1 uncommitted file and 2 commits on worktree-kestrel. All will be lost if you remove.",
            "You have 0 commits on worktree-kestrel. The branch will be deleted if you remove the worktree.",
            "This session was named \"atlas\". Keep the worktree to resume it later, or remove it to clean up.",
            "You are working in a worktree. Keep it to continue working there, or remove it to clean up.",
        ] {
            assert_eq!(asks(says), None, "{says}");
        }
        let connectors = format!("{HOOKED} The worktree's connector-downloads folder holds 2 items saved from connectors. Git doesn't track that folder, so removing the worktree deletes them.");
        assert_eq!(asks(&connectors), None);
    }

    #[test]
    fn answers_of_its_own_worktree_on_the_branch_it_deletes() {
        let own = "/Users/me/projects/northwind-app/.claude/worktrees/kestrel";
        let on = |b: &str| Some(b.to_string());
        let d = read(SCREEN).unwrap();
        assert!(answerable(&d, own, || on("worktree-kestrel")));
        // HEAD on another branch, or detached (a rebase under way): the branch it deletes isn't
        // the one the check looks at.
        assert!(!answerable(&d, own, || on("release")));
        assert!(!answerable(&d, own, || None));
        // As a terminal showed it on 2026-11-18, the worktree a hook made, whose branch is the
        // hook's business.
        let hooked = format!("   Exiting worktree session\n   {HOOKED}\n   ❯ 1. Keep worktree    Stays at {own}\n     2. Remove worktree  Any unpushed commits in the worktree may be lost.\n   Enter to confirm · Esc to cancel\n");
        let h = read(&hooked).unwrap();
        assert!(answerable(&h, own, || None));
        // Narrower, its sentence and its path wrapped.
        let narrow = "   Exiting worktree session\n   You have 68 commits on worktree-kestrel. The branch will be deleted if\n   you remove the worktree.\n   ❯ 1. Keep worktree    Stays at /Users/me/projects/northwind-app/.claude/\n                         worktrees/kestrel\n     2. Remove worktree  All changes and commits will be lost.\n   Enter to confirm · Esc to cancel\n";
        assert!(answerable(&read(narrow).unwrap(), own, || on("worktree-kestrel")));
        // Another worktree than the one Codealong checked: one the session went on to.
        assert!(!answerable(&h, "/Users/me/projects/northwind-app/.claude/worktrees/atlas", || None));
        // A path with spaces of its own is matched as it is, not squeezed into another's.
        let spaced = "/Users/me/projects/northwind-app/.claude/worktrees/release notes";
        assert!(answerable(&read(&hooked.replace(own, spaced)).unwrap(), spaced, || None));
        assert!(!answerable(&read(&hooked.replace(own, &spaced.replace(' ', ""))).unwrap(), spaced, || None));
        // Nor one in tmux, whose answer decides its tmux session too. (One Codealong couldn't
        // read never comes here: `up_for_check` reads it off the screen or not at all.)
        let tmux = format!("Exiting worktree session\n{HOOKED}\n❯ 1. Keep worktree and tmux session  Stays at {own}. Reattach with: tmux attach -t kestrel\n  2. Keep worktree, end tmux session  Keeps worktree at {own}, terminates tmux session.\n  3. Remove worktree and tmux session  Clean up the worktree directory.\nEnter to confirm · Esc to cancel");
        assert!(!answerable(&read(&tmux).unwrap(), own, || None));
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
