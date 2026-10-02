//! Running a command from a session's conversation in a new terminal: a shell block in an agent's
//! reply, or a command the agent ran. It's typed into the new terminal's shell a line at a time,
//! as resuming and attaching are, so it shows there and goes into the history as if you had typed
//! it. What's typed is the block without its comments: zsh takes `#` as a comment only with
//! `interactive_comments` set, which it isn't by default, so a comment after a command would reach
//! the command as arguments. A line that starts a command loses its indentation too (a block in a
//! list comes indented with it), which would keep it out of a history that skips lines starting
//! with a space. Quoted text over several lines and here-documents go as they are.

use std::collections::VecDeque;
use std::path::Path;

use anyhow::{anyhow, bail, Result};

use crate::launch;
use crate::model::AppNames;
use crate::util::tilde;

/// Types a block's commands into a new terminal in `dir`, placed as `[apps] new_terminal` says,
/// beside the terminals of `near` (`board::terminal_neighbours`). A folder that's gone is an
/// error, not a reason to run it somewhere else.
pub fn run(apps: &AppNames, near: &[Vec<String>], dir: &Path, block: &str) -> Result<()> {
    if unseen(block) {
        bail!("It has characters that don't show here, so it isn't run: what ran wouldn't be what you see.");
    }
    let typed = input(block).ok_or_else(|| anyhow!("There's no command in it to run."))?;
    if !dir.is_absolute() || !dir.is_dir() {
        bail!("It would run in {}, which isn't there any more.", tilde(dir));
    }
    launch::terminal_open(&apps.terminal_id, launch::Placement { new_terminal: apps.new_terminal, near }, dir, Some(&typed))
}

/// What a line ends inside, for the lines after it.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Quote {
    No,
    Single,
    /// `$'…'`, where a backslash escapes a quote.
    Ansi,
    Double,
    Back,
}

/// A here-document still to come: the line that ends it, and whether that line's leading tabs
/// come off first (`<<-`).
struct Heredoc {
    delim: String,
    tabs: bool,
}

/// What to type for a block of shell commands: its lines, each ending in a newline, without
/// comments and the blank lines between commands. None when there's no command in it.
pub fn input(block: &str) -> Option<String> {
    let mut out: Vec<&str> = Vec::new();
    let mut quote = Quote::No;
    let mut pending: VecDeque<Heredoc> = VecDeque::new();
    let mut body: Option<Heredoc> = None;
    for line in block.lines() {
        if let Some(h) = &body {
            let end = if h.tabs { line.trim_start_matches('\t') } else { line };
            if end == h.delim {
                body = pending.pop_front();
            }
            out.push(line);
            continue;
        }
        // Inside quotes, or after a backslash that joins this line to the one before, the line is
        // part of that command, blank or not, and its leading blanks may matter.
        let continued = quote != Quote::No || out.last().is_some_and(|l| continues(l));
        let line = if continued { line } else { line.trim_start() };
        let end = code_end(line, &mut quote, &mut pending);
        let code = if end < line.len() { trim_blanks(&line[..end]) } else { line };
        if continued || !code.trim().is_empty() {
            out.push(code);
        }
        if body.is_none() && quote == Quote::No {
            body = pending.pop_front();
        }
    }
    if out.iter().all(|l| l.trim().is_empty()) {
        return None;
    }
    Some(out.iter().map(|l| format!("{l}\n")).collect())
}

