//! What a program asks its terminal, taken out of what goes to a terminal attached to one of
//! Codealong's (terminals.rs `attach`). The server's emulator answers every question itself, so
//! a terminal shown the program's output as it is would answer again, through its keys, and the
//! program would read the second answer as typing: a cursor position asked once was answered
//! twice, the second with the attached terminal's coordinates.
//!
//! The questions: device attributes (`CSI c`, `CSI > c`, `CSI = c`), device status and cursor
//! position (`CSI n`, `CSI ? n`), the terminal's name and version (`CSI > q`), the Kitty keyboard
//! flags (`CSI ? u`), a mode's state (`CSI $ p`), sizes in cells and pixels (`CSI 14 t`, `16 t`,
//! `18 t`…), the terminal's identity the old way (`ESC Z`), colors and the clipboard (an OSC with
//! a `?` field: 4, 10 to 19, 52, and others, a title aside), and termcap and settings (`DCS + q`,
//! `DCS $ q`). An OSC that asks one color and sets another is taken out whole. Everything else
//! goes through as it is. A sequence split between two reads is held until its end comes.

/// What's held of a sequence not finished yet is let through past this, as it is: no question is
/// that long.
const MOST_HELD: usize = 4096;

/// Takes the questions out of one stream, across the chunks it comes in.
#[derive(Default)]
pub struct Queries {
    held: Vec<u8>,
}

impl Queries {
    /// `bytes` without the questions in them; a sequence they end partway through is held for the
    /// next chunk.
    pub fn strip(&mut self, bytes: &[u8]) -> Vec<u8> {
        let mut input = std::mem::take(&mut self.held);
        input.extend_from_slice(bytes);
        let mut out = Vec::with_capacity(input.len());
        let mut i = 0;
        while i < input.len() {
            if input[i] != 0x1b {
                out.push(input[i]);
                i += 1;
                continue;
            }
            match sequence(&input[i..]) {
                Seq::Whole { len, question } => {
                    if !question {
                        out.extend_from_slice(&input[i..i + len]);
                    }
                    i += len;
                }
                Seq::Rewritten { len, kept } => {
                    out.extend_from_slice(&kept);
                    i += len;
                }
                Seq::Partial if input.len() - i <= MOST_HELD => {
                    self.held = input[i..].to_vec();
                    return out;
                }
                Seq::Partial | Seq::Other => {
                    out.push(input[i]);
                    i += 1;
                }
            }
        }
        out
    }
}

enum Seq {
    /// A whole sequence of `len` bytes, and whether it asks something.
    Whole { len: usize, question: bool },
    /// A whole sequence of `len` bytes that asks and sets at once (a palette entry asked and
    /// another set): what it sets, as sequences of their own.
    Rewritten { len: usize, kept: Vec<u8> },
    /// It doesn't end in what's here.
    Partial,
    /// Not one this reads (an escape alone, or one of two bytes).
    Other,
}

/// The sequence `s` starts with (`s[0]` is ESC).
fn sequence(s: &[u8]) -> Seq {
    match s.get(1) {
        None => Seq::Partial,
        Some(b'[') => csi(s),
        Some(b']') => match string(s, osc_asks) {
            Seq::Whole { len, question: true } => osc_sets(&s[..len]).map_or(Seq::Whole { len, question: true }, |kept| Seq::Rewritten { len, kept }),
            other => other,
        },
        Some(b'P') => string(s, |body| body.starts_with(b"+q") || body.starts_with(b"$q")),
        // DECID, which asks what the terminal is, as `CSI c` does.
        Some(b'Z') => Seq::Whole { len: 2, question: true },
        Some(_) => Seq::Other,
    }
}

/// A CSI sequence: parameters (0x30-0x3f), intermediates (0x20-0x2f), then its final byte.
fn csi(s: &[u8]) -> Seq {
    let mut i = 2;
    while i < s.len() && (0x30..=0x3f).contains(&s[i]) {
        i += 1;
    }
    let params = &s[2..i];
    while i < s.len() && (0x20..=0x2f).contains(&s[i]) {
        i += 1;
    }
    let inter = &s[2 + params.len()..i];
    let Some(&last) = s.get(i) else { return Seq::Partial };
    if !(0x40..=0x7e).contains(&last) {
        return Seq::Other;
    }
    let lead = params.first().copied();
    let first: Option<u32> = std::str::from_utf8(params).ok().and_then(|p| p.trim_start_matches(['?', '>', '=', '<']).split([';', ':']).next()?.parse().ok());
    let question = match (last, inter) {
        (b'c', []) => true,
        (b'n', []) => true,
        (b'q', []) => lead == Some(b'>'),
        (b'u', []) => lead == Some(b'?'),
        (b'p', b"$") => true,
        (b't', []) => matches!(first, Some(11 | 13 | 14 | 15 | 16 | 18 | 19 | 20 | 21)),
        _ => false,
    };
    Seq::Whole { len: i + 1, question }
}

/// An OSC or DCS string, ended by BEL or ESC \, its body judged by `asks`.
fn string(s: &[u8], asks: impl Fn(&[u8]) -> bool) -> Seq {
    let mut i = 2;
    while i < s.len() {
        match s[i] {
            0x07 => return Seq::Whole { len: i + 1, question: asks(&s[2..i]) },
            0x1b if s.get(i + 1) == Some(&b'\\') => return Seq::Whole { len: i + 2, question: asks(&s[2..i]) },
            0x1b if i + 1 == s.len() => return Seq::Partial,
            _ => i += 1,
        }
    }
    Seq::Partial
}

