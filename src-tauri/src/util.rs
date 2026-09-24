//! Small helpers shared by the adapters: time, paths, bounded file reads and text shaping.

use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const DAY_MS: i64 = 24 * 60 * 60 * 1000;

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
    if p == "~" {
        return home();
    }
    if let Some(rest) = p.strip_prefix("~/") {
        return home().join(rest);
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
    let f = File::open(p).ok()?;
    let mut reader = BufReader::new(f.take(max));
    let mut line = String::new();
    reader.read_line(&mut line).ok()?;
    if line.is_empty() {
        None
    } else {
        Some(line)
    }
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
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// First non-empty line, whitespace collapsed, markdown links reduced to their text, cut to
/// `max` characters with an ellipsis.
pub fn one_line(s: &str, max: usize) -> String {
    let line = s.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
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

/// Makes text safe to put inside a backtick code span.
pub fn code(s: &str) -> String {
    format!("`{}`", s.replace('`', "'"))
}

pub fn file_name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rfc3339() {
        assert_eq!(parse_rfc3339_ms("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_rfc3339_ms("2026-09-23T14:59:54.748Z"), Some(1_790_175_594_748));
        assert_eq!(parse_rfc3339_ms("2026-09-23T17:32:03.758417Z"), Some(1_790_184_723_758));
        assert_eq!(parse_rfc3339_ms("2026-09-23T10:32:03-07:00"), Some(1_790_184_723_000));
        assert_eq!(parse_rfc3339_ms("nonsense"), None);
    }

    #[test]
    fn shapes_text() {
        assert_eq!(one_line("\n  hello   world \nsecond", 40), "hello world");
        assert_eq!(truncate("abcdefghij", 5), "abcd…");
        assert_eq!(code("a`b"), "`a'b`");
        assert_eq!(one_line("Implemented in [#1017](https://github.com/x/y/pull/1017) and **merged**", 80), "Implemented in #1017 and merged");
    }
}
