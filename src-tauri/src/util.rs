//! Small helpers shared by the adapters: time, paths, bounded file reads and text shaping.

use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const DAY_MS: i64 = 24 * 60 * 60 * 1000;

/// How an error from Go's networking ends when its connection was lost or couldn't be made, as
/// the Go programs Codealong reads say it: `agy` (antigravity.rs `CONNECTION_LOST`) and `gh`
/// (github.rs `unreachable`). A refused connection is left out: it doesn't come right by waiting.
pub const GO_CONNECTION_LOST: &[&str] = &[
    "broken pipe",
    "connection reset by peer",
    "no such host",
    "network is unreachable",
    "network is down",
    "no route to host",
    "i/o timeout",
    "TLS handshake timeout",
    "EOF",
    "http2: client connection lost",
];

/// An error as a window, the command or another device is told it: with what caused it, which
/// `to_string` leaves off an `anyhow` error ("Couldn't open the terminal: osascript timed out",
/// not "Couldn't open the terminal").
pub fn words(e: impl Into<anyhow::Error>) -> String {
    format!("{:#}", e.into())
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"))
}

/// Expands a leading `~` to the home directory.
pub fn expand(p: &str) -> PathBuf {
    expand_in(p, &home())
}

/// `expand`, with `home` as the home directory.
pub fn expand_in(p: &str, home: &Path) -> PathBuf {
    if p == "~" {
        return home.to_path_buf();
    }
    if let Some(rest) = p.strip_prefix("~/") {
        return home.join(rest);
    }
    PathBuf::from(p)
}

/// Shows a path with the home directory as `~`.
pub fn tilde(p: &Path) -> String {
    match p.strip_prefix(home()) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".to_string(),
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => p.display().to_string(),
    }
}

pub fn mtime_ms(p: &Path) -> Option<i64> {
    let modified = std::fs::metadata(p).ok()?.modified().ok()?;
    Some(modified.duration_since(UNIX_EPOCH).ok()?.as_millis() as i64)
}

/// Reads up to `max` bytes from the start of a file, dropping a trailing partial line
/// when the cap was hit.
pub fn read_head(p: &Path, max: usize) -> Option<String> {
    let mut f = File::open(p).ok()?;
    let mut buf = vec![0u8; max];
    let mut n = 0;
    while n < max {
        match f.read(&mut buf[n..]) {
            Ok(0) => break,
            Ok(k) => n += k,
            Err(_) => return None,
        }
    }
    buf.truncate(n);
    let mut s = String::from_utf8_lossy(&buf).into_owned();
    if n == max {
        match s.rfind('\n') {
            Some(i) => s.truncate(i + 1),
            None => s.clear(),
        }
    }
    Some(s)
}

/// Reads the first line of a file, however long, up to `max` bytes.
pub fn read_first_line(p: &Path, max: u64) -> Option<String> {
    read_line_at(p, 0, max)
}

/// Reads the line that starts at `at` in a file, however long, up to `max` bytes.
pub fn read_line_at(p: &Path, at: u64, max: u64) -> Option<String> {
    let mut f = File::open(p).ok()?;
    f.seek(SeekFrom::Start(at)).ok()?;
    let mut reader = BufReader::new(f.take(max));
    let mut line = String::new();
    reader.read_line(&mut line).ok()?;
    if line.is_empty() {
        None
    } else {
        Some(line)
    }
}

/// The bytes of a file from `start` up to `end`, as text. What isn't UTF-8 in them reads as
/// U+FFFD, so a range should start and end where lines do.
pub fn read_range(path: &Path, start: u64, end: u64) -> Option<String> {
    let mut f = File::open(path).ok()?;
    f.seek(SeekFrom::Start(start)).ok()?;
    let mut buf = Vec::with_capacity(end.saturating_sub(start) as usize);
    f.take(end.saturating_sub(start)).read_to_end(&mut buf).ok()?;
    Some(String::from_utf8_lossy(&buf).into_owned())
}

/// Where the last whole line of a file `len` long ends, looking back no further than `floor`:
/// a line still being written waits for the next read. `floor` when no line ends after it.
pub fn whole_end(f: &mut File, floor: u64, len: u64) -> std::io::Result<u64> {
    const CHUNK: u64 = 256 * 1024;
    let mut end = len;
    // Most files end where a line does.
    if end > floor {
        let mut last = [0u8; 1];
        f.seek(SeekFrom::Start(end - 1))?;
        f.read_exact(&mut last)?;
        if last[0] == b'\n' {
            return Ok(end);
        }
    }
    while end > floor {
        // Look back a chunk at a time for the last newline.
        let lo = end.saturating_sub(CHUNK).max(floor);
        let mut buf = vec![0; (end - lo) as usize];
        f.seek(SeekFrom::Start(lo))?;
        f.read_exact(&mut buf)?;
        if let Some(i) = buf.iter().rposition(|&b| b == b'\n') {
            return Ok(lo + i as u64 + 1);
        }
        end = lo;
    }
    Ok(floor)
}

/// The end of a file of lines, as `read_tail` read it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Tail {
    /// Where its first line starts in the file and where its last one ends: the next read goes
    /// on from `end`.
    pub start: u64,
    pub end: u64,
    /// The lines, each with its newline.
    pub bytes: Vec<u8>,
}

