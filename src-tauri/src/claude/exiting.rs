//! Claude Code's "Exiting worktree session" dialog: what `/exit` asks before it exits a session
//! that holds a worktree it made for itself (`claude -w`), when removing the worktree would lose
//! something (commits on its branch, uncommitted files, a name you gave the session). Mark as
//! done types `/exit` into such a session so its worktree goes with it (idle.rs `exit`), so it's
//! Codealong that brings the dialog up more often than not. From 2.1.322's code, and in tmux
//! (2026-11-10):
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

use super::registry::{registry_of, RegistryEntry};

/// What a session's registry entry says it waits on while a command's dialog is up.
const DIALOG_OPEN: &str = "dialog open";

/// The dialog's title, which starts it on the screen.
const TITLE: &str = "Exiting worktree session";

/// What its last line ends with.
const LAST_LINE: &str = "Esc to cancel";

/// How the id of the dialog's question starts: its process and when it began waiting follow, so
/// an answer never goes to a dialog that came up after the one it was meant for.
pub const EXIT_ID: &str = "exit-worktree:";

/// The dialog as it shows: what removing the worktree would lose, and its choices.
#[derive(Debug, Clone, PartialEq)]
pub struct Dialog {
    pub says: String,
    pub choices: Vec<Choice>,
}

/// The dialog at the foot of a terminal's text (`launch::screen_of`), when that's what's up: its
/// title, what it says, its numbered choices and its last line, with nothing but blank lines
/// under that. A line that wraps goes on the next: what it says, or the choice before.
pub fn read(screen: &str) -> Option<Dialog> {
    let lines: Vec<&str> = screen.lines().map(|l| l.trim_matches(|c: char| c.is_whitespace() || c == '│')).collect();
    let title = lines.iter().rposition(|l| *l == TITLE)?;
    let rest = &lines[title + 1..];
    let last = rest.iter().position(|l| l.ends_with(LAST_LINE))?;
    if rest[last + 1..].iter().any(|l| !l.is_empty()) {
        return None;
    }
    let mut says: Vec<&str> = Vec::new();
    let mut choices: Vec<Choice> = Vec::new();
    for line in &rest[..last] {
        let line = line.trim_start_matches('❯').trim();
        if line.is_empty() {
            continue;
        }
        match numbered(line, choices.len() + 1) {
            Some(choice) => choices.push(choice),
            None => match choices.last_mut() {
                Some(choice) => choice.description = Some(choice.description.take().map_or_else(|| line.to_string(), |d| format!("{d} {line}"))),
                None => says.push(line),
            },
        }
    }
    (choices.len() >= 2).then(|| Dialog { says: says.join(" "), choices })
}

/// `n. Label  What it does`, as choice `n` shows: its label ends at the first run of two spaces.
fn numbered(line: &str, n: usize) -> Option<Choice> {
    let rest = line.strip_prefix(&format!("{n}."))?.trim_start();
    let (label, does) = rest.split_once("  ").map_or((rest, ""), |(l, d)| (l, d.trim()));
    let label = label.trim();
    (!label.is_empty()).then(|| Choice { label: label.to_string(), description: (!does.is_empty()).then(|| does.to_string()) })
}

/// The dialog as Codealong knows it without reading it, where its terminal can't be read and
/// Codealong typed the `/exit`: what one not started with `--tmux` offers. Its terminal says
/// what removing would lose. In one started with it, a 2 keeps the worktree.
fn assumed(worktree: &str) -> Dialog {
    let name = crate::util::file_name(Path::new(worktree));
    Dialog {
        says: format!("Claude Code asks whether to keep its worktree, {name}, before it exits: removing it would lose something, which its terminal says."),
        choices: vec![
            Choice { label: "Keep worktree".into(), description: Some(format!("Stays at {worktree}")) },
            Choice { label: "Remove worktree".into(), description: Some("Its folder and branch are deleted, with what's only there.".into()) },
        ],
    }
}

/// The dialog up in the live session `s`, with its registry entry: while the entry says a
/// dialog is open, what its terminal shows, or, where that can't be read, the one the `/exit`
/// Codealong typed brought up. A background job is stopped rather than exited, so it's never
/// one's.
fn up(home: &Path, s: &Session) -> Option<(Dialog, RegistryEntry)> {
    let reg = registry_of(home, s.pid?).filter(|r| r.session_id == s.id && r.waits_on(DIALOG_OPEN) && r.kind.as_deref() != Some("bg"))?;
    match crate::launch::screen_of(reg.pid, Path::new(&s.cwd)) {
        Ok(Some(text)) => read(&text).map(|d| (d, reg)),
        _ => {
            let since = reg.status_updated_at?;
            crate::idle::exit_asked(&s.key).filter(|a| a.pid == reg.pid && a.at <= since)?;
            Some((assumed(s.own_worktree.as_deref()?), reg))
        }
    }
}

/// The id an answer names the dialog up in its registry entry `reg` by.
fn id_of(reg: &RegistryEntry) -> String {
    format!("{EXIT_ID}{}:{}", reg.pid, reg.status_updated_at.unwrap_or(0))
}

fn question_of(dialog: &Dialog) -> Question {
    let says = if dialog.says.is_empty() { "Keep its worktree?".to_string() } else { dialog.says.clone() };
    Question { question: says, header: Some("Exiting".into()), options: dialog.choices.clone(), multi_select: false }
}

/// The dialog up in `s`, as a question to answer with one of its choices (`Pending::exiting`).
pub fn pending(ctx: &Ctx, s: &Session) -> Option<Pending> {
    let (dialog, reg) = up(&ctx.account.home_path(), s)?;
    Some(Pending {
        id: id_of(&reg),
        label: TITLE.into(),
        input: None,
        cannot: super::cannot_press(ctx, &reg),
        always: false,
        no_once: false,
        plan: false,
        message: false,
        always_rule: None,
        questions: vec![question_of(&dialog)],
        unwritten: false,
        terminal: None,
        exiting: true,
    })
}

/// The dialog `id` names, if it's still the one up in `s`.
fn still_up(ctx: &Ctx, s: &Session, id: &str) -> Result<(Dialog, RegistryEntry)> {
    let (dialog, reg) = up(&ctx.account.home_path(), s).ok_or_else(|| anyhow!("It isn't asking about its worktree any more."))?;
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
    let digit = u8::try_from(i + 1).ok().filter(|n| *n <= 9).ok_or_else(|| anyhow!("It offers more choices than Codealong picks among with a key. Answer it in its terminal."))?;
    super::press_in(&ctx.account.home_path(), s, &reg, Key::Digit(digit), "Open it to answer.")?;
    Ok(format!("Picked {picked}."))
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
    fn says_what_it_can_where_it_cant_read_the_terminal() {
        let d = assumed("/Users/me/projects/northwind-app/.claude/worktrees/kestrel");
        assert!(d.says.contains("kestrel"));
        assert_eq!(d.choices.iter().map(|c| c.label.as_str()).collect::<Vec<_>>(), ["Keep worktree", "Remove worktree"]);
        let q = question_of(&d);
        assert_eq!((q.header.as_deref(), q.multi_select), (Some("Exiting"), false));
    }
}