/// Whether a block has characters that don't show where it does, or show out of order, and that a
/// terminal would take as keys: control characters other than tab and newline (a carriage return
/// shows as a space and types as Return), and the ones that reorder text. A block with any isn't
/// run, since what ran wouldn't be what you saw.
pub fn unseen(block: &str) -> bool {
    block.replace("\r\n", "\n").chars().any(|c| (c.is_control() && !matches!(c, '\n' | '\t')) || matches!(c, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}'))
}

/// Whether a typed line goes on to the next: it ends in a backslash that isn't itself escaped.
fn continues(line: &str) -> bool {
    line.bytes().rev().take_while(|&c| c == b'\\').count() % 2 == 1
}

/// `s` without the blanks it ends in, but for one that a backslash escapes.
fn trim_blanks(s: &str) -> &str {
    let t = s.trim_end_matches([' ', '\t']);
    if t.len() < s.len() && continues(t) {
        &s[..t.len() + 1]
    } else {
        t
    }
}

/// Where the code on a line ends: at a comment, or at the line's end. Carries the quote the line
/// ends inside to the next one, and notes the here-documents it starts. A `#` starts a comment at
/// the start of a word, outside quotes: `${#x}`, `a#b` and `"#"` aren't comments.
fn code_end(line: &str, quote: &mut Quote, heredocs: &mut VecDeque<Heredoc>) -> usize {
    let b = line.as_bytes();
    let mut word_start = *quote == Quote::No;
    // The character before was a `$` that isn't escaped: a quote after it is `$'…'`.
    let mut dollar = false;
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        match *quote {
            Quote::Single => {
                if c == b'\'' {
                    *quote = Quote::No;
                }
                word_start = false;
                dollar = false;
            }
            Quote::Ansi | Quote::Double | Quote::Back => {
                let close = match *quote {
                    Quote::Ansi => b'\'',
                    Quote::Double => b'"',
                    _ => b'`',
                };
                if c == b'\\' {
                    i += 1;
                } else if c == close {
                    *quote = Quote::No;
                }
                word_start = false;
                dollar = false;
            }
            Quote::No => {
                match c {
                    b'#' if word_start => return i,
                    b'\\' => i += 1,
                    b'\'' => *quote = if dollar { Quote::Ansi } else { Quote::Single },
                    b'"' => *quote = Quote::Double,
                    b'`' => *quote = Quote::Back,
                    // A here-string (`<<<`) is a word on this line, not a document on the next.
                    b'<' if b[i..].starts_with(b"<<<") => {
                        i += 3;
                        word_start = false;
                        dollar = false;
                        continue;
                    }
                    b'<' if b[i..].starts_with(b"<<") => {
                        i = heredoc(b, i, heredocs);
                        word_start = false;
                        dollar = false;
                        continue;
                    }
                    _ => {}
                }
                word_start = matches!(c, b' ' | b'\t' | b';' | b'&' | b'|' | b'(' | b')');
                dollar = c == b'$';
            }
        }
        i += 1;
    }
    b.len()
}