impl Tail {
    /// The lines as text. They were cut where lines end before being decoded, so a character
    /// the read started inside went with its line, and whatever else isn't UTF-8 reads as
    /// U+FFFD without moving `start` or `end`.
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.bytes).into_owned()
    }
}

/// Reads a file of lines from `from` to its end, whole lines only. A read that starts inside a
/// line drops that line, which belongs to whoever read up to there, and a last line with no
/// newline yet is left for the next read. Nothing before `from` is read but the one byte that
/// says whether a line starts there, however the rest decodes.
pub fn read_tail(path: &Path, from: u64) -> std::io::Result<Tail> {
    let mut f = File::open(path)?;
    let len = f.metadata()?.len();
    let from = from.min(len);
    let end = whole_end(&mut f, from, len)?;
    let mid_line = from > 0 && {
        let mut before = [0u8; 1];
        f.seek(SeekFrom::Start(from - 1))?;
        f.read_exact(&mut before)?;
        before[0] != b'\n'
    };
    let mut bytes = Vec::with_capacity((end - from) as usize);
    f.seek(SeekFrom::Start(from))?;
    (&mut f).take(end - from).read_to_end(&mut bytes)?;
    let mut start = from;
    if mid_line {
        match bytes.iter().position(|&b| b == b'\n') {
            Some(i) => {
                bytes.drain(..=i);
                start += i as u64 + 1;
            }
            None => bytes.clear(),
        }
    }
    Ok(Tail { start, end: start + bytes.len() as u64, bytes })
}

/// The last `max` bytes of a file of lines, as `read_tail` reads them.
pub fn read_last(path: &Path, max: u64) -> std::io::Result<Tail> {
    let len = std::fs::metadata(path)?.len();
    read_tail(path, len.saturating_sub(max))
}

/// A provider flag can precede the usual tail by many turns. On a fresh parse, look through
/// the earlier records once; subsequent incremental reads carry it forward. Most bytes are
/// tool output, so parse only candidate lines and let the harness check their structure. Read
/// forwards: unlike the bounded work lookback, this can cross very large tool-result lines.
pub fn flagged_before(path: &Path, floor: u64, end: u64, markers: &[&str], flagged: impl Fn(&serde_json::Value) -> bool) -> std::io::Result<bool> {
    let mut file = File::open(path)?;
    file.seek(SeekFrom::Start(floor))?;
    let mut reader = BufReader::new(file.take(end.saturating_sub(floor)));
    let mut buf = Vec::new();
    while reader.read_until(b'\n', &mut buf)? > 0 {
        if let Ok(line) = std::str::from_utf8(&buf) {
            if markers.iter().any(|m| line.contains(m))
                && serde_json::from_str(line).ok().is_some_and(|v| flagged(&v)) {
                return Ok(true);
            }
        }
        buf.clear();
    }
    Ok(false)
}

/// Hands the whole lines in `floor..end` (both at line starts) to `visit`, last first, reading
/// a megabyte at a time, until `visit` returns true.
pub fn scan_lines_back(path: &Path, floor: u64, end: u64, mut visit: impl FnMut(&str) -> bool) {
    use std::io::{Seek, SeekFrom};
    const CHUNK: u64 = 1024 * 1024;
    let Ok(mut f) = std::fs::File::open(path) else { return };
    let mut hi = end;
    // The start of a line the last read cut off, which the next (earlier) read finishes.
    let mut carry: Vec<u8> = Vec::new();
    while hi > floor {
        let lo = hi.saturating_sub(CHUNK).max(floor);
        let mut buf = Vec::with_capacity((hi - lo) as usize + carry.len());
        if f.seek(SeekFrom::Start(lo)).is_err() || (&mut f).take(hi - lo).read_to_end(&mut buf).is_err() {
            return;
        }
        buf.extend_from_slice(&carry);
        // What comes before the first newline belongs to a line that starts in an earlier chunk.
        let whole_from = if lo == floor {
            0
        } else {
            match buf.iter().position(|&b| b == b'\n') {
                Some(i) => i + 1,
                None => {
                    carry = buf;
                    hi = lo;
                    continue;
                }
            }
        };
        for line in buf[whole_from..].split(|&b| b == b'\n').rev() {
            if let Ok(line) = std::str::from_utf8(line) {
                if !line.is_empty() && visit(line) {
                    return;
                }
            }
        }
        carry = buf[..whole_from].to_vec();
        hi = lo;
    }
}

/// Parses RFC 3339 timestamps such as `2026-09-23T14:59:54.748Z` or
/// `2026-09-23T17:32:03.758417+00:00` into epoch milliseconds.
pub fn parse_rfc3339_ms(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() < 19 || b[4] != b'-' || b[7] != b'-' || (b[10] != b'T' && b[10] != b' ') {
        return None;
    }
    let num = |from: usize, to: usize| -> Option<i64> { s.get(from..to)?.parse::<i64>().ok() };
    let (year, month, day) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
    let (hour, minute, second) = (num(11, 13)?, num(14, 16)?, num(17, 19)?);
    let mut i = 19;
    let mut millis = 0i64;
    if b.get(i) == Some(&b'.') {
        i += 1;
        let start = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        let frac = &s[start..i];
        let padded = format!("{:0<3}", &frac[..frac.len().min(3)]);
        millis = padded.parse().ok()?;
    }
    let offset_min = match b.get(i) {
        None | Some(b'Z') | Some(b'z') => 0,
        Some(b'+') | Some(b'-') => {
            let sign = if b[i] == b'-' { -1 } else { 1 };
            let h = num(i + 1, i + 3)?;
            let m = if b.get(i + 3) == Some(&b':') { num(i + 4, i + 6)? } else { num(i + 3, i + 5).unwrap_or(0) };
            sign * (h * 60 + m)
        }
        _ => return None,
    };
    let days = days_from_civil(year, month, day);
    let secs = days * 86_400 + hour * 3600 + minute * 60 + second - offset_min * 60;
    Some(secs * 1000 + millis)
}

