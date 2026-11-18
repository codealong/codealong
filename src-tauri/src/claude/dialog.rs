//! Claude Code's own dialogs, read off the terminal it runs in: the ones it brings up by itself
//! (its offer to make auto mode the default, "Make auto mode your default permission mode?", at a
//! session's start and between turns on 2.1.327), and a command's panel that's only a list of
//! choices. They leave nothing in the transcript, and the registry says only `status:
//! "waiting"`, `waitingFor: "dialog open"`, so what's up is read off the screen
//! (`launch::screen_of`: Rex, tmux and Codealong's own terminals; Ghostty, iTerm2 and Terminal
//! can't be read, and a background job has no screen); before this a session waiting on one was
//! answered only there. The exit's question about a worktree is read the same way, by its title,
//! and answered by its digits (exiting.rs). A panel with more under its list than a line of keys
//! (`/model`'s effort control and its More models row) isn't one, and is answered where it runs.
//!
//! As Rex showed one (2.1.327, 2026-11-18): a panel at the foot of the screen under a rule drawn
//! across it (tmux showed a panel's top as `▔`), its title the first line under that, then what
//! it says, then its choices, the cursor `❯` on one and each label where the text after the
//! cursor starts, numbered (`❯ 1. Keep worktree`) or not (a `Select` with `hideIndexes`), what a
//! choice does after a run of spaces, and on some a last line of keys ("Enter to confirm · Esc to
//! cancel"):
//!
//! ```text
//! ──────────────────────────────────────────────────────────────────────────────
//!   Make auto mode your default permission mode?
//!
//!   Auto mode lets Claude handle permission prompts automatically. Claude checks each tool
//!   call for risky actions and prompt injection before executing, …
//!
//!   ❯ Yes, set auto mode as my default permission mode
//!     No, keep bypass permissions
//! ```
//!
//! Its keys: Claude Code's `Select` moves with ↑ and ↓ and picks with Return, and Escape is its
//! cancel (the auto mode offer's is "No, keep …"). A digit picks only where the choices are
//! numbered, and that offer, with `hideIndexes` and `disablePrintableKeybindings`, takes none
//! (2.1.327's code), so a choice is picked by moving to it and pressing Return, the screen read
//! again in between so that Return goes to the same dialog with the cursor on the choice you
//! picked, and after, so that the answer is said to be given only once the dialog has gone. The
//! offer turns down a key that comes as it opens (`refuseInput`: you typing on into the prompt it
//! covered), which a pick from Codealong never is, and takes no Return after letters were typed
//! into it until an arrow key is pressed, so the cursor is always moved, away and back where it's
//! on the pick already. A command's panels can follow one another while the registry says the
//! same, so an answer names the dialog by what it says and offers too (`id_of`).

use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Result};
use codealong_peer::api::Choice;
use serde_json::Value;

use crate::harness::{Ctx, Pending, Question};
use crate::launch::{Key, Press};
use crate::model::Session;

use super::registry::{entry_of, registry_of, Entry, RegistryEntry, DIALOG_OPEN};

/// How the id of a dialog's question starts: its process and when it began waiting follow, so
/// an answer never goes to a dialog that came up after the one it was meant for.
pub const DIALOG_ID: &str = "dialog:";

/// The cursor Claude Code's lists put on the choice the arrow keys are on.
const CURSOR: char = '❯';

/// How long the cursor gets to show on the choice picked once the arrow keys are pressed, before
/// Codealong gives up without pressing Return; and how often the screen is read meanwhile.
const MOVED_WAIT: Duration = Duration::from_secs(2);
const MOVED_EVERY: Duration = Duration::from_millis(100);

/// A dialog as its terminal shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct Dialog {
    pub title: String,
    /// What it says under its title, its lines joined.
    pub says: String,
    pub choices: Vec<Choice>,
    /// The choice the cursor is on.
    pub on: usize,
}

/// The dialog at the foot of a terminal's text, when one is up there: the lines under the last
/// rule drawn across the screen, the first of them its title.
pub fn read(screen: &str) -> Option<Dialog> {
    let lines = lines_of(screen);
    let rule = lines.iter().rposition(|l| is_rule(l))?;
    let rest = &lines[rule + 1..];
    let title = rest.iter().position(|l| !l.trim().is_empty())?;
    under(rest[title].trim(), &rest[title + 1..])
}

