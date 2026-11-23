//! Codex's terminal UI approval prompts, read off the terminal screen (`launch::screen_of`:
//! tmux, Rex and Codealong's own terminals; Ghostty, iTerm2 and Terminal can't be read).
//!
//! Normally Codealong learns of a Codex CLI approval from its `PermissionRequest` hook
//! (`asks.rs`), and presses `y` or Escape in the terminal. That misses several cases where a
//! real prompt is nevertheless up on screen:
//! - `Approve for me` (`ModeKind::Auto`), where `asks::live` suppresses hook notes because
//!   Codex's guardian reviewer usually answers them, so a call the guardian escalates to a user
//!   prompt leaves `pending` empty;
//! - multiple approval requests in one turn (`!asks::answerable`), where the hook can't tell
//!   which prompt is currently shown;
//! - MCP / permissions prompts (`!note.keyed()`), custom `[tui.keymap]` bindings, or sessions
//!   started before the hook was installed.
//!
//! Codex's `ApprovalOverlay` (`codex-cli` 0.169.0) draws a heading ("Would you like to make the
//! following edits?", "Would you like to run the following command?", "Would you like to grant
//! these permissions?", "Do you want to approve network access to …?", "<server> needs your
//! approval."), optional details (command, diff, reason, destination), and a numbered list of
//! choices with `›` on the highlighted option:
//!
//! ```text
//! Would you like to make the following edits?
//!
//!   /private/tmp/codealong-codex-model-check.py (+5 -2)
//!
//! › 1. Yes, proceed
//!   2. Yes, and don't ask again for these files
//!   3. No, and tell Codex what to do differently
//!
//! Press enter to confirm or esc to cancel
//! ```
//!
//! Reading it off the screen lets Codealong show the live prompt with Allow, optional Always,
//! and Deny, and answer it with arrow keys and Return (verified by re-reading the screen before
//! and after Return, just like `claude/dialog.rs`), independent of `[tui.keymap]`.

use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Result};

use crate::harness::{Answer, Pending};
use crate::launch::{Key, Press};
use crate::model::{Session, Surface};
use crate::util::one_line;

/// Prefix for screen-read Codex approval ids so `Codex::answer` routes them here.
pub const SCREEN_ID: &str = "screen:";

/// How long we wait for the cursor to land on the chosen option or for the prompt to clear after
/// Return/Escape, and how often we re-read the terminal meanwhile.
const MOVED_WAIT: Duration = Duration::from_secs(2);
const MOVED_EVERY: Duration = Duration::from_millis(100);

/// Maximum lines above the choice list to scan for the approval heading.
const MAX_HEADING_SCAN: usize = 45;

/// A Codex approval overlay as read off the terminal screen.
#[derive(Debug, Clone, PartialEq)]
pub struct Prompt {
    pub title: String,
    pub body: Option<String>,
    pub choices: Vec<String>,
    /// 0-based index of the choice the cursor (`›` or `❯`) is currently on.
    pub on: usize,
}

impl Prompt {
    /// Index of the one-time "Yes, proceed / just this once / for this turn" choice.
    pub fn allow_index(&self) -> Option<usize> {
        self.choices.iter().position(|c| is_once_yes(c))
    }

    /// Index of the first "Yes, and don't ask again / for this session / conversation" choice.
    /// When Codex offers both conversation-scoped and permanent options (network access), the
    /// first one is the narrower conversation-scoped choice.
    pub fn always_index(&self) -> Option<usize> {
        self.choices.iter().position(|c| is_always_yes(c))
    }

    /// Index of the non-blocking decline choice: prefers "No, and tell Codex what to do
    /// differently", then any other "No, ..." that doesn't permanently block a host, then
    /// "Cancel ...".
    pub fn deny_index(&self) -> Option<usize> {
        if let Some(i) = self.choices.iter().position(|c| c.starts_with("No, and tell Codex")) {
            return Some(i);
        }
        if let Some(i) = self.choices.iter().rposition(|c| is_no(c) && !c.to_lowercase().contains("block")) {
            return Some(i);
        }
        self.choices.iter().rposition(|c| c.starts_with("Cancel"))
    }