/// Days since 1970-01-01 for a proleptic Gregorian date (Howard Hinnant's algorithm).
pub(crate) fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The date `days` after 1970-01-01, as (year, month, day): `days_from_civil` undone.
pub(crate) fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { yoe + era * 400 + 1 } else { yoe + era * 400 }, m, d)
}

/// Epoch milliseconds as JavaScript's `toISOString` writes them: `2026-10-18T23:58:12.064Z`.
pub fn iso_utc(ms: i64) -> String {
    let (days, ms) = (ms.div_euclid(DAY_MS), ms.rem_euclid(DAY_MS));
    let (y, mo, d) = civil_from_days(days);
    format!("{y:04}-{mo:02}-{d:02}T{:02}:{:02}:{:02}.{:03}Z", ms / 3_600_000, ms / 60_000 % 60, ms / 1000 % 60, ms % 1000)
}

/// Today on this Mac's clock, as days since 1970-01-01.
pub(crate) fn today_local() -> i64 {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as libc::time_t;
    // SAFETY: localtime_r fills the struct it's given; a zeroed tm is a valid one.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&secs, &mut tm) };
    days_from_civil(tm.tm_year as i64 + 1900, tm.tm_mon as i64 + 1, tm.tm_mday as i64)
}

/// A time on this Mac's clock, in its time zone as it was on that day, as epoch milliseconds.
pub(crate) fn local_ms(year: i64, month: i64, day: i64, hour: i64, minute: i64, second: i64, millis: i64) -> Option<i64> {
    // SAFETY: a zeroed tm is a valid one, and mktime only reads and normalizes the one it's given.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    tm.tm_year = (year - 1900) as libc::c_int;
    tm.tm_mon = (month - 1) as libc::c_int;
    tm.tm_mday = day as libc::c_int;
    tm.tm_hour = hour as libc::c_int;
    tm.tm_min = minute as libc::c_int;
    tm.tm_sec = second as libc::c_int;
    // Whether summer time applies is the zone's to say.
    tm.tm_isdst = -1;
    let secs = unsafe { libc::mktime(&mut tm) };
    (secs != -1).then(|| secs as i64 * 1000 + millis)
}

/// Names as a sentence lists them, the last after `last` ("and", "or"): "a", "a or b", "a, b
/// or c".
pub fn listed(names: &[String], last: &str) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., end] => format!("{} {last} {end}", rest.join(", ")),
    }
}

/// First non-empty line, whitespace collapsed, markdown links reduced to their text, cut to
/// `max` characters with an ellipsis. A line that only opens or closes a block a directive wraps
/// (`:::writing{variant="email" id="83147"}`, `:::`, as Codex wraps a piece of writing) isn't
/// one: the window draws it as a card's edge, not as words.
pub fn one_line(s: &str, max: usize) -> String {
    let fence = |l: &str| l.strip_prefix(":::").is_some_and(|r| r.is_empty() || r.starts_with(|c: char| c.is_ascii_alphabetic()));
    let line = s.lines().map(str::trim).find(|l| !l.is_empty() && !fence(l)).unwrap_or("");
    let collapsed = line.split_whitespace().collect::<Vec<_>>().join(" ");
    truncate(&strip_links(&collapsed).replace("**", ""), max)
}

/// `[#1017](https://github.com/...)` → `#1017`.
fn strip_links(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(open) = rest.find('[') {
        let Some(close) = rest[open..].find("](").map(|i| open + i) else { break };
        let Some(end) = rest[close..].find(')').map(|i| close + i) else { break };
        out.push_str(&rest[..open]);
        out.push_str(&rest[open + 1..close]);
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out
}

pub fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let cut: String = s.chars().take(max.saturating_sub(1)).collect();
    format!("{}…", cut.trim_end())
}

/// `s` in single quotes for the shell, each quote in it closed, escaped and reopened.
pub fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Makes text safe to put inside a backtick code span.
pub fn code(s: &str) -> String {
    format!("`{}`", s.replace('`', "'"))
}

pub fn file_name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// `path` against `base`, with `~` expanded and `.` and `..` folded, without touching the disk:
/// the folder may be gone by the time a log is read.
pub fn resolve(base: &str, path: &str) -> String {
    use std::path::Component;
    let joined = Path::new(base).join(expand(path));
    let mut out = PathBuf::new();
    for c in joined.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other),
        }
    }
    out.to_string_lossy().into_owned()
}