/// The dialog titled `title` at the foot of a terminal's text, where its title is known. A rule
/// drawn under the title is another panel's top: the title is an old dialog's, in the scrollback
/// above the one that's up.
pub fn titled(screen: &str, title: &str) -> Option<Dialog> {
    let lines = lines_of(screen);
    let at = lines.iter().rposition(|l| l.trim() == title)?;
    if lines[at + 1..].iter().any(|l| is_rule(l)) {
        return None;
    }
    under(title, &lines[at + 1..])
}

/// A screen's lines as a dialog is read from them: a box's left side (`│`) taken for a space, so
/// columns still count from the screen's edge, and its right side and the spaces at the end left
/// off.
fn lines_of(screen: &str) -> Vec<String> {
    screen
        .lines()
        .map(|l| {
            let l = l.trim_end_matches(|c: char| c.is_whitespace() || c == '│');
            if l.trim_start().starts_with('│') {
                l.replacen('│', " ", 1)
            } else {
                l.to_string()
            }
        })
        .collect()
}

/// A rule drawn across the screen above a panel: nothing but a line's characters.
fn is_rule(line: &str) -> bool {
    let line = line.trim();
    line.chars().count() >= 8 && line.chars().all(|c| matches!(c, '─' | '━' | '▔'))
}

/// A dialog's last line, which says its keys: "Enter to confirm · Esc to cancel".
fn says_keys(line: &str) -> bool {
    let line = line.to_lowercase();
    line.contains("esc to ") || line.contains("enter to ")
}

/// How many characters start a line before its text.
fn indent(line: &str) -> usize {
    line.chars().take_while(|c| c.is_whitespace()).count()
}

/// The dialog titled `title`, from the lines under its title to the screen's foot: what it says,
/// then its choices, which are the last lines with something on them (but for a line of keys),
/// one of them led by the cursor and the others' labels in line with its label. A line further
/// in carries on the choice above it. None for anything else: a list with no cursor or with
/// lines out of line, one that takes several picks, one whose labels repeat, or anything under
/// it.
pub fn under(title: &str, lines: &[String]) -> Option<Dialog> {
    let blank = |l: &str| l.trim().is_empty();
    let mut end = lines.len();
    while end > 0 && blank(&lines[end - 1]) {
        end -= 1;
    }
    if end > 0 && says_keys(&lines[end - 1]) {
        end -= 1;
        while end > 0 && blank(&lines[end - 1]) {
            end -= 1;
        }
    }
    // The cursor is in the run of lines at the foot, with no blank line between it and them.
    let mut block = end;
    while block > 0 && !blank(&lines[block - 1]) {
        block -= 1;
    }
    let cursor = (block..end).rev().find(|&i| lines[i].trim_start().starts_with(CURSOR))?;
    let after = lines[cursor].trim_start()[CURSOR.len_utf8()..].trim_start();
    let label_at = lines[cursor].chars().count() - after.chars().count();
    // The choices before the cursor's are the lines above it in line with its label or further in.
    let mut first = cursor;
    while first > block && indent(&lines[first - 1]) >= label_at {
        first -= 1;
    }
    let mut choices: Vec<Choice> = Vec::new();
    let mut numbers: Vec<Option<usize>> = Vec::new();
    let mut on = None;
    for (i, line) in lines.iter().enumerate().take(end).skip(first) {
        let (text, starts) = if i == cursor {
            on = Some(choices.len());
            (after, true)
        } else {
            let at = indent(line);
            if at < label_at {
                return None;
            }
            (line.trim(), at == label_at)
        };
        if starts {
            let (number, c) = choice(text)?;
            numbers.push(number);
            choices.push(c);
        } else {
            let last = choices.last_mut()?;
            let carried = match last.description.take() {
                Some(d) => Some(format!("{d} {text}")),
                None => {
                    last.label = format!("{} {text}", last.label);
                    None
                }
            };
            last.description = carried;
        }
    }
    // Numbered from 1 in order, as a `Select` that shows its indexes numbers them, or not at all.
    let numbered = numbers.iter().enumerate().all(|(i, n)| *n == Some(i + 1));
    let unique = choices.iter().enumerate().all(|(i, c)| !c.label.is_empty() && !choices[..i].iter().any(|o| o.label == c.label));
    if choices.len() < 2 || !unique || !(numbered || numbers.iter().all(Option::is_none)) {
        return None;
    }
    let says = lines[..first].iter().map(|l| l.trim()).filter(|l| !l.is_empty()).collect::<Vec<_>>().join(" ");
    Some(Dialog { title: title.to_string(), says, choices, on: on? })
}