    /// A concise one-line label for the approval card header.
    pub fn label(&self) -> String {
        if self.title == "Would you like to run the following command?" {
            if let Some(cmd) = self.command_in_body() {
                return format!("Run `{}`", one_line(cmd, 80));
            }
        }
        if self.title == "Would you like to make the following edits?" && self.body.is_some() {
            return "Edit files".to_string();
        }
        self.title.clone()
    }

    /// Extracts the command after `$ ` in the body of a command approval prompt, if present.
    fn command_in_body(&self) -> Option<&str> {
        self.body.as_deref()?.lines().map(str::trim).find_map(|l| l.strip_prefix("$ ").map(str::trim).filter(|c| !c.is_empty()))
    }

    /// Converts this prompt into a `Pending` if it offers at least one way to allow it.
    pub fn to_pending(&self, pid: u32) -> Option<Pending> {
        let allow = self.allow_index();
        let always = self.always_index();
        if allow.is_none() && always.is_none() {
            return None;
        }
        Some(Pending {
            id: id_of(pid, self),
            label: self.label(),
            input: self.body.clone(),
            cannot: None,
            always: always.is_some(),
            no_once: allow.is_none(),
            always_rule: always.map(|i| self.choices[i].clone()),
            ..Default::default()
        })
    }
}

fn is_yes(choice: &str) -> bool {
    choice == "Yes" || choice.starts_with("Yes,") || choice.starts_with("Yes ")
}

fn is_no(choice: &str) -> bool {
    choice == "No" || choice.starts_with("No,") || choice.starts_with("No ")
}

fn is_always_yes(choice: &str) -> bool {
    if !is_yes(choice) {
        return false;
    }
    let lower = choice.to_lowercase();
    lower.contains("don't ask again")
        || lower.contains("for this session")
        || lower.contains("for this conversation")
        || lower.contains("in the future")
        || lower.contains("always")
}

fn is_once_yes(choice: &str) -> bool {
    is_yes(choice) && !is_always_yes(choice)
}

/// Parses a Codex TUI approval overlay at the foot of `screen`, or `None` if no approval prompt
/// is active there.
pub fn read(screen: &str) -> Option<Prompt> {
    let lines = lines_of(screen);
    let mut end = lines.len();
    trim_blank(&lines, &mut end);

    // Codex may draw a status line ("% left" / "% context left") below the bottom pane, and a
    // key-hint line ("Press enter to confirm or esc to cancel") under the choices.
    let mut saw_footer = false;
    for _ in 0..3 {
        if end == 0 {
            return None;
        }
        let last = lines[end - 1].trim();
        if is_status_line(last) {
            end -= 1;
            trim_blank(&lines, &mut end);
        } else if is_key_hint(last) {
            saw_footer = true;
            end -= 1;
            trim_blank(&lines, &mut end);
        } else {
            break;
        }
    }
    if end == 0 {
        return None;
    }

    // Never match an idle composer prompt (`› ` or `❯ ` alone / with typed text, not a numbered
    // choice) at the foot of the screen.
    let (first_choice, choices, on) = parse_choices(&lines, end)?;

    // Scan upward from just above the choices for a Codex approval heading.
    let scan_start = first_choice.saturating_sub(MAX_HEADING_SCAN);
    let heading_idx = find_heading(&lines, scan_start, first_choice, &choices, saw_footer)?;
    let title = lines[heading_idx].trim().to_string();

    // Everything between the heading and the choices (trimmed of outer blank lines) is the body.
    let body_slice = &lines[heading_idx + 1..first_choice];
    let body_start = body_slice.iter().position(|l| !l.trim().is_empty());
    let body = body_start.and_then(|start| {
        let stop = body_slice.iter().rposition(|l| !l.trim().is_empty())? + 1;
        let kept: Vec<&str> = body_slice[start..stop].iter().map(String::as_str).collect();
        let min_indent = kept.iter().filter(|l| !l.trim().is_empty()).map(|l| l.chars().take_while(|c| c.is_whitespace()).count()).min().unwrap_or(0);
        let dedented = kept
            .into_iter()
            .map(|l| if l.trim().is_empty() { String::new() } else { l.chars().skip(min_indent).collect() })
            .collect::<Vec<_>>()
            .join("\n");
        (!dedented.trim().is_empty()).then_some(dedented)
    });

    Some(Prompt { title, body, choices, on })
}