/// What an OSC that asks also sets, as OSCs of their own ended as it was, where it sets
/// anything: the palette entries (4) and special colors (5) given a color beside those asked, and
/// the dynamic colors (10 to 19) given one, each its own number counted from the first. None where
/// it only asks.
fn osc_sets(seq: &[u8]) -> Option<Vec<u8>> {
    let (body, end): (&[u8], &[u8]) = if seq.ends_with(b"\x07") { (&seq[2..seq.len() - 1], b"\x07") } else { (&seq[2..seq.len() - 2], b"\x1b\\") };
    let mut fields = body.split(|b| *b == b';');
    let command: u32 = std::str::from_utf8(fields.next()?).ok()?.parse().ok()?;
    let rest: Vec<&[u8]> = fields.collect();
    let mut out = Vec::new();
    let mut emit = |number: u32, parts: &[&[u8]]| {
        out.extend_from_slice(format!("\x1b]{number}").as_bytes());
        for p in parts {
            out.push(b';');
            out.extend_from_slice(p);
        }
        out.extend_from_slice(end);
    };
    match command {
        4 | 5 => {
            let set: Vec<&[u8]> = rest.chunks(2).filter(|pair| pair.len() == 2 && pair[1] != b"?").flat_map(|pair| pair.iter().copied()).collect();
            if !set.is_empty() {
                emit(command, &set);
            }
        }
        10..=19 => {
            for (n, spec) in rest.iter().enumerate() {
                if *spec != b"?" {
                    emit(command + n as u32, &[spec]);
                }
            }
        }
        _ => {}
    }
    (!out.is_empty()).then_some(out)
}

/// An OSC that asks: one with a field that's `?` (a color, a palette entry among others set, the
/// clipboard, Claude Code's 7501), but never a title (0, 1, 2), which may be `?` itself.
fn osc_asks(body: &[u8]) -> bool {
    let mut fields = body.split(|b| *b == b';');
    let command = fields.next().and_then(|c| std::str::from_utf8(c).ok()).and_then(|c| c.parse::<u32>().ok());
    match command {
        Some(0..=2) | None => false,
        Some(_) => fields.any(|f| f == b"?"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strip(chunks: &[&[u8]]) -> Vec<u8> {
        let mut q = Queries::default();
        chunks.iter().flat_map(|c| q.strip(c)).collect()
    }

    #[test]
    fn takes_out_what_a_program_asks_and_leaves_the_rest() {
        // What Claude Code asks as it starts, between what it draws.
        let asked = b"\x1b[?2004h\x1b[>0q\x1b[?u\x1b]7501;?\x07\x1b[c\x1b[16thi \x1b[1;32mgreen\x1b[0m";
        assert_eq!(strip(&[asked]), b"\x1b[?2004hhi \x1b[1;32mgreen\x1b[0m");
        assert_eq!(strip(&[b"a\x1b[6nb\x1b[>cc\x1b[?996nd"]), b"abcd");
        assert_eq!(strip(&[b"\x1b]11;?\x1b\\\x1b]0;title\x07"]), b"\x1b]0;title\x07", "a color asked, a title set");
        assert_eq!(strip(&[b"\x1bP+q544e\x1b\\\x1bP1$r0m\x1b\\"]), b"\x1bP1$r0m\x1b\\");
        assert_eq!(strip(&[b"\x1b[?2026$p\x1b[!p"]), b"\x1b[!p", "a mode asked, a soft reset");
        assert_eq!(strip(&[b"\x1b]4;1;?;2;#00ff00\x1b\\x"]), b"\x1b]4;2;#00ff00\x1b\\x", "one palette entry asked, the other set kept");
        assert_eq!(strip(&[b"\x1b]10;?;#ffffff\x07"]), b"\x1b]11;#ffffff\x07", "the foreground asked, the background set");
        assert_eq!(strip(&[b"\x1b]4;1;?;3;?\x07"]), b"", "only asking");
        assert_eq!(strip(&[b"a\x1bZb"]), b"ab", "DECID");
        assert_eq!(strip(&[b"\x1b]2;?\x07\x1b]8;;https://example.com/?q\x07"]), b"\x1b]2;?\x07\x1b]8;;https://example.com/?q\x07", "a title that's ?, a link with ? in it");
        // What looks alike but sets things: the cursor's shape, the Kitty flags pushed, a window
        // title pushed, a size set.
        let sets: &[u8] = b"\x1b[2 q\x1b[>5u\x1b[22;0t\x1b[8;24;80t\x1b[5;10H";
        assert_eq!(strip(&[sets]), sets);
    }

    #[test]
    fn a_question_split_between_reads_is_held_until_its_end() {
        assert_eq!(strip(&[b"one\x1b[", b"6", b"ntwo"]), b"onetwo");
        assert_eq!(strip(&[b"\x1b]11;", b"?\x1b", b"\\x"]), b"x");
        assert_eq!(strip(&[b"\x1b[1;3", b"1mred"]), b"\x1b[1;31mred");
        // An escape alone at a read's end is held too, then let through with what follows.
        assert_eq!(strip(&[b"esc\x1b", b"x"]), b"esc\x1bx");
    }
}