/// A choice as its line shows it: its number where the list shows them (`1. `, or `1.  ` in
/// `/model`'s), its label, then what it does after a run of spaces. None for a box to tick, which
/// belongs to a list that takes several picks.
fn choice(text: &str) -> Option<(Option<usize>, Choice)> {
    const TICKS: [&str; 7] = ["[ ]", "[x]", "[✔]", "[✓]", "☐", "☑", "☒"];
    let (number, text) = match text.split_once('.') {
        Some((n, rest)) if rest.starts_with(' ') => match n.parse::<usize>() {
            Ok(n) => (Some(n), rest.trim_start()),
            Err(_) => (None, text),
        },
        _ => (None, text),
    };
    if text.is_empty() || TICKS.iter().any(|t| text.starts_with(t)) {
        return None;
    }
    let (label, does) = text.split_once("  ").map_or((text, ""), |(l, d)| (l, d.trim()));
    Some((number, Choice { label: label.trim().to_string(), description: (!does.is_empty()).then(|| does.to_string()) }))
}

/// The registry entry of a live session that waits on a dialog, with what its terminal shows when
/// that can be read. A background job's dialog has no screen to read, so it's never one's.
pub fn open(home: &Path, s: &Session) -> Option<(RegistryEntry, Option<String>)> {
    let reg = registry_of(home, s.pid?).filter(|r| waits(r, s))?;
    let screen = screen_of(&reg, s);
    Some((reg, screen))
}

/// The registry entry `reg` is the session `s` waiting on a dialog, outside a background job.
fn waits(reg: &RegistryEntry, s: &Session) -> bool {
    reg.session_id == s.id && reg.waits_on(DIALOG_OPEN) && reg.kind.as_deref() != Some("bg")
}

fn screen_of(reg: &RegistryEntry, s: &Session) -> Option<String> {
    crate::launch::screen_of(reg.pid, Path::new(&s.cwd)).ok().flatten()
}

/// The id an answer names the dialog `d` up in its registry entry `reg` by: its process, when it
/// began waiting (`entry_part`), and what the dialog says and offers, since a command's panels
/// follow one another with the registry saying the same all the while.
fn id_of(reg: &RegistryEntry, d: &Dialog) -> String {
    format!("{}{}", entry_part(reg), fingerprint(d))
}

fn entry_part(reg: &RegistryEntry) -> String {
    format!("{DIALOG_ID}{}:{}:", reg.pid, reg.status_updated_at.unwrap_or(0))
}

/// What a dialog says and offers, each choice with what it does, in a few characters: not where
/// its cursor is.
fn fingerprint(d: &Dialog) -> String {
    use sha2::{Digest, Sha256};
    let choices = d.choices.iter().map(|c| format!("{}\t{}", c.label, c.description.as_deref().unwrap_or_default())).collect::<Vec<_>>().join("\n");
    Sha256::digest(format!("{}\n{}\n{choices}", d.title, d.says).as_bytes()).iter().take(8).map(|b| format!("{b:02x}")).collect()
}

/// What its terminal shows as Codealong reads it again while answering.
#[derive(Debug, Clone, PartialEq)]
enum Seen {
    /// It waits on no dialog Codealong reads as one: none, or a panel that isn't a list.
    Gone,
    /// A dialog, with the id an answer names it by (`id_of`).
    Up(String, Dialog),
    /// Its registry entry or its screen didn't read, so the dialog may be up or gone.
    Unread,
}

/// What its terminal shows now, while Codealong answers the dialog `id` names.
fn seen(home: &Path, s: &Session, id: &str) -> Seen {
    let Some(pid) = s.pid else { return Seen::Gone };
    let entry = entry_of(home, pid);
    let screen = match &entry {
        Entry::Is(reg) if waits(reg, s) => screen_of(reg, s),
        _ => None,
    };
    seen_in(entry, s, screen.as_deref(), id)
}