fn trim_blank(lines: &[String], end: &mut usize) {
    while *end > 0 && lines[*end - 1].trim().is_empty() {
        *end -= 1;
    }
}

/// Strips outer box borders (`│`) and trailing whitespace, keeping leading column alignment so
/// wrapped choice labels can be recognized.
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

fn indent(line: &str) -> usize {
    line.chars().take_while(|c| c.is_whitespace()).count()
}

fn is_key_hint(line: &str) -> bool {
    let lower = line.to_lowercase();
    lower.contains("to confirm")
        || lower.contains("to cancel")
        || lower.contains("esc to ")
        || lower.contains("enter to ")
        || (lower.contains("confirm") && (lower.contains("cancel") || lower.contains("skip")))
}

fn is_status_line(line: &str) -> bool {
    let lower = line.to_lowercase();
    lower.contains("% context left") || lower.contains("% left")
}

fn is_turn_boundary(line: &str) -> bool {
    line.starts_with('•') || line.starts_with('›') || line.starts_with('❯')
}

/// Parses a contiguous numbered choice block ending at `end`, requiring at least 2 unique choices
/// numbered `1..=N` with exactly one marked by `›` or `❯`. Returns `(first_line_index, choices, on)`.
fn parse_choices(lines: &[String], end: usize) -> Option<(usize, Vec<String>, usize)> {
    let mut block = end;
    while block > 0 && !lines[block - 1].trim().is_empty() {
        block -= 1;
    }
    // Find the cursor line (`›` or `❯` followed by `<number>. <text>`).
    let cursor = (block..end).rev().find(|&i| cursor_choice(&lines[i]).is_some())?;
    let (_, cursor_text_col, _) = cursor_choice(&lines[cursor])?;

    // Walk backward from `cursor` to the first choice in this contiguous block.
    let mut first = cursor;
    while first > block {
        let prev = &lines[first - 1];
        if numbered_choice(prev).is_some() || indent(prev) >= cursor_text_col {
            first -= 1;
        } else {
            break;
        }
    }

    let mut choices: Vec<String> = Vec::new();
    let mut on: Option<usize> = None;
    for line in lines.iter().take(end).skip(first) {
        if let Some((num, _col, text)) = cursor_choice(line) {
            if num != choices.len() + 1 || on.is_some() {
                return None;
            }
            on = Some(choices.len());
            choices.push(text.to_string());
        } else if let Some((num, _col, text)) = numbered_choice(line) {
            if num != choices.len() + 1 {
                return None;
            }
            choices.push(text.to_string());
        } else if indent(line) >= cursor_text_col && !choices.is_empty() {
            // Wrapped continuation of the previous choice's label.
            let last = choices.last_mut()?;
            last.push(' ');
            last.push_str(line.trim());
        } else {
            return None;
        }
    }

    let on = on?;
    let unique = choices.iter().enumerate().all(|(i, c)| !c.is_empty() && !choices[..i].iter().any(|o| o == c));
    if choices.len() < 2 || !unique {
        return None;
    }
    Some((first, choices, on))
}

/// Parses a cursor-marked numbered choice: `  › 1. Yes, proceed` -> `(1, label_col, "Yes, proceed")`.
fn cursor_choice(line: &str) -> Option<(usize, usize, &str)> {
    let trimmed = line.trim_start();
    let after_cursor = trimmed.strip_prefix('›').or_else(|| trimmed.strip_prefix('❯'))?;
    let rest = after_cursor.trim_start();
    let (num, text) = split_number(rest)?;
    let col = line.chars().count() - text.chars().count();
    Some((num, col, text))
}

/// Parses an unmarked numbered choice: `    2. No, and tell Codex...` -> `(2, label_col, "No, ...")`.
fn numbered_choice(line: &str) -> Option<(usize, usize, &str)> {
    let trimmed = line.trim_start();
    if trimmed.starts_with('›') || trimmed.starts_with('❯') {
        return None;
    }
    let (num, text) = split_number(trimmed)?;
    let col = line.chars().count() - text.chars().count();
    Some((num, col, text))
}