/// A command's leading `cd` target as written, and what runs there after `&&`, `;` or a newline.
fn leading_cd(command: &str) -> Option<(String, Option<&str>)> {
    let rest = command.trim_start().strip_prefix("cd")?;
    if !rest.is_empty() && !rest.starts_with([' ', '\t']) {
        return None;
    }
    let rest = rest.trim_start_matches([' ', '\t']);
    let mut dir = String::new();
    let mut chars = rest.char_indices().peekable();
    let mut end = rest.len();
    while let Some((i, c)) = chars.next() {
        match c {
            '"' | '\'' => {
                for (_, q) in chars.by_ref() {
                    if q == c {
                        break;
                    }
                    dir.push(q);
                }
            }
            '\\' => {
                if let Some((_, next)) = chars.next() {
                    dir.push(next);
                }
            }
            c if c.is_whitespace() || matches!(c, ';' | '&' | '|' | ')') => {
                end = i;
                break;
            }
            c => dir.push(c),
        }
    }
    let after = rest[end..].trim_start_matches([' ', '\t']);
    let then = ["&&", ";", "\n"].iter().find_map(|sep| after.strip_prefix(sep)).map(str::trim_start);
    Some((dir, then))
}

/// The folder a shell command works in: where its leading `cd` goes, else `base`, the folder
/// it runs in. None when only the shell could say (`cd -`, `cd $DIR`), or when a command
/// without a `cd` names a path outside `base`: reading a temp file, looking into another
/// worktree or running a tool by its path says nothing about working in `base`, and agents
/// whose shell is reset to where they started run a lot of those between real work elsewhere.
/// None too when it goes to another folder only to look (`cd ../other && git show
/// origin/main:README.md`): an agent reads a repo it isn't working in that way, many commands
/// in a row, and reading with its file tools doesn't count as work there either.
pub fn command_dir(command: &str, base: &str) -> Option<String> {
    match leading_cd(command) {
        None if names_path_outside(command, base) => None,
        None => Some(base.to_string()),
        Some((dir, _)) if dir.is_empty() => Some(home().to_string_lossy().into_owned()),
        Some((dir, _)) if dir == "-" || dir.contains(['$', '`']) => None,
        Some((dir, then)) => Some(resolve(base, &dir)).filter(|dir| dir == base || !then.is_some_and(only_looks)),
    }
}

/// Whether a shell script only looks around: every command in it is one known to read (`ls`,
/// `grep`, `git show`…) and nothing is written to a file. A program it doesn't know may change
/// things, and so may a loop, a here-document or a command inside another, so a script with
/// any of those isn't only looking.
pub fn only_looks(script: &str) -> bool {
    simple_commands(script).is_some_and(|commands| commands.iter().all(|words| looks(words)))
}

/// A script's commands, each as its words, split at `;`, `&&`, `||`, `|`, newlines and
/// parentheses. None when it writes to a file (`> out.txt`; `/dev/null` and `2>&1` are fine)
/// or has something this doesn't read: a here-document, a command substitution, backticks.
fn simple_commands(script: &str) -> Option<Vec<Vec<String>>> {
    let mut commands: Vec<Vec<String>> = vec![Vec::new()];
    // The word being read; `Some("")` is an empty quoted one.
    let mut word: Option<String> = None;
    let mut chars = script.chars().peekable();
    fn end_word(commands: &mut Vec<Vec<String>>, word: &mut Option<String>) {
        if let Some(w) = word.take() {
            commands.last_mut().expect("never empty").push(w);
        }
    }
    while let Some(c) = chars.next() {
        match c {
            '\'' => word.get_or_insert_with(String::new).extend(chars.by_ref().take_while(|&q| q != '\'')),
            '"' => {
                let w = word.get_or_insert_with(String::new);
                loop {
                    match chars.next()? {
                        '"' => break,
                        '\\' => w.push(chars.next()?),
                        '`' => return None,
                        '$' if chars.peek() == Some(&'(') => return None,
                        q => w.push(q),
                    }
                }
            }
            '\\' => match chars.next() {
                // A line carried on.
                Some('\n') | None => {}
                Some(next) => word.get_or_insert_with(String::new).push(next),
            },
            '`' => return None,
            '$' if chars.peek() == Some(&'(') => return None,
            '#' if word.is_none() => {
                chars.by_ref().take_while(|&q| q != '\n').for_each(drop);
                commands.push(Vec::new());
            }
            ' ' | '\t' => end_word(&mut commands, &mut word),
            '<' if matches!(chars.peek(), Some('<' | '(')) => return None,
            // What a command reads from is one of its words, which is as good as an argument.
            '<' => end_word(&mut commands, &mut word),
            '>' | '&' if c == '>' || chars.peek() == Some(&'>') => {
                // The descriptor before it (`2>`) isn't a word.
                if !word.as_ref().is_some_and(|w| !w.is_empty() && w.chars().all(|d| d.is_ascii_digit())) {
                    end_word(&mut commands, &mut word);
                }
                word = None;
                while chars.next_if(|&q| q == '>').is_some() {}
                let copies = chars.next_if_eq(&'&').is_some();
                while chars.next_if(|&q| q == ' ' || q == '\t').is_some() {}
                let mut to = String::new();
                while let Some(q) = chars.next_if(|&q| !q.is_whitespace() && !matches!(q, ';' | '&' | '|' | '(' | ')' | '<' | '>')) {
                    to.push(q);
                }
                let descriptor = copies && (to == "-" || (!to.is_empty() && to.chars().all(|d| d.is_ascii_digit())));
                if !descriptor && to != "/dev/null" {
                    return None;
                }
            }
            '\n' | ';' | '&' | '|' | '(' | ')' => {
                end_word(&mut commands, &mut word);
                commands.push(Vec::new());
            }
            c => word.get_or_insert_with(String::new).push(c),
        }
    }
    end_word(&mut commands, &mut word);
    commands.retain(|words| !words.is_empty());
    Some(commands)
}