/// `seen`, from the registry entry and the screen as they were read. A registry file caught
/// halfway through a write says nothing, nor does a screen with no dialog on it while the
/// registry entry is still the one the dialog came up under: a frame caught empty or partway
/// drawn can't be told from a panel that isn't a list, and a dialog that's answered moves the
/// registry on.
fn seen_in(entry: Entry, s: &Session, screen: Option<&str>, id: &str) -> Seen {
    let reg = match entry {
        Entry::Is(reg) => reg,
        Entry::Gone => return Seen::Gone,
        Entry::Unread => return Seen::Unread,
    };
    if !waits(&reg, s) {
        return Seen::Gone;
    }
    let Some(screen) = screen else { return Seen::Unread };
    match read(screen) {
        Some(d) => Seen::Up(id_of(&reg, &d), d),
        None if id.starts_with(&entry_part(&reg)) => Seen::Unread,
        None => Seen::Gone,
    }
}

/// The dialog `id` names is up, its cursor on `picked`.
fn on_pick(seen: &Seen, id: &str, picked: &str) -> bool {
    matches!(seen, Seen::Up(at, d) if at == id && d.choices.get(d.on).is_some_and(|c| c.label == picked))
}

/// The dialog `id` names has gone, as a key that answers it leaves it. A screen that couldn't be
/// read says neither.
fn gone(seen: &Seen, id: &str) -> bool {
    match seen {
        Seen::Gone => true,
        Seen::Up(at, _) => at != id,
        Seen::Unread => false,
    }
}

/// Reads its terminal again until `done` holds for what it shows, or until `MOVED_WAIT` runs out:
/// whether it came to that, and what it showed last.
fn wait_for(home: &Path, s: &Session, id: &str, done: impl Fn(&Seen) -> bool) -> (bool, Seen) {
    let deadline = Instant::now() + MOVED_WAIT;
    loop {
        let now = seen(home, s, id);
        if done(&now) || Instant::now() >= deadline {
            return (done(&now), now);
        }
        std::thread::sleep(MOVED_EVERY);
    }
}

/// After `key` (Return or Escape) was pressed at the dialog `id` names: an error unless its
/// terminal shows it gone.
fn took(home: &Path, s: &Session, id: &str, key: &str) -> Result<()> {
    match wait_for(home, s, id, |v| gone(v, id)) {
        (true, _) => Ok(()),
        (false, Seen::Unread) => bail!("Pressed {key}, but Codealong couldn't tell from its terminal that it took it. Look there."),
        (false, _) => bail!("It's still showing the dialog after {key}. Answer it in its terminal."),
    }
}

/// The dialog as a question, its title first: an answer is keyed by it, so one given to a dialog
/// that says something else now isn't taken.
fn question_of(d: &Dialog) -> Question {
    let question = if d.says.is_empty() { d.title.clone() } else { format!("{} {}", d.title, d.says) };
    Question { question, header: None, options: d.choices.clone(), multi_select: false }
}

/// The dialog up in `s`, as a question to answer with one of its choices (`Pending::dialog`): the
/// exit's question about its worktree first (exiting.rs), then any other its terminal shows.
pub fn pending(ctx: &Ctx, s: &Session) -> Option<Pending> {
    let (reg, screen) = open(&ctx.account.home_path(), s)?;
    if let Some(p) = super::exiting::pending(ctx, s, &reg, screen.as_deref()) {
        return Some(p);
    }
    let dialog = read(screen.as_deref()?)?;
    Some(Pending {
        id: id_of(&reg, &dialog),
        label: dialog.title.clone(),
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
        exiting: false,
        dialog: true,
    })
}

/// The dialog `id` names, if it's still the one up in `s`, as its terminal shows it now.
fn still_up(ctx: &Ctx, s: &Session, id: &str) -> Result<(Dialog, RegistryEntry)> {
    let gone = || anyhow!("It isn't showing that dialog any more.");
    let (reg, screen) = open(&ctx.account.home_path(), s).ok_or_else(gone)?;
    let screen = screen.ok_or_else(|| anyhow!("Codealong can't read its terminal now. Answer it there."))?;
    let dialog = read(&screen).ok_or_else(gone)?;
    if id_of(&reg, &dialog) != id {
        bail!("It's asking something else now. Look again before answering.");
    }
    Ok((dialog, reg))
}