fn split_number(s: &str) -> Option<(usize, &str)> {
    let (n, rest) = s.split_once(". ")?;
    let num = n.parse::<usize>().ok()?;
    let text = rest.trim();
    (!text.is_empty()).then_some((num, text))
}

/// Known headings drawn by Codex's `ApprovalOverlay` (0.169.0).
fn is_known_approval_heading(line: &str) -> bool {
    matches!(
        line,
        "Would you like to run the following command?"
            | "Would you like to make the following edits?"
            | "Would you like to grant these permissions?"
            | "Would you like to send input to the existing terminal?"
    ) || (line.starts_with("Would you like to send input to terminal ") && line.ends_with('?'))
        || (line.starts_with("Do you want to approve network access to ") && line.ends_with('?'))
        || line.ends_with(" needs your approval.")
}

/// Finds the approval heading above `first_choice`, stopping at a previous turn's prompt or
/// agent bullet so older scrollback above the overlay is never mistaken for the active prompt.
fn find_heading(lines: &[String], start: usize, first_choice: usize, choices: &[String], saw_footer: bool) -> Option<usize> {
    let mut fallback: Option<usize> = None;
    let looks_approval = choices.first().is_some_and(|c| is_yes(c))
        && choices.last().is_some_and(|c| is_no(c) || c.starts_with("Cancel"));

    for i in (start..first_choice).rev() {
        let t = lines[i].trim();
        if t.is_empty() {
            continue;
        }
        if is_turn_boundary(t) {
            break;
        }
        if is_known_approval_heading(t) {
            return Some(i);
        }
        // Fallback for future Codex approval wordings when a key-hint footer and Yes/No choices
        // confirm an approval overlay is up: the nearest question line above the body.
        if fallback.is_none() && saw_footer && looks_approval && first_choice - i <= 25 && t.ends_with('?') {
            fallback = Some(i);
        }
    }
    fallback
}

/// The id an answer names this prompt by: `screen:<pid>:<fingerprint>`.
pub fn id_of(pid: u32, prompt: &Prompt) -> String {
    format!("{SCREEN_ID}{pid}:{}", fingerprint(prompt))
}