/// Whether a command, as its words, only reads.
fn looks(words: &[String]) -> bool {
    let assigns = |w: &String| w.split_once('=').is_some_and(|(name, _)| !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
    let words: Vec<&str> = words.iter().skip_while(|w| assigns(w)).map(String::as_str).collect();
    // Only variables set.
    let Some((&program, args)) = words.split_first() else { return true };
    let has = |flags: &[&str]| args.iter().any(|a| flags.contains(a));
    match program.rsplit('/').next().unwrap_or(program) {
        "cd" | "ls" | "cat" | "head" | "tail" | "grep" | "egrep" | "fgrep" | "rg" | "wc" | "cut" | "sort" | "uniq" | "tr" | "jq" | "awk" | "echo" | "printf" | "pwd" | "date" | "tree" | "stat"
        | "file" | "diff" | "du" | "which" | "test" | "[" | "true" | "false" | "basename" | "dirname" | "realpath" | "readlink" | "nl" | "column" | "sleep" => true,
        "sed" => !args.iter().any(|a| a.starts_with("-i") || *a == "--in-place"),
        "find" => !has(&["-delete", "-exec", "-execdir", "-ok", "-okdir"]) && !args.iter().any(|a| a.starts_with("-fprint") || *a == "-fls"),
        "git" => git_looks(args),
        // `gh pr view`, `gh run list`, `gh pr checks`…: what an action is called says whether it reads.
        "gh" => args.first() == Some(&"search") || matches!(args.get(1), Some(&("view" | "list" | "diff" | "checks" | "status" | "watch"))),
        _ => false,
    }
}

/// Whether a git command only reads. Fetching counts: it changes none of the folder's files.
fn git_looks(args: &[&str]) -> bool {
    // Its own options come before the subcommand; these take a value.
    let mut args = args.iter().copied();
    let mut sub = None;
    while let Some(a) = args.next() {
        match a {
            "-C" | "-c" | "--git-dir" | "--work-tree" | "--namespace" => {
                args.next();
            }
            a if a.starts_with('-') => {}
            a => {
                sub = Some(a);
                break;
            }
        }
    }
    let rest: Vec<&str> = args.collect();
    let Some(sub) = sub else { return true };
    match sub {
        "show" | "log" | "status" | "diff" | "grep" | "ls-tree" | "ls-files" | "ls-remote" | "blame" | "rev-parse" | "rev-list" | "cat-file" | "describe" | "shortlog" | "merge-base" | "for-each-ref"
        | "show-ref" | "name-rev" | "whatchanged" | "show-branch" | "check-ignore" | "fetch" => true,
        "worktree" => rest.first() == Some(&"list"),
        "stash" => matches!(rest.first(), Some(&("list" | "show"))),
        "remote" => matches!(rest.first(), None | Some(&("-v" | "show" | "get-url"))),
        "reflog" => matches!(rest.first(), None | Some(&"show")),
        "tag" => matches!(rest.first(), None | Some(&("-l" | "--list"))),
        "config" => rest.iter().any(|a| matches!(*a, "--get" | "--get-all" | "--get-regexp" | "--list" | "-l")),
        // Listing takes only options; a name makes a branch, and these change one.
        "branch" => rest.iter().all(|a| a.starts_with('-') && !matches!(*a, "-d" | "-D" | "-m" | "-M" | "-c" | "-C" | "-f" | "-u" | "--delete" | "--move" | "--copy" | "--force" | "--unset-upstream" | "--edit-description") && !a.starts_with("--set-upstream")),
        _ => false,
    }
}

/// Whether a command names a path outside `base`: one that starts a word and is absolute
/// (`/…`), in the home folder (`~/…`) or above `base` (`../…`). `/dev/null` and the like
/// don't count, and neither do URLs, whose slashes don't start a word.
fn names_path_outside(command: &str, base: &str) -> bool {
    let starts_word = |prev: Option<char>| prev.map_or(true, |c| c.is_whitespace() || matches!(c, '\'' | '"' | '=' | '('));
    let ends_word = |c: char| c.is_whitespace() || matches!(c, '\'' | '"' | '`' | ';' | '|' | '&' | ')' | '<' | '>');
    let mut prev = None;
    for (i, c) in command.char_indices() {
        let rest = &command[i..];
        if starts_word(prev) && (rest.starts_with('/') || rest.starts_with("~/") || rest.starts_with("../")) {
            let word = &rest[..rest.find(ends_word).unwrap_or(rest.len())];
            let path = resolve(base, word);
            if !path.starts_with("/dev/") && !Path::new(&path).starts_with(base) {
                return true;
            }
        }
        prev = Some(c);
    }
    false
}

/// A command without its leading `cd <folder> &&`, which the row's worktree already says.
pub fn without_cd(command: &str) -> &str {
    match leading_cd(command) {
        Some((_, Some(rest))) if !rest.is_empty() => rest,
        _ => command,
    }
}

/// How many recent places of work a parsed log keeps: enough for `board::session_seed` to
/// skip the ones outside every worktree.
const WORK_KEPT: usize = 12;

/// How far before its tail a first read of a log looks for where the session worked.
const WORK_LOOKBACK: u64 = 4 * 1024 * 1024;

/// Records where a tool call worked, oldest first.
pub fn note_work(work: &mut Vec<String>, path: String) {
    work.push(path);
    if work.len() > WORK_KEPT {
        work.remove(0);
    }
}

/// Puts ahead of a first read's places of work, `work`, the ones before its tail that reading the
/// log as it grew would have kept, `line_work` saying where each line worked, in order. A tail of
/// long results can hold too few commands and edits to say which worktree a session is in, and a
/// first read (after a relaunch) has nothing from before it. Read backwards from `end` (a line
/// start), no further than `floor` nor `WORK_LOOKBACK`, so a long log with little work in it
/// isn't read whole.
pub fn work_before(path: &Path, floor: u64, end: u64, work: &mut Vec<String>, mut line_work: impl FnMut(&str) -> Vec<String>) {
    let want = WORK_KEPT.saturating_sub(work.len());
    if want == 0 {
        return;
    }
    let mut found = Vec::new();
    scan_lines_back(path, floor.max(end.saturating_sub(WORK_LOOKBACK)), end, |line| {
        found.extend(line_work(line).into_iter().rev());
        found.len() >= want
    });
    found.truncate(want);
    found.reverse();
    work.splice(0..0, found);
}


/// Maps `f` over `items` on a few threads.
pub fn parallel_map<T: Send, R: Send>(items: Vec<T>, f: impl Fn(T) -> Option<R> + Sync) -> Vec<R> {
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(8);
    let chunk = items.len().div_ceil(threads).max(1);
    let mut buckets: Vec<Vec<T>> = Vec::new();
    let mut items = items.into_iter().peekable();
    while items.peek().is_some() {
        buckets.push(items.by_ref().take(chunk).collect());
    }
    let f = &f;
    std::thread::scope(|scope| {
        let handles: Vec<_> = buckets.into_iter().map(|b| scope.spawn(move || b.into_iter().filter_map(f).collect::<Vec<R>>())).collect();
        handles.into_iter().flat_map(|h| h.join().unwrap_or_default()).collect()
    })
}

/// Replaces `file` with `text` in one step, so nobody reads it half written: through a link to
/// it, as the harnesses write theirs, and readable only by you, as they keep them.
pub fn write_whole(file: &Path, text: &str) -> anyhow::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let target = std::fs::canonicalize(file).unwrap_or_else(|_| file.to_path_buf());
    let name = target.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    let temp = target.with_file_name(format!("{name}.codealong-{}", std::process::id()));
    let written = (|| -> std::io::Result<()> {
        let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&temp)?;
        f.write_all(text.as_bytes())?;
        f.sync_all()?;
        std::fs::rename(&temp, &target)
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    Ok(written?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_flags_scan_only_the_requested_history_past_large_or_invalid_lines() {
        let dir = crate::fixtures::scratch("flagged-history");
        let path = dir.join("history.jsonl");
        let flag = b"{\"flagged\":true}\n";
        let mut bytes = flag.to_vec();
        let floor = bytes.len() as u64;
        bytes.extend_from_slice(&vec![b'x'; 3 * 1024 * 1024]);
        bytes.extend_from_slice(b"\n\xff\n{\"flagged\":false}\n");
        let before = bytes.len() as u64;
        bytes.extend_from_slice(flag);
        let end = bytes.len() as u64;
        std::fs::write(&path, bytes).unwrap();
        let read = |floor, end| flagged_before(&path, floor, end, &["\"flagged\""], |v| v["flagged"] == true).unwrap();
        assert!(!read(floor, before), "neither the prefix nor the unread tail counts");
        assert!(read(floor, end), "an old flag after large and malformed lines is found");
        assert!(!read(end, end));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn an_error_says_what_caused_it() {
        let e = anyhow::anyhow!("osascript timed out").context("Couldn't open the terminal");
        assert_eq!(words(e), "Couldn't open the terminal: osascript timed out");
        assert_eq!(words(std::io::Error::other("disk full")), "disk full");
    }

    #[test]
    fn reads_the_end_of_a_file_as_whole_lines() {
        let dir = crate::fixtures::scratch("read-tail");
        let path = dir.join("lines.jsonl");
        // "é" is two bytes, at 8 and 9: a read can start between them.
        std::fs::write(&path, "one\ntwo é\nthree\nfou").unwrap();
        let read = |from: u64| {
            let t = read_tail(&path, from).unwrap();
            (t.start, t.end, t.text())
        };
        assert_eq!(read(0), (0, 17, "one\ntwo é\nthree\n".to_string()));
        // From where a line starts, the line is whole.
        assert_eq!(read(4), (4, 17, "two é\nthree\n".to_string()));
        // From inside a line, or inside a character, the line it's in is dropped; the text
        // starts where the next one does, and nothing before the read is read for it.
        assert_eq!(read(5), (11, 17, "three\n".to_string()));
        assert_eq!(read(9), (11, 17, "three\n".to_string()));
        assert_eq!(read_last(&path, 11).unwrap(), read_tail(&path, 9).unwrap());
        // The line still being written waits: the next read starts where this one ended.
        assert_eq!(read(17), (17, 17, String::new()));
        std::fs::write(&path, "one\ntwo é\nthree\nfour\n").unwrap();
        assert_eq!(read(17), (17, 22, "four\n".to_string()));
        // Past the end there's nothing, and a file that isn't there is an error.
        assert_eq!(read(99), (22, 22, String::new()));
        assert!(read_tail(&dir.join("missing"), 0).is_err());
        // Bytes that aren't UTF-8 inside a line don't move where the lines are.
        std::fs::write(&path, b"a\xff\nb\n").unwrap();
        assert_eq!(read(0), (0, 5, "a\u{fffd}\nb\n".to_string()));
        assert_eq!(read_range(&path, 3, 5).as_deref(), Some("b\n"));
        let mut f = File::open(&path).unwrap();
        assert_eq!((whole_end(&mut f, 0, 5).unwrap(), whole_end(&mut f, 0, 2).unwrap(), whole_end(&mut f, 1, 2).unwrap()), (5, 0, 1));
    }

    #[test]
    fn parses_rfc3339() {
        assert_eq!(parse_rfc3339_ms("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_rfc3339_ms("2026-09-23T14:59:54.748Z"), Some(1_790_175_594_748));
        assert_eq!(parse_rfc3339_ms("2026-09-23T17:32:03.758417Z"), Some(1_790_184_723_758));
        assert_eq!(parse_rfc3339_ms("2026-09-23T10:32:03-07:00"), Some(1_790_184_723_000));
        assert_eq!(parse_rfc3339_ms("nonsense"), None);
        // And written back as JavaScript writes them.
        assert_eq!(iso_utc(1_790_175_594_748), "2026-09-23T14:59:54.748Z");
        assert_eq!(iso_utc(0), "1970-01-01T00:00:00.000Z");
    }

    /// Whatever the zone the tests run in, the time read back on the clock is the one given.
    #[test]
    fn reads_a_time_on_this_macs_clock() {
        let ms = local_ms(2026, 10, 14, 17, 45, 37, 928).unwrap();
        assert_eq!(ms % 1000, 928);
        let secs = (ms / 1000) as libc::time_t;
        // SAFETY: localtime_r fills the struct it's given; a zeroed tm is a valid one.
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        unsafe { libc::localtime_r(&secs, &mut tm) };
        assert_eq!((tm.tm_year + 1900, tm.tm_mon + 1, tm.tm_mday, tm.tm_hour, tm.tm_min, tm.tm_sec), (2026, 10, 14, 17, 45, 37));
    }

    #[test]
    fn shapes_text() {
        assert_eq!(one_line("\n  hello   world \nsecond", 40), "hello world");
        assert_eq!(truncate("abcdefghij", 5), "abcd…");
        assert_eq!(code("a`b"), "`a'b`");
        assert_eq!(one_line("Implemented in [#1017](https://github.com/x/y/pull/1017) and **merged**", 80), "Implemented in #1017 and merged");
        assert_eq!(one_line(":::writing{variant=\"standard\" id=\"83147\"}\nW&M 3 is ready.\n:::", 40), "W&M 3 is ready.");
        assert_eq!(one_line(":::\n\nAfter the block", 40), "After the block");
    }

    #[test]
    fn finds_where_commands_run() {
        let base = "/Users/me/projects/app";
        assert_eq!(command_dir("npm test", base).as_deref(), Some(base));
        assert_eq!(command_dir("cd /Users/me/projects/app-fix && npm test", base).as_deref(), Some("/Users/me/projects/app-fix"));
        assert_eq!(command_dir("cd ../app-fix; npm test", base).as_deref(), Some("/Users/me/projects/app-fix"));
        assert_eq!(command_dir("  cd \"../my app\" && make", base).as_deref(), Some("/Users/me/projects/my app"));
        assert_eq!(command_dir("cd ../my\\ app && make", base).as_deref(), Some("/Users/me/projects/my app"));
        assert_eq!(command_dir("cd src/ui && npx vitest run", base).as_deref(), Some("/Users/me/projects/app/src/ui"));
        assert_eq!(command_dir("cd ~/projects/x", base), Some(format!("{}/projects/x", home().display())));
        assert_eq!(command_dir("cd $WORKTREE && ls", base), None);
        assert_eq!(command_dir("cd - && ls", base), None);
        assert_eq!(command_dir("cdk deploy", base).as_deref(), Some(base));
        // Without a `cd`, a command that names a path elsewhere isn't work here.
        assert_eq!(command_dir("cat /private/tmp/claude-501/x/scratchpad/notes.md", base), None);
        assert_eq!(command_dir("S=/private/tmp/scratch; ls $S", base), None);
        assert_eq!(command_dir("git -C /Users/me/projects/app-fix log --oneline", base), None);
        assert_eq!(command_dir("cat ../app-fix/README.md", base), None);
        assert_eq!(command_dir("ls ~/.claude/projects", base), None);
        assert_eq!(command_dir("python3 - <<'EOF'\nopen('/Users/me/projects/app-fix/a.ts')\nEOF", base), None);
        // Paths inside it, /dev/null, URLs and flags that only look like paths are fine.
        assert_eq!(command_dir("cat /Users/me/projects/app/src/a.ts", base).as_deref(), Some(base));
        assert_eq!(command_dir("npm test > /dev/null 2>&1", base).as_deref(), Some(base));
        assert_eq!(command_dir("curl -s https://example.com/api/x | jq .", base).as_deref(), Some(base));
        assert_eq!(command_dir("gh pr merge 10 --repo sprucehq/vault --squash", base).as_deref(), Some(base));
        assert_eq!(command_dir("sed -i '' 's/old/new/' src/a.ts", base).as_deref(), Some(base));
        // A leading `cd` says where it works, whatever else the command names.
        assert_eq!(command_dir("cd ../app-fix && cargo build > /tmp/build.log", base).as_deref(), Some("/Users/me/projects/app-fix"));
        // Going somewhere only to look isn't work there: an agent reads another repo that way.
        assert_eq!(command_dir("cd ../docs && git show origin/main:AGENTS.md | head -200", base), None);
        assert_eq!(command_dir("cd /Users/me/projects/docs && git log -3 --format='%h %s' origin/main -- reviews | head -80; git show origin/main:a.yaml | head -40", base), None);
        assert_eq!(command_dir("cd ../docs && ls scripts tools 2>/dev/null | head -80; git ls-tree -r --name-only origin/main | grep -i -E \"upload|slot\"", base), None);
        assert_eq!(command_dir("cd .claude/worktrees/fix && git status --short", base), None);
        assert_eq!(command_dir("cd ../docs && git show origin/main:a.pdf > /tmp/a.pdf", base).as_deref(), Some("/Users/me/projects/docs"), "it writes a file");
        assert_eq!(command_dir("cd ../docs && git status && git commit -m 'Fix'", base).as_deref(), Some("/Users/me/projects/docs"));
        // Looking where the shell already is counts as it does without the `cd`.
        assert_eq!(command_dir("cd /Users/me/projects/app && git status", base).as_deref(), Some(base));
    }

    #[test]
    fn tells_looking_from_work() {
        for script in [
            "ls -la src",
            "git status --short --branch; git log --oneline -3",
            "git -C ../other --no-pager show origin/main:README.md | sed -n '120,170p'",
            "git fetch origin --quiet && git diff --stat origin/main...HEAD",
            "grep -rn \"a > b\" src --include='*.rs' 2>/dev/null | grep -v node_modules | cut -c1-260 | head -50",
            "S=/tmp/scratch; cat $S/notes.md",
            "git branch -a; git worktree list; git stash list; git remote -v",
            "find . -name '*.md' -newer README.md | wc -l",
            "gh pr view 56 --json title,body; gh run list --limit 5; gh pr checks 56",
            "cat a.txt 2>&1 >/dev/null # > not a file",
            "",
        ] {
            assert!(only_looks(script), "{script}");
        }
        for script in [
            "npm test",
            "git commit -m 'Fix'",
            "git status && git push origin HEAD:main",
            "git branch -m renamed",
            "git branch fix origin/main",
            "git worktree add ../fix -b fix",
            "git stash",
            "ls > files.txt",
            "cat a.txt >> b.txt",
            "sed -i '' 's/old/new/' src/a.ts",
            "find . -name '*.orig' -delete",
            "cat <<'EOF' | sh\nrm -rf build\nEOF",
            "echo \"$(rm -rf build)\"",
            "ls `make clean`",
            "for f in *.md; do cat $f; done",
            "gh pr merge 10 --squash",
            "python3 scripts/check.py",
        ] {
            assert!(!only_looks(script), "{script}");
        }
    }

    #[test]
    fn drops_a_leading_cd_from_commands() {
        assert_eq!(without_cd("cd /Users/me/projects/app-fix && scripts/preview.sh worktrees"), "scripts/preview.sh worktrees");
        assert_eq!(without_cd("cd ../x; git status"), "git status");
        assert_eq!(without_cd("cd ../x"), "cd ../x");
        assert_eq!(without_cd("cd ../x || exit 1"), "cd ../x || exit 1");
        assert_eq!(without_cd("git status && cd .."), "git status && cd ..");
    }

    #[test]
    fn keeps_the_latest_places_of_work() {
        let mut work = Vec::new();
        for i in 0..20 {
            note_work(&mut work, format!("/w/{i}"));
        }
        assert_eq!(work.len(), WORK_KEPT);
        assert_eq!(work.last().map(String::as_str), Some("/w/19"));
    }
}

/// Calls `f` with each line of a file that has one of `needles` in it, read a line at a time:
/// transcripts run to hundreds of megabytes, and only a few lines in them matter.
pub fn each_line_with(path: &Path, needles: &[&str], mut f: impl FnMut(&str)) {
    use std::io::BufRead;
    let Ok(file) = std::fs::File::open(path) else { return };
    let mut r = std::io::BufReader::with_capacity(1 << 20, file);
    let finders: Vec<memchr::memmem::Finder> = needles.iter().map(|n| memchr::memmem::Finder::new(n.as_bytes())).collect();
    let mut line = Vec::new();
    loop {
        line.clear();
        match r.read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        if finders.iter().any(|x| x.find(&line).is_some()) {
            f(String::from_utf8_lossy(&line).trim_end());
        }
    }
}