/// Picks the choice `answers` names (its question's, keyed by the question as `pending` gave
/// it): the arrow keys from the choice the cursor is on to it, then, once its terminal shows the
/// dialog `id` names still up with the cursor there, Return, which has answered it once the
/// dialog is gone.
pub fn answer(ctx: &Ctx, s: &Session, id: &str, answers: &Value) -> Result<String> {
    let (dialog, reg) = still_up(ctx, s, id)?;
    let asked = question_of(&dialog).question;
    let labels = || dialog.choices.iter().map(|c| c.label.as_str()).collect::<Vec<_>>().join(" or ");
    let picked = match answers.get(&asked) {
        Some(v) => v.as_str().ok_or_else(|| anyhow!("Pick {} first.", labels()))?,
        None => bail!("It's asking something else now. Look again before answering."),
    };
    let Some(to) = dialog.choices.iter().position(|c| c.label == picked) else {
        bail!("It takes one of its choices, nothing else: {}.", labels());
    };
    let (home, cwd) = (ctx.account.home_path(), Path::new(&s.cwd));
    crate::launch::press_all(reg.pid, cwd, &moves(dialog.on, to))?;
    if !wait_for(&home, s, id, |v| on_pick(v, id, picked)).0 {
        bail!("Codealong didn't see its cursor come to {picked}, so it didn't press Return. Answer it in its terminal.");
    }
    crate::launch::press_all(reg.pid, cwd, &[Press::Key(Key::Return)])?;
    took(&home, s, id, "Return")?;
    Ok(format!("Picked {picked}."))
}

/// The arrow keys that take a list's cursor from choice `from` to choice `to`. Where it's there
/// already, one step away and back: the auto mode offer takes no Return after you typed into it,
/// until an arrow key is pressed (2.1.327's code).
fn moves(from: usize, to: usize) -> Vec<Press> {
    if from == to {
        let (away, back) = if from > 0 { (Key::Up, Key::Down) } else { (Key::Down, Key::Up) };
        return vec![Press::Key(away), Press::Key(back)];
    }
    let key = if to > from { Key::Down } else { Key::Up };
    std::iter::repeat_n(Press::Key(key), from.abs_diff(to)).collect()
}