/// Content fingerprint of the prompt (title, body, choices), excluding the cursor position `on`.
fn fingerprint(prompt: &Prompt) -> String {
    use sha2::{Digest, Sha256};
    let choices = prompt.choices.join("\n");
    let body = prompt.body.as_deref().unwrap_or_default();
    Sha256::digest(format!("{}\n{}\n{choices}", prompt.title, body).as_bytes())
        .iter()
        .take(8)
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Arrow presses needed to move the cursor from `from` to `to`.
fn moves(from: usize, to: usize) -> Vec<Press> {
    if from == to {
        return Vec::new();
    }
    let key = if to > from { Key::Down } else { Key::Up };
    std::iter::repeat_n(Press::Key(key), from.abs_diff(to)).collect()
}

#[derive(Debug, Clone, PartialEq)]
enum Seen {
    Gone,
    Up(String, Prompt),
    Unread,
}

fn seen_in(pid: u32, screen: Option<&str>) -> Seen {
    let Some(text) = screen else { return Seen::Unread };
    if text.trim().is_empty() {
        return Seen::Unread;
    }
    match read(text) {
        Some(p) => Seen::Up(id_of(pid, &p), p),
        None => Seen::Gone,
    }
}

fn on_pick(seen: &Seen, id: &str, target: usize) -> bool {
    matches!(seen, Seen::Up(at, p) if at == id && p.on == target)
}

fn gone(seen: &Seen, id: &str) -> bool {
    match seen {
        Seen::Gone => true,
        Seen::Up(at, _) => at != id,
        Seen::Unread => false,
    }
}

fn wait_for(pid: u32, cwd: &Path, done: impl Fn(&Seen) -> bool) -> (bool, Seen) {
    let deadline = Instant::now() + MOVED_WAIT;
    loop {
        let screen = crate::launch::screen_of(pid, cwd).ok().flatten();
        let now = seen_in(pid, screen.as_deref());
        if done(&now) || Instant::now() >= deadline {
            return (done(&now), now);
        }
        std::thread::sleep(MOVED_EVERY);
    }
}

/// Reads an active Codex approval overlay from the session's terminal, if it runs in a terminal
/// Codealong can read and press keys in.
pub fn pending(s: &Session) -> Option<Pending> {
    if s.surface != Surface::Cli {
        return None;
    }
    let pid = s.pid?;
    if !crate::launch::terminal_kind(pid).is_some_and(|t| t.presses_keys()) {
        return None;
    }
    let screen = crate::launch::screen_of(pid, Path::new(&s.cwd)).ok().flatten()?;
    read(&screen)?.to_pending(pid)
}

/// Answers a screen-read Codex approval prompt `id` by moving the cursor to the chosen option
/// with arrow keys (confirmed against the screen) and pressing Return (or Escape for Deny when
/// no decline choice is listed), confirming the prompt disappears before returning.
pub fn answer(s: &Session, id: &str, answer: Answer) -> Result<String> {
    if s.surface != Surface::Cli {
        bail!("Answer it where Codex asks it: Codealong can't from here.");
    }
    let pid = s.pid.ok_or_else(|| anyhow!("Codex isn't running in a terminal any more."))?;
    let cwd = Path::new(&s.cwd);
    let screen = crate::launch::screen_of(pid, cwd)?.ok_or_else(|| anyhow!("Codealong can't read its terminal now. Answer it there."))?;
    let prompt = read(&screen).ok_or_else(|| anyhow!("Codex isn't asking that any more."))?;
    if id_of(pid, &prompt) != id {
        bail!("Codex is asking something else now. Look again before answering.");
    }

    let target = match answer {
        Answer::Allow => Some(prompt.allow_index().ok_or_else(|| anyhow!("Codex offers no one-time Allow for this prompt."))?),
        Answer::Always => Some(prompt.always_index().ok_or_else(|| anyhow!("Codex offers no Always choice for this prompt."))?),
        Answer::Deny => prompt.deny_index(),
    };

    if let Some(to) = target {
        let steps = moves(prompt.on, to);
        if !steps.is_empty() {
            crate::launch::press_all(pid, cwd, &steps)?;
            if !wait_for(pid, cwd, |v| on_pick(v, id, to)).0 {
                let picked = &prompt.choices[to];
                bail!("Codealong didn't see Codex's cursor move to “{picked}”, so it didn't press Return. Answer it in its terminal.");
            }
        }
        crate::launch::press_all(pid, cwd, &[Press::Key(Key::Return)])?;
        let (cleared, last) = wait_for(pid, cwd, |v| gone(v, id));
        if !cleared {
            match last {
                Seen::Unread => bail!("Pressed Return, but Codealong couldn't tell from its terminal that Codex took it. Look there."),
                _ => bail!("Codex is still showing that prompt after Return. Answer it in its terminal."),
            }
        }
        Ok(match answer {
            Answer::Allow => format!("Allowed {}.", prompt.label()),
            Answer::Always => format!("Picked {}.", prompt.choices[to]),
            Answer::Deny => format!("Denied {}; it stops and waits for you.", prompt.label()),
        })
    } else {
        // Deny when no explicit "No, ..." choice was listed: press Escape.
        crate::launch::press_all(pid, cwd, &[Press::Key(Key::Escape)])?;
        let (cleared, last) = wait_for(pid, cwd, |v| gone(v, id));
        if !cleared {
            match last {
                Seen::Unread => bail!("Pressed Escape, but Codealong couldn't tell from its terminal that Codex took it. Look there."),
                _ => bail!("Codex is still showing that prompt after Escape. Answer it in its terminal."),
            }
        }
        Ok(format!("Denied {}; it stops and waits for you.", prompt.label()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EDIT_PROMPT: &str = "\
• The live Codex list confirms gpt-6.2-astra supports xhigh, so I'll save those defaults.

Would you like to make the following edits?

  /private/tmp/codealong-codex-model-check.py (+5 -2)
  - model = \"gpt-6-astra\"
  + model = \"gpt-6.2-astra\"

› 1. Yes, proceed
  2. Yes, and don't ask again for these files
  3. No, and tell Codex what to do differently

Press enter to confirm or esc to cancel
";

    const CMD_PROMPT: &str = "\
Would you like to run the following command?

  Reason: Install dependencies outside the sandbox
  $ npm install --registry https://registry.npmjs.org

› 1. Yes, proceed
  2. Yes, and don't ask again for commands that start with `npm install`
  3. No, and tell Codex what to do differently

Press enter to confirm or esc to cancel
  gpt-6.2-astra xhigh · 71% left · ~/projects/codealong
";

    const NETWORK_PROMPT: &str = "\
Do you want to approve network access to \"registry.npmjs.org\"?

  Destination: registry.npmjs.org

› 1. Yes, just this once
  2. Yes, and allow this host for this conversation
  3. Yes, and allow this host in the future
  4. No, and block this host in the future
  5. No, and tell Codex what to do differently

Press enter to confirm or esc to cancel
";

    const PERMS_PROMPT: &str = "\
Would you like to grant these permissions?

  read /private/tmp
  write /private/tmp

› 1. Yes, grant these permissions for this turn
  2. Yes, grant for this turn with strict auto review
  3. Yes, grant these permissions for this session
  4. No, continue without permissions

Press enter to confirm or esc to cancel
";

    const MCP_PROMPT: &str = "\
github needs your approval.

  Description: Create an issue on codealong/codealong
  Input: {\"title\": \"Fix screen approvals\"}

› 1. Yes, provide the requested info
  2. No, but continue without it
  3. Cancel this request

Press enter to confirm or esc to cancel
";

    #[test]
    fn reads_edit_approval_overlay_and_maps_to_pending() {
        let p = read(EDIT_PROMPT).unwrap();
        assert_eq!(p.title, "Would you like to make the following edits?");
        assert!(p.body.as_deref().unwrap().contains("/private/tmp/codealong-codex-model-check.py"));
        // A diff body containing a horizontal rule still finds the heading above it.
        let with_rule = EDIT_PROMPT.replace("  + model = \"gpt-6.2-astra\"", "  ────────────────\n  + model = \"gpt-6.2-astra\"");
        assert_eq!(read(&with_rule).unwrap().title, "Would you like to make the following edits?");
        assert_eq!(p.choices, ["Yes, proceed", "Yes, and don't ask again for these files", "No, and tell Codex what to do differently"]);
        assert_eq!(p.on, 0);
        assert_eq!((p.allow_index(), p.always_index(), p.deny_index()), (Some(0), Some(1), Some(2)));

        let pending = p.to_pending(42).unwrap();
        assert!(pending.id.starts_with("screen:42:"));
        assert_eq!(pending.label, "Edit files");
        assert!(pending.always);
        assert!(!pending.no_once);
        assert_eq!(pending.always_rule.as_deref(), Some("Yes, and don't ask again for these files"));
        assert_eq!(pending.cannot, None);
    }

    #[test]
    fn reads_command_approval_with_status_bar_below() {
        let p = read(CMD_PROMPT).unwrap();
        assert_eq!(p.label(), "Run `npm install --registry https://registry.npmjs.org`");
        assert!(p.body.as_deref().unwrap().contains("Reason: Install dependencies"));
        assert_eq!((p.allow_index(), p.always_index(), p.deny_index()), (Some(0), Some(1), Some(2)));
    }

    #[test]
    fn network_prompt_picks_conversation_always_and_non_blocking_deny() {
        let p = read(NETWORK_PROMPT).unwrap();
        assert_eq!(p.title, "Do you want to approve network access to \"registry.npmjs.org\"?");
        assert_eq!(p.allow_index(), Some(0));
        // Prefers the narrower conversation-scoped allow over permanent allow.
        assert_eq!(p.always_index(), Some(1));
        assert_eq!(p.choices[1], "Yes, and allow this host for this conversation");
        // Never picks "No, and block this host in the future" for a plain Deny.
        assert_eq!(p.deny_index(), Some(4));
        assert_eq!(p.choices[4], "No, and tell Codex what to do differently");
    }

    #[test]
    fn reads_permissions_and_mcp_prompts() {
        let perms = read(PERMS_PROMPT).unwrap();
        assert_eq!((perms.allow_index(), perms.always_index(), perms.deny_index()), (Some(0), Some(2), Some(3)));
        assert_eq!(perms.choices[2], "Yes, grant these permissions for this session");

        let mcp = read(MCP_PROMPT).unwrap();
        assert_eq!(mcp.title, "github needs your approval.");
        assert_eq!((mcp.allow_index(), mcp.always_index(), mcp.deny_index()), (Some(0), None, Some(1)));
        let pending = mcp.to_pending(7).unwrap();
        assert!(!pending.always);
        assert!(!pending.no_once);
    }

    #[test]
    fn handles_wrapped_choices_and_moved_cursor() {
        let narrow = "\
Would you like to make the following edits?

  /private/tmp/check.py

  1. Yes, proceed
› 2. Yes, and don't ask again for
     these files
  3. No, and tell Codex what to
     do differently

Press enter to confirm or esc to cancel
";
        let p = read(narrow).unwrap();
        assert_eq!(p.on, 1);
        assert_eq!(p.choices, ["Yes, proceed", "Yes, and don't ask again for these files", "No, and tell Codex what to do differently"]);
    }

    #[test]
    fn rejects_composer_scrollback_and_non_approval_menus() {
        // Idle composer at bottom.
        assert_eq!(read("Done.\n\n› \n  gpt-6.2-astra · 71% left\n"), None);
        // Old approval in scrollback above the idle composer.
        assert_eq!(read(&format!("{EDIT_PROMPT}\n› Fix the tests\n")), None);
        // Plain numbered markdown list in agent output.
        assert_eq!(read("Here are the steps:\n  1. Run cargo test\n  2. Push the branch\n"), None);
        // Non-approval `/model` or `/permissions` menu (not an approval heading, choices aren't Yes/No).
        assert_eq!(read("Select model\n› 1. gpt-6.2-astra\n  2. gpt-6-luna\nPress enter to confirm or esc to cancel\n"), None);
        // Only one choice.
        assert_eq!(read("Would you like to run the following command?\n› 1. Yes, proceed\n"), None);
    }

    #[test]
    fn fallback_matches_unknown_yes_no_approval_heading_with_footer() {
        let future = "\
Would you like to access the keychain?

  Service: com.example.token

› 1. Yes, allow once
  2. No, deny

Press enter to confirm or esc to cancel
";
        let p = read(future).unwrap();
        assert_eq!(p.title, "Would you like to access the keychain?");
        assert_eq!((p.allow_index(), p.deny_index()), (Some(0), Some(1)));
    }

    #[test]
    fn fingerprint_ignores_cursor_and_tracks_content() {
        let a = read(EDIT_PROMPT).unwrap();
        let moved = EDIT_PROMPT.replace("› 1. Yes, proceed", "  1. Yes, proceed").replace("  3. No,", "› 3. No,");
        let b = read(&moved).unwrap();
        assert_eq!(b.on, 2);
        assert_eq!(id_of(42, &a), id_of(42, &b));

        let other = read(&EDIT_PROMPT.replace("model-check.py", "other-file.py")).unwrap();
        assert_ne!(id_of(42, &a), id_of(42, &other));
    }

    #[test]
    fn moves_and_seen_state_machine() {
        assert_eq!(moves(0, 0), []);
        assert_eq!(moves(0, 2), [Press::Key(Key::Down), Press::Key(Key::Down)]);
        assert_eq!(moves(2, 1), [Press::Key(Key::Up)]);

        let p = read(EDIT_PROMPT).unwrap();
        let id = id_of(42, &p);
        let up0 = seen_in(42, Some(EDIT_PROMPT));
        assert!(on_pick(&up0, &id, 0));
        assert!(!on_pick(&up0, &id, 1));
        assert!(!gone(&up0, &id));

        assert_eq!(seen_in(42, None), Seen::Unread);
        assert_eq!(seen_in(42, Some("   \n")), Seen::Unread);
        assert!(!gone(&Seen::Unread, &id));

        let cleared = seen_in(42, Some("• Editing /private/tmp/codealong-codex-model-check.py…\n"));
        assert_eq!(cleared, Seen::Gone);
        assert!(gone(&cleared, &id));

        let next = seen_in(42, Some(CMD_PROMPT));
        assert!(gone(&next, &id));
    }
}