/// Notes the here-document whose `<<` is at `at`, and returns where its delimiter ends. The
/// delimiter is the word after the operator, less its quotes: `<<EOF`, `<<-'EOF'`, `<< "EOF"`.
fn heredoc(b: &[u8], at: usize, heredocs: &mut VecDeque<Heredoc>) -> usize {
    let mut i = at + 2;
    let tabs = b.get(i) == Some(&b'-');
    if tabs {
        i += 1;
    }
    while matches!(b.get(i), Some(b' ' | b'\t')) {
        i += 1;
    }
    let mut delim = Vec::new();
    while let Some(&c) = b.get(i) {
        match c {
            b'\'' | b'"' => {
                i += 1;
                while let Some(&d) = b.get(i) {
                    i += 1;
                    if d == c {
                        break;
                    }
                    delim.push(d);
                }
                continue;
            }
            b'\\' => {
                if let Some(&d) = b.get(i + 1) {
                    delim.push(d);
                }
                i += 2;
                continue;
            }
            b' ' | b'\t' | b';' | b'&' | b'|' | b'(' | b')' | b'<' | b'>' => break,
            _ => delim.push(c),
        }
        i += 1;
    }
    if !delim.is_empty() {
        heredocs.push_back(Heredoc { delim: String::from_utf8_lossy(&delim).into_owned(), tabs });
    }
    i.min(b.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(block: &str) -> String {
        input(block).unwrap_or_default()
    }

    #[test]
    fn a_command_is_typed_as_it_is() {
        assert_eq!(typed("npm run dev"), "npm run dev\n");
        assert_eq!(typed("npm run dev\n"), "npm run dev\n");
        assert_eq!(typed("git log --oneline -5 | grep '#12'"), "git log --oneline -5 | grep '#12'\n");
    }

    #[test]
    fn comments_and_blank_lines_are_left_out() {
        let block = "# Install first\nnpm ci\n\n# then build\nnpm run build   # takes a minute\n";
        assert_eq!(typed(block), "npm ci\nnpm run build\n");
        assert_eq!(typed("claude mcp list                          # expect: plugin:spruce ✔ Connected"), "claude mcp list\n");
        assert_eq!(typed("make;# done"), "make;\n");
        assert_eq!(input("# only a note\n\n   # and another"), None);
        assert_eq!(input(""), None);
    }

    #[test]
    fn a_hash_inside_a_word_or_quotes_stays() {
        assert_eq!(typed("echo ${#PATH} $# a#b"), "echo ${#PATH} $# a#b\n");
        assert_eq!(typed("git commit -m \"Fix #12\" # the issue"), "git commit -m \"Fix #12\"\n");
        assert_eq!(typed("echo 'it # stays' \"and # this\" `echo # too`"), "echo 'it # stays' \"and # this\" `echo # too`\n");
        assert_eq!(typed("echo \\# escaped"), "echo \\# escaped\n");
        assert_eq!(typed("echo $'it\\'s # here' # gone"), "echo $'it\\'s # here'\n");
        assert_eq!(typed("echo \"a\"#b"), "echo \"a\"#b\n");
        assert_eq!(typed("echo a\\ #b"), "echo a\\ #b\n");
    }

    #[test]
    fn a_line_that_starts_a_command_loses_its_indentation() {
        assert_eq!(typed("   cd app\n   npm test\n"), "cd app\nnpm test\n");
    }

    #[test]
    fn continued_lines_stay_as_they_are() {
        let block = "   docker run --rm \\\n     -v \"$PWD:/src\" \\\n     image   # the build\n";
        assert_eq!(typed(block), "docker run --rm \\\n     -v \"$PWD:/src\" \\\n     image\n");
        // A comment's line after a backslash still ends the command, as it would have.
        assert_eq!(typed("make \\\n  # a note\nls"), "make \\\n\nls\n");
        // A backslash before the blanks a comment leaves keeps the blank it escapes.
        assert_eq!(typed("echo a\\  # b"), "echo a\\ \n");
        // A comment's backslash doesn't continue the line.
        assert_eq!(typed("ls # see \\\npwd"), "ls\npwd\n");
    }

    #[test]
    fn quoted_text_over_several_lines_goes_whole() {
        let block = "git commit -m \"Title\n\n# not a comment\n  indented\" # a comment\ngit push";
        assert_eq!(typed(block), "git commit -m \"Title\n\n# not a comment\n  indented\"\ngit push\n");
    }

    #[test]
    fn here_documents_go_as_they_are() {
        let block = "cat > notes.md <<'EOF'\n# Notes\n\n  - one\nEOF\n  # done\nls";
        assert_eq!(typed(block), "cat > notes.md <<'EOF'\n# Notes\n\n  - one\nEOF\nls\n");
        let tabs = "cat <<-END | sort # sorted\n\tb\n\ta\n\tEND\necho ok";
        assert_eq!(typed(tabs), "cat <<-END | sort\n\tb\n\ta\n\tEND\necho ok\n");
        // Two on one line: the second's body follows the first's.
        assert_eq!(typed("cat <<A <<\"B\"\n# a\nA\n# b\nB\n"), "cat <<A <<\"B\"\n# a\nA\n# b\nB\n");
        // A here-string is no document.
        assert_eq!(typed("grep x <<< \"$text\" # find\n# next\nls"), "grep x <<< \"$text\"\nls\n");
    }

    /// Claude Code writes commit messages as a here-document inside a quoted `$(…)`.
    #[test]
    fn a_commit_message_from_a_here_document_goes_whole() {
        let block = "git commit -m \"$(cat <<'EOF'\nFix the build\n\n# Why: it broke\nEOF\n)\"";
        assert_eq!(typed(block), format!("{block}\n"));
    }

    #[test]
    fn hidden_characters_are_found() {
        assert!(!unseen("ls -la\n\techo 'x'\r\n"));
        assert!(unseen("echo safe\rrm -rf build"));
        assert!(unseen("ls\x1b[2K"));
        assert!(unseen("rm -rf ~\u{15}ls"));
        assert!(unseen("echo \u{202E}sl"));
        assert!(unseen("x\u{7f}"));
    }
}