/// Presses Escape, which closes it as its own cancel does.
pub fn decline(ctx: &Ctx, s: &Session, id: &str) -> Result<String> {
    let (dialog, reg) = still_up(ctx, s, id)?;
    let home = ctx.account.home_path();
    super::press_in(&home, s, &reg, Key::Escape, "Open it to answer.")?;
    took(&home, s, id, "Escape")?;
    Ok(format!("Closed {}, as Escape does there.", dialog.title))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Claude Code's offer to make auto mode the default, as Rex showed it (2.1.327,
    /// 2026-11-18), under the end of a reply.
    const AUTO: &str = "\
  If the northwind checks go green, I'll merge it.

✻ Brewed for 43s · done 8:33 AM

───────────────────────────────────────────────────────────────────────────────────────
  Make auto mode your default permission mode?

  Auto mode lets Claude handle permission prompts automatically. Claude checks each tool call for risky actions and prompt injection before
  executing, runs the ones it assesses as lower-risk, and blocks the rest.

  ❯ Yes, set auto mode as my default permission mode
    No, keep bypass permissions
";

    #[test]
    fn reads_the_auto_mode_offer() {
        let d = read(AUTO).unwrap();
        assert_eq!(d.title, "Make auto mode your default permission mode?");
        assert!(d.says.starts_with("Auto mode lets Claude handle permission prompts automatically. Claude checks"));
        assert!(d.says.ends_with("before executing, runs the ones it assesses as lower-risk, and blocks the rest."));
        assert_eq!(d.choices.iter().map(|c| c.label.as_str()).collect::<Vec<_>>(), ["Yes, set auto mode as my default permission mode", "No, keep bypass permissions"]);
        assert_eq!(d.on, 0);
        let q = question_of(&d);
        assert!(q.question.starts_with("Make auto mode your default permission mode? Auto mode lets"));
        assert!(!q.multi_select);
    }

    #[test]
    fn follows_the_cursor() {
        let moved = AUTO.replace("  ❯ Yes", "    Yes").replace("    No, keep", "  ❯ No, keep");
        assert_eq!(read(&moved).unwrap().on, 1);
    }

    #[test]
    fn reads_numbered_choices_and_what_they_do() {
        let model = "────────────\n Select model\n Switch between Claude models.\n\n ❯ 1. Default (recommended) ✔  Opus 5.5 · Most capable\n   2. Sonnet                    Sonnet 5.5 · Everyday\n   3. Haiku                     Haiku 5.5 · Fastest for\n                                quick answers\n\n Enter to confirm · Esc to exit\n";
        let d = read(model).unwrap();
        assert_eq!((d.title.as_str(), d.says.as_str(), d.on), ("Select model", "Switch between Claude models.", 0));
        assert_eq!(
            d.choices,
            [
                Choice { label: "Default (recommended) ✔".into(), description: Some("Opus 5.5 · Most capable".into()) },
                Choice { label: "Sonnet".into(), description: Some("Sonnet 5.5 · Everyday".into()) },
                Choice { label: "Haiku".into(), description: Some("Haiku 5.5 · Fastest for quick answers".into()) },
            ]
        );
    }

    #[test]
    fn isnt_a_panel_with_more_than_choices() {
        // `/model` as tmux showed it (2.1.327, 2026-11-18): a row that opens more models and an
        // effort control under the list, which its keys answer as well as the choices.
        let model = "────────────\n  Select model\n  Your pick is the default for new sessions. For other models, use --model.\n    1.  Default (recommended)  Opus 5.5 · Best for everyday, complex tasks\n    2.  Opus 5.5               For complex work and everyday tasks\n  ❯ 3.  Haiku 5.5 ✔            Fastest for quick answers\n    ▸   More models\n  ◐ Medium effort (default)\n  ←/→ to adjust · s to use this session only · Enter to save · Esc to cancel\n";
        assert_eq!(read(model), None);
        // Without them, its two spaces after a number are a numbered list's.
        let plain = model.replace("    ▸   More models\n  ◐ Medium effort (default)\n", "");
        let d = read(&plain).unwrap();
        assert_eq!(d.choices.iter().map(|c| c.label.as_str()).collect::<Vec<_>>(), ["Default (recommended)", "Opus 5.5", "Haiku 5.5 ✔"]);
        assert_eq!((d.on, d.choices[0].description.as_deref()), (2, Some("Opus 5.5 · Best for everyday, complex tasks")));
    }

    #[test]
    fn carries_a_label_that_wrapped_on() {
        let narrow = "──────────\nAllow the kestrel folder?\n❯ 1. Yes, and don't ask again for\n     this folder\n  2. No\n";
        let d = read(narrow).unwrap();
        assert_eq!(d.choices[0].label, "Yes, and don't ask again for this folder");
        assert_eq!(d.choices[1].label, "No");
    }

    #[test]
    fn reads_a_box_drawn_around_it() {
        let boxed = "─────────────\n│ Pick a theme              │\n│ ❯ Dark                    │\n│   Light                   │\n";
        let d = read(boxed).unwrap();
        assert_eq!((d.title.as_str(), d.choices.len(), d.on), ("Pick a theme", 2, 0));
    }

    #[test]
    fn isnt_a_prompt_or_anything_else() {
        // The input box between its rules, a status line under it.
        assert_eq!(read("⏺ Done.\n──────────────\n❯ \n──────────────\n  ⏵⏵ bypass permissions on (shift+tab to cycle)\n"), None);
        // Text under the list.
        assert_eq!(read(&format!("{AUTO}\n  Picked.\n")), None);
        // No cursor, one choice, or labels that repeat.
        assert_eq!(read("──────────\nTitle\n  Yes\n  No\n"), None);
        assert_eq!(read("──────────\nTitle\n❯ Yes\n"), None);
        assert_eq!(read("──────────\nTitle\n❯ Yes\n  Yes\n"), None);
        // A list that takes several picks.
        assert_eq!(read("──────────\nPick tools\n❯ [ ] Bash\n  [✔] Read\n"), None);
        // Numbers out of order, or a line out of line with the choices.
        assert_eq!(read("──────────\nTitle\n❯ 1. Yes\n  3. No\n"), None);
        assert_eq!(read("──────────\nTitle\n  ❯ Yes\n    No\n  more words\n"), None);
        // No rule above it.
        assert_eq!(read("Title\n❯ Yes\n  No\n"), None);
    }

    #[test]
    fn moves_with_the_arrows() {
        assert_eq!(moves(0, 2), [Press::Key(Key::Down), Press::Key(Key::Down)]);
        assert_eq!(moves(2, 1), [Press::Key(Key::Up)]);
        // Already there: away and back, which frees a Return held after typing.
        assert_eq!(moves(0, 0), [Press::Key(Key::Down), Press::Key(Key::Up)]);
        assert_eq!(moves(1, 1), [Press::Key(Key::Up), Press::Key(Key::Down)]);
    }

    #[test]
    fn knows_a_dialog_by_what_it_says_not_its_cursor() {
        let d = read(AUTO).unwrap();
        let moved = read(&AUTO.replace("  ❯ Yes", "    Yes").replace("    No, keep", "  ❯ No, keep")).unwrap();
        assert_eq!(fingerprint(&d), fingerprint(&moved));
        let other = read(&AUTO.replace("keep bypass permissions", "keep manual approvals")).unwrap();
        assert_ne!(fingerprint(&d), fingerprint(&other));
        // What a choice does is part of it too.
        let go = |to: &str| read(&format!("──────────\nSwitch folder?\n❯ Continue  Use {to}\n  Stay\n")).unwrap();
        assert_ne!(fingerprint(&go("atlas")), fingerprint(&go("kestrel")));
    }

    #[test]
    fn presses_return_only_at_the_dialog_it_was_asked_about() {
        let d = read(AUTO).unwrap();
        let id = "dialog:42:1000:abc";
        let up = |at: &str, on: usize| Seen::Up(at.into(), Dialog { on, ..d.clone() });
        assert!(on_pick(&up(id, 1), id, "No, keep bypass permissions"));
        assert!(!on_pick(&up(id, 0), id, "No, keep bypass permissions"));
        // The same words in a dialog that came up again since, or a screen that couldn't be read.
        assert!(!on_pick(&up("dialog:42:2000:abc", 1), id, "No, keep bypass permissions"));
        assert!(!on_pick(&Seen::Unread, id, "No, keep bypass permissions"));
    }

    #[test]
    fn reads_that_cant_be_told_from_a_dialog_still_up_say_nothing() {
        let s = Session { pid: Some(42), ..Session::new(crate::model::Harness::Claude, "claude", "abc") };
        let entry = |status: &str, at: i64| {
            Entry::Is(super::super::registry::parse_registry(&format!(r#"{{"pid":42,"sessionId":"abc","cwd":"/x","status":"{status}","waitingFor":"dialog open","statusUpdatedAt":{at},"kind":"interactive"}}"#)).unwrap())
        };
        let id = format!("{DIALOG_ID}42:1000:{}", fingerprint(&read(AUTO).unwrap()));
        assert!(matches!(seen_in(entry("waiting", 1000), &s, Some(AUTO), &id), Seen::Up(at, _) if at == id));
        // A registry file caught halfway, a screen that didn't read, and one with no dialog on it
        // under the same entry: a frame caught empty.
        assert_eq!(seen_in(Entry::Unread, &s, Some(AUTO), &id), Seen::Unread);
        assert_eq!(seen_in(entry("waiting", 1000), &s, None, &id), Seen::Unread);
        assert_eq!(seen_in(entry("waiting", 1000), &s, Some(""), &id), Seen::Unread);
        // The registry moved on: the session at its prompt, its process gone, or another dialog.
        assert_eq!(seen_in(entry("idle", 2000), &s, Some(""), &id), Seen::Gone);
        assert_eq!(seen_in(Entry::Gone, &s, None, &id), Seen::Gone);
        assert_eq!(seen_in(entry("waiting", 2000), &s, Some(""), &id), Seen::Gone);
    }

    #[test]
    fn is_answered_only_once_it_has_gone() {
        let d = read(AUTO).unwrap();
        let id = "dialog:42:1000:abc";
        assert!(gone(&Seen::Gone, id));
        assert!(gone(&Seen::Up("dialog:42:1000:def".into(), d.clone()), id), "another dialog came after it");
        assert!(!gone(&Seen::Up(id.into(), d), id), "still up");
        assert!(!gone(&Seen::Unread, id), "a screen that couldn't be read says nothing");
    }

    #[test]
    fn an_old_title_in_the_scrollback_isnt_the_dialog_up() {
        let exit = "   Exiting worktree session\n   You have 1 commit on worktree-kestrel.\n   ❯ 1. Keep worktree    Stays at /w\n     2. Remove worktree  All changes and commits will be lost.\n   Enter to confirm · Esc to cancel\n";
        assert!(titled(exit, "Exiting worktree session").is_some());
        // The exit was called off, and the auto mode offer came up under it.
        let both = format!("{exit}\n{AUTO}");
        assert_eq!(titled(&both, "Exiting worktree session"), None);
        assert_eq!(read(&both).unwrap().title, "Make auto mode your default permission mode?");
    }
}
