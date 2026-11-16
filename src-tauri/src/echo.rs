//! Speakers or headphones, worked out from the sound: whether the microphone hears what the Mac
//! plays. A listening that hears both (listen.rs) has the others in a meeting twice over on
//! speakers, once from the Mac and once, quieter, through the microphone as you; its strip says
//! *on speakers* while this says so. Nothing is taken out of the microphone's sound for it: the
//! voice processing that did so silenced the Mac's audio and turned the call down (main.swift
//! `Mic`). What's taken out is the microphone's words that repeat the Mac's (`Repeats`).
//!
//! Nothing about the device settles it: a USB sound card or a dock's jack has either plugged
//! in. What does is how quiet the microphone ever gets. While the Mac plays loudly, a
//! microphone that hears it is never as quiet as it gets when the Mac is silent; one that
//! doesn't (headphones) is just as quiet in both, whenever you're not talking yourself. So the
//! two are compared at their quiet ends (`LOW`), which your own voice, loud as it is, doesn't
//! reach: you'd have to talk through four fifths of what the others say to hide it. Both are
//! taken a reading at a time, so that a sound of the room's own that comes and goes (typing)
//! weighs the same in each; and since what the Mac plays reaches the microphone's readings a
//! little late, the readings while it's loud are taken at each delay it might have (`REACH`),
//! and the delay that shows it best is the one that counts. Loudness is as the helper sends
//! it, about ten times a second for each source (`Event::Level`).
//!
//! Tried on spoken recordings mixed as a call would be (2026-11-10): the Mac heard at a fifth
//! of its level, a twentieth, and a thirtieth in a noisy room came out at 21, 5 and 1.9 times
//! the microphone's quiet; headphones at 1.0 to 1.1, a noisy room, a pair that leaks and typing
//! throughout included, however much of the time you talked. Following the loudness up and down (a correlation) told them
//! apart far worse, your own voice being most of what a microphone hears.
//!
//! What the Mac plays through is the first guess (`plays_through`): speakers that say they are
//! start as speakers, and a change of device starts over. What's heard then has the last word,
//! each time there's enough of it to go by, since headphones plugged into a dock's jack leave
//! the device as it was.

use std::collections::VecDeque;

/// Loudness is compared a tenth of a second at a time, over the last minute and a half.
const BIN_MS: i64 = 100;
const KEPT: usize = 900;

/// How many bins after the Mac is loud the microphone may hear it in: sound takes no time to
/// cross a desk, but the two sources are buffered and noted apart.
const REACH: usize = 3;

/// How much there must be to go by: four seconds of the Mac playing loudly, and two of it silent.
const LOUD_NEEDED: usize = 40;
const SILENT_NEEDED: usize = 20;

/// The quiet end of a set of bins: the level a fifth of them are under.
const LOW: f64 = 0.2;

/// The microphone hears the Mac when its quiet while the Mac is loud is this many times its
/// quiet while the Mac is silent, and this much over it.
const TIMES: f64 = 1.6;
const OVER: f64 = 0.0015;

/// How often it's judged, in bins.
const JUDGED_EVERY: i64 = 10;

#[derive(Clone, Copy, Default, Debug, PartialEq)]
struct Bin {
    them: Option<f32>,
    you: Option<f32>,
}

/// Whether one listening's microphone hears what its Mac plays.
#[derive(Default)]
pub struct Echo {
    speakers: bool,
    /// What the Mac plays through, by its id, as last said.
    output: Option<String>,
    /// The bin number of the first bin kept.
    first: i64,
    bins: VecDeque<Bin>,
    judged_at: i64,
}

impl Echo {
    /// Whether the microphone hears what the Mac plays: you're on speakers.
    pub fn speakers(&self) -> bool {
        self.speakers
    }

    /// What the Mac plays through, as the helper says it (its id and `kind`: "speakers",
    /// "headphones" or "unknown"): said as it starts hearing, and when it changes. A change
    /// starts over, from what the device says of itself; the same device said again (the
    /// helper started again) changes nothing.
    pub fn plays_through(&mut self, device: &str, kind: &str) {
        if self.output.as_deref() != Some(device) {
            self.output = Some(device.to_string());
            self.bins.clear();
            self.judged_at = 0;
            self.speakers = kind == "speakers";
        }
    }

    /// How loud a source was at `at` (ms): the microphone (`mine`) or what the Mac plays.
    pub fn heard(&mut self, mine: bool, level: f64, at: i64) {
        let bin = at.div_euclid(BIN_MS);
        if self.bins.is_empty() || bin < self.first {
            // The first reading, or a clock that went back: start from here.
            self.bins.clear();
            self.first = bin;
            self.judged_at = bin;
        }
        while self.first + (self.bins.len() as i64) <= bin {
            self.bins.push_back(Bin::default());
        }
        while self.bins.len() > KEPT {
            self.bins.pop_front();
            self.first += 1;
        }
        let Some(at_bin) = usize::try_from(bin - self.first).ok().and_then(|i| self.bins.get_mut(i)) else { return };
        let slot = if mine { &mut at_bin.you } else { &mut at_bin.them };
        let level = level.clamp(0.0, 1.0) as f32;
        *slot = Some(slot.map_or(level, |was| was.max(level)));
        if bin - self.judged_at < JUDGED_EVERY {
            return;
        }
        self.judged_at = bin;
        // Not the bins still filling: the newest are judged once what follows them is in. Too
        // little to go by leaves it as it was.
        let settled = self.bins.len().saturating_sub(REACH + 1);
        if let Some((loud, silent)) = quiets(self.bins.iter().take(settled).copied().collect::<Vec<_>>().as_slice()) {
            self.speakers = hears(loud, silent);
        }
    }
}

/// Whether a microphone as quiet as `loud` while the Mac plays loudly, and as `silent` while it
/// plays nothing, hears it.
fn hears(loud: f64, silent: f64) -> bool {
    loud >= silent * TIMES && loud - silent >= OVER
}

/// The level `q` of the way up a set of readings.
fn quantile(mut levels: Vec<f32>, q: f64) -> f64 {
    levels.sort_by(|a, b| a.total_cmp(b));
    levels.get(((levels.len() as f64 * q) as usize).min(levels.len().saturating_sub(1))).map_or(0.0, |&l| f64::from(l))
}

/// How quiet the microphone gets while the Mac plays loudly, and while it plays nothing: None
/// until there's enough of both to go by. What counts as loud and as nothing goes by how loud the
/// Mac gets, since a call's level is whatever its app and its volume make it.
fn quiets(bins: &[Bin]) -> Option<(f64, f64)> {
    let top = quantile(bins.iter().filter_map(|b| b.them).collect(), 0.9);
    let (loud, nothing) = ((top * 0.4).max(0.005), top * 0.05);
    // The microphone's reading each number of bins after one in which the Mac was loud, a list
    // for each delay; and its reading in each bin around which the Mac was silent.
    let mut during: Vec<Vec<f32>> = vec![Vec::new(); REACH + 1];
    let mut between = Vec::new();
    for i in 0..bins.len().saturating_sub(REACH) {
        if bins[i].them.is_some_and(|t| f64::from(t) >= loud) {
            for (lag, readings) in during.iter_mut().enumerate() {
                readings.extend(bins[i + lag].you);
            }
            continue;
        }
        let played: Vec<f64> = bins[i..=i + REACH].iter().filter_map(|b| b.them).map(f64::from).collect();
        // Silent after it and before it too: the last of what was played is still on its way.
        let silent = !played.is_empty() && played.iter().all(|&t| t <= nothing) && i >= REACH && bins[i - REACH..i].iter().filter_map(|b| b.them).all(|t| f64::from(t) <= nothing);
        if silent {
            between.extend(bins[i].you);
        }
    }
    if between.len() < SILENT_NEEDED {
        return None;
    }
    // The delay at which the microphone is least quiet while the Mac is loud is the one the
    // Mac's sound takes, if it reaches it at all; where it doesn't, they're all as quiet.
    let loudest = during.into_iter().filter(|readings| readings.len() >= LOUD_NEEDED).map(|readings| quantile(readings, LOW)).reduce(f64::max)?;
    Some((loudest, quantile(between, LOW)))
}

// What of the microphone's words repeats the Mac's.
//
// On speakers each thing the others say is heard twice, as Them from the Mac and as You through
// the microphone, the two transcribed apart and a little differently: a word misheard, a stretch
// cut in other places (one line of yours for three of theirs, or the other way about). So a line
// of yours is matched against the words of theirs said about when it was, in order (`common`),
// and what repeats them is left out of the transcript. Theirs are never left out: the Mac's sound
// is the clean one, and nothing of yours is in it.
//
// The two settle in either order, so a line of yours is held back a moment (`HOLD_MS`) for
// theirs to come, and what they're still in the middle of saying counts too, since a long stretch
// of theirs settles only once it ends. Lines are written in the order they settled, so one of
// theirs that settles meanwhile waits behind it.
//
// What gives a repeat away is words in the same order at the same time; what's left of yours
// around them is kept. A line that's mostly theirs, closely (`Verdict::Repeats::close`), proves
// the microphone hears the Mac, as loud speakers do (`Echo::speakers`). With neither, nothing is
// left out; once there's either, a looser repeat is too, and a word or two ("Yeah.") they said at
// that moment as well.

/// A settled line from one of a listening's sources, the microphone's (`mine`) or the Mac's, with
/// when its words were said, in ms.
#[derive(Clone, Debug, PartialEq)]
pub struct Said {
    pub mine: bool,
    pub text: String,
    pub at: i64,
    pub until: i64,
}

/// How long a line of yours is held back once it's settled, for the Mac's words of the same time
/// to settle too: they come within a second or so of the end of their stretch.
const HOLD_MS: i64 = 3000;

/// How far apart in time words of the two can be and still be the same sound: each source's
/// times are its own, buffered and turned into times of day apart.
const NEAR_MS: i64 = 2000;

/// How long the Mac's lines are kept to match against: far longer than any line is held.
const THEIRS_KEPT_MS: i64 = 60_000;

/// How long a close repeat proves the microphone hears the Mac, for lines that can't prove it
/// themselves.
const PROVEN_MS: i64 = 5 * 60_000;

/// How many words of yours before or after a repeat are yours, kept as your line: you talking as
/// the others stop or start. Fewer are taken for the microphone mishearing the ends of theirs,
/// unless they're said apart from them (`APART_MS`).
const OWN_WORDS: usize = 4;

/// How much sooner than the Mac's words of the time a line of yours must start, or later end, for
/// the words of yours there to be yours however few ("No, I can't." as they finish): the
/// microphone's mishearing of the ends of theirs starts and ends with them.
const APART_MS: i64 = 700;

/// How long before the first of the words they're in the middle of saying (`saying`) they began:
/// the model has them about a second behind the sound, and a word or two in.
const SAYING_LAG_MS: i64 = 2000;

/// What the Mac is in the middle of saying: its words so far, about when they began, and when they
/// last changed, which is about when the last of them was said.
struct Saying {
    text: String,
    since: i64,
    changed: i64,
}

/// A listening's lines on their way to its transcript, the microphone's held back until it's
/// known whether they repeat the Mac's.
#[derive(Default)]
pub struct Repeats {
    /// The Mac's lines of the last minute.
    theirs: VecDeque<Said>,
    saying: Option<Saying>,
    /// Each line in the order it settled, with when it may be written: at once for theirs.
    queue: VecDeque<(Said, i64)>,
    /// When a line of yours last repeated theirs closely.
    repeated_at: Option<i64>,
    /// Whether the microphone is heard to hear the Mac, as last said (`on_speakers`).
    speakers: bool,
}

impl Repeats {
    /// Whether you're on speakers, as the loudness of the two says (`Echo::speakers`). It stays as
    /// last said: an ending lets go of what tells it before the helper's last lines come.
    pub fn on_speakers(&mut self, speakers: bool) {
        self.speakers = speakers;
    }

    /// A line that settled: the lines that may be written now, in the order they settled.
    pub fn said(&mut self, said: Said, now: i64) -> Vec<Said> {
        if !said.mine {
            self.theirs.push_back(said.clone());
            while self.theirs.front().is_some_and(|t| now - t.until > THEIRS_KEPT_MS) {
                self.theirs.pop_front();
            }
        }
        let due = if said.mine { now + HOLD_MS } else { now };
        self.queue.push_back((said, due));
        self.due(now)
    }

    /// What a source is in the middle of saying, empty once it's settled or the source is lost.
    /// Only the Mac's counts.
    pub fn saying(&mut self, mine: bool, text: &str, now: i64) {
        if mine {
            return;
        }
        self.saying = match text.trim() {
            "" => None,
            text => Some(Saying { text: text.to_string(), since: self.saying.as_ref().map_or(now - SAYING_LAG_MS, |s| s.since), changed: now }),
        };
    }

    /// The lines that may be written by now.
    pub fn due(&mut self, now: i64) -> Vec<Said> {
        let mut out = Vec::new();
        while self.queue.front().is_some_and(|&(_, due)| due <= now) {
            let Some((said, _)) = self.queue.pop_front() else { break };
            out.extend(self.judged(said, now));
        }
        out
    }

    /// Every line still held back, judged on what has come: hearing has ended.
    pub fn rest(&mut self, now: i64) -> Vec<Said> {
        let mut out = Vec::new();
        while let Some((said, _)) = self.queue.pop_front() {
            out.extend(self.judged(said, now));
        }
        out
    }

    /// A line as it's to be written, if any of it is: one of yours without what repeats theirs.
    fn judged(&mut self, said: Said, now: i64) -> Option<Said> {
        if !said.mine {
            return Some(said);
        }
        let near = |at: i64, until: i64| at <= said.until + NEAR_MS && until >= said.at - NEAR_MS;
        let mut heard: Vec<&Said> = self.theirs.iter().filter(|t| near(t.at, t.until)).collect();
        heard.sort_by_key(|t| t.at);
        let mut theirs: Vec<String> = heard.iter().flat_map(|t| words(&t.text)).map(|(_, w)| w).collect();
        let mut from = heard.iter().map(|t| t.at).min();
        let mut until = heard.iter().map(|t| t.until).max();
        // What they're still saying, while it's still changing near this line: one that stopped
        // changing long ago is one whose source was lost before its words settled.
        if let Some(saying) = self.saying.as_ref().filter(|s| near(s.since, s.changed)) {
            theirs.extend(words(&saying.text).into_iter().map(|(_, w)| w));
            from = Some(from.map_or(saying.since, |from| from.min(saying.since)));
            // Its last words aren't in yet, so nothing of yours is past them.
            until = Some(i64::MAX);
        }
        let apart = (from.is_some_and(|from| said.at + APART_MS <= from), until.is_some_and(|until| said.until >= until.saturating_add(APART_MS)));
        let proven = self.speakers || self.repeated_at.is_some_and(|at| now - at < PROVEN_MS);
        match verdict(&said.text, &theirs, proven, apart) {
            Verdict::Yours => Some(said),
            Verdict::Repeats { kept, close } => {
                if close {
                    self.repeated_at = Some(now);
                }
                kept.map(|text| Said { text, ..said })
            }
        }
    }
}

/// What of a line of yours repeats the Mac's words of the time.
#[derive(Debug, PartialEq)]
enum Verdict {
    /// None of it, as far as can be told.
    Yours,
    /// It repeats them, but for the words of yours before or after, if any. `close` when it's
    /// mostly theirs, in so many of their words that it proves the microphone hears the Mac.
    Repeats { kept: Option<String>, close: bool },
}

/// What of `line` repeats `theirs`, the Mac's words of the time in order, given whether it's
/// proven the microphone hears the Mac, and whether the line starts sooner than theirs and ends
/// later (`apart`), each by enough that what's said then is yours.
fn verdict(line: &str, theirs: &[String], proven: bool, apart: (bool, bool)) -> Verdict {
    let mine = words(line);
    let n = mine.len();
    let plain: Vec<String> = mine.iter().map(|(_, w)| w.clone()).collect();
    // Words that match one at a time may be the same few common ones by chance; a run of them in
    // order isn't. A line of a word or two has only that.
    let shortest = if n <= 2 { 1 } else { 2 };
    let runs: Vec<(usize, usize)> = runs(&plain, theirs).into_iter().filter(|&(_, len)| len >= shortest).collect();
    let (Some(&(first, _)), Some(&(last_start, last_len))) = (runs.first(), runs.last()) else { return Verdict::Yours };
    let last = last_start + last_len;
    let matched: usize = runs.iter().map(|&(_, len)| len).sum();
    let (before, after) = (first, n - last);
    // Words between the runs are theirs misheard; those around them are yours when there are
    // enough of them, or they were said apart from theirs.
    let (own_before, own_after) = (before >= OWN_WORDS || (before > 0 && apart.0), after >= OWN_WORDS || (after > 0 && apart.1));
    let lost = (last - first - matched) + if own_before { 0 } else { before } + if own_after { 0 } else { after };
    let close = matched >= 4 && matched * 4 >= n * 3;
    // Too little of it theirs, or more of it left out than matched: it's yours, words in common
    // and all.
    if matched < n.min(3) || lost > matched || !(proven || close) {
        return Verdict::Yours;
    }
    let join = |part: &[(&str, String)]| part.iter().map(|(w, _)| *w).collect::<Vec<_>>().join(" ");
    let kept: Vec<String> = [own_before.then(|| join(&mine[..first])), own_after.then(|| join(&mine[last..]))].into_iter().flatten().collect();
    Verdict::Repeats { kept: (!kept.is_empty()).then(|| kept.join(" … ")), close }
}

/// A line's words, each as written and as compared: in lower case, without its punctuation.
fn words(line: &str) -> Vec<(&str, String)> {
    line.split_whitespace()
        .filter_map(|w| {
            let plain: String = w.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect();
            (!plain.is_empty()).then_some((w, plain))
        })
        .collect()
}

/// The runs of words `a` and `b` have in common, in the same order in both: where each starts in
/// `a`, and how long. The longest first, then the same on either side of it, as difflib finds
/// them: a run is what gives a repeat away, so none is broken up for a few more words in common
/// (the most in common would take "we can ship" of "we can we can ship" a word at a time).
fn runs(a: &[String], b: &[String]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut left = vec![(0, a.len(), 0, b.len())];
    while let Some((a_from, a_to, b_from, b_to)) = left.pop() {
        let (i, j, len) = longest(&a[a_from..a_to], &b[b_from..b_to]);
        if len == 0 {
            continue;
        }
        out.push((a_from + i, len));
        left.push((a_from, a_from + i, b_from, b_from + j));
        left.push((a_from + i + len, a_to, b_from + j + len, b_to));
    }
    out.sort_unstable();
    out
}

/// The longest run of words `a` and `b` have in common: where it starts in each, and how long, 0
/// for none. Of runs as long, the one that ends first.
fn longest(a: &[String], b: &[String]) -> (usize, usize, usize) {
    let mut best = (0, 0, 0);
    // How long the run in common ending at each word of `b` is, for the word of `a` before.
    let mut before = vec![0; b.len() + 1];
    for i in 0..a.len() {
        let mut here = vec![0; b.len() + 1];
        for j in 0..b.len() {
            if a[i] == b[j] {
                here[j + 1] = before[j] + 1;
                if here[j + 1] > best.2 {
                    best = (i + 1 - here[j + 1], j + 1 - here[j + 1], here[j + 1]);
                }
            }
        }
        before = here;
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A call, as the helper would send it: a reading for each source every tenth of a second
    /// or so. The other side talks in turns of a few seconds with pauses between, its loudness
    /// going up and down as speech does; you talk in some of the pauses, and over the others
    /// now and then. On speakers (`echo` of the Mac's level) the microphone hears the Mac a
    /// tenth of a second late; the room is never quieter than `room`.
    fn call(echo: f64, room: f64, you_talk: f64, seconds: i64) -> Vec<(bool, f64, i64)> {
        let mut seed = 0x2545_f491_4f6c_dd1du64;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed >> 11) as f64 / (1u64 << 53) as f64
        };
        let mut out = Vec::new();
        let mut played = vec![0.0f64; (seconds * 10) as usize + 2];
        for (i, level) in played.iter_mut().enumerate() {
            // Six seconds of talk, then four of nothing, and so on.
            let talking = (i / 10) % 10 < 6;
            *level = if talking { 0.03 + 0.12 * next() } else { 0.0004 * next() };
        }
        for i in 0..(seconds * 10) as usize {
            let at = 1_000_000 + i as i64 * 100;
            out.push((false, played[i], at));
            let their_turn = (i / 10) % 10 < 6;
            // Mostly in the pauses, and over the others one time in ten.
            let mine = if next() < if their_turn { you_talk * 0.1 } else { you_talk } { 0.08 + 0.2 * next() } else { 0.0 };
            let heard = if i > 0 { echo * played[i - 1] } else { 0.0 };
            // The microphone's readings come at their own pace, a little off the Mac's.
            out.push((true, room * (1.0 + 0.2 * next()) + heard + mine, at + 37));
        }
        out
    }

    /// Whether it came to call it speakers, and after how many seconds of the call.
    fn found(mut echo: Echo, call: &[(bool, f64, i64)]) -> Option<i64> {
        call.iter()
            .find(|&&(mine, level, at)| {
                echo.heard(mine, level, at);
                echo.speakers()
            })
            .map(|&(_, _, at)| (at - 1_000_000) / 1000)
    }

    /// Whether it was speakers once the readings were all in.
    fn after(echo: &mut Echo, call: &[(bool, f64, i64)], shift: i64) -> bool {
        for &(mine, level, at) in call {
            echo.heard(mine, level, at + shift);
        }
        echo.speakers()
    }

    #[test]
    fn a_microphone_that_hears_the_mac_is_on_speakers_however_much_you_talk() {
        for (echo, room, you_talk) in [(0.2, 0.002, 0.5), (0.05, 0.002, 0.5), (0.05, 0.004, 0.5), (0.2, 0.002, 0.0), (0.2, 0.002, 0.7), (0.1, 0.01, 0.5)] {
            let at = found(Echo::default(), &call(echo, room, you_talk, 90));
            assert!(at.is_some_and(|s| s <= 30), "echo {echo}, room {room}, you talk {you_talk}: {at:?}");
        }
    }

    #[test]
    fn what_of_the_mac_is_lost_in_the_room_isnt_called_speakers() {
        // Heard so faintly that it's hardly over the room's own noise: nothing a transcript
        // would have twice, and not enough to go by.
        for (echo, room) in [(0.01, 0.004), (0.02, 0.01)] {
            assert_eq!(found(Echo::default(), &call(echo, room, 0.5, 90)), None, "echo {echo}, room {room}");
        }
    }

    #[test]
    fn one_that_doesnt_is_on_headphones_whatever_the_room() {
        for (echo, room, you_talk) in [(0.0, 0.002, 0.5), (0.0, 0.01, 0.5), (0.0, 0.03, 0.7), (0.0, 0.002, 0.0), (0.004, 0.002, 0.5)] {
            assert_eq!(found(Echo::default(), &call(echo, room, you_talk, 90)), None, "echo {echo}, room {room}, you talk {you_talk}");
        }
    }

    #[test]
    fn a_rooms_own_noise_that_comes_and_goes_isnt_the_macs() {
        // Typing, on headphones: a fifth of a second of it, a fifth of quiet, and so on, whatever
        // the others are saying. As loud while the Mac plays as while it doesn't, measured alike.
        for (burst, room) in [(0.05, 0.002), (0.2, 0.002), (0.02, 0.006)] {
            let typing: Vec<_> = call(0.0, room, 0.0, 120).into_iter().map(|(mine, level, at)| (mine, if mine && (at / 200) % 2 == 0 { level + burst } else { level }, at)).collect();
            assert_eq!(found(Echo::default(), &typing), None, "bursts of {burst} in a room at {room}");
        }
        // And with you talking in the pauses as well.
        let both: Vec<_> = call(0.0, 0.002, 0.5, 120).into_iter().map(|(mine, level, at)| (mine, if mine && (at / 200) % 2 == 0 { level + 0.05 } else { level }, at)).collect();
        assert_eq!(found(Echo::default(), &both), None);
    }

    #[test]
    fn nothing_is_decided_until_theres_enough_of_both_to_go_by() {
        // Three seconds of a call is too little, on speakers as loud as they come.
        assert_eq!(found(Echo::default(), &call(0.3, 0.002, 0.5, 3)), None);
        // Only the microphone, or only the Mac: nothing to compare.
        let both = call(0.3, 0.002, 0.5, 90);
        for mine in [true, false] {
            let one: Vec<_> = both.iter().copied().filter(|&(m, _, _)| m == mine).collect();
            assert_eq!(found(Echo::default(), &one), None, "only {}", if mine { "the microphone" } else { "the Mac" });
        }
        // The Mac never silent (music under the talk): no quiet to compare with, so nothing's said.
        let never: Vec<_> = both.iter().map(|&(mine, level, at)| (mine, if mine { level } else { level.max(0.05) }, at)).collect();
        assert_eq!(found(Echo::default(), &never), None);
        assert_eq!(quiets(&[]), None);
    }

    #[test]
    fn what_the_mac_plays_through_is_the_first_guess_and_a_change_starts_over() {
        let mut echo = Echo::default();
        assert!(!echo.speakers());
        // A sound card says nothing of what's plugged into it: listened for.
        echo.plays_through("usb", "unknown");
        assert!(!echo.speakers());
        assert!(after(&mut echo, &call(0.2, 0.002, 0.5, 60), 0));
        // The same device said again (the helper started again) changes nothing.
        echo.plays_through("usb", "unknown");
        assert!(echo.speakers());
        // Headphones picked: over, and listened for again; nothing's heard through them.
        echo.plays_through("airpods", "headphones");
        assert!(!echo.speakers());
        assert!(!after(&mut echo, &call(0.0, 0.002, 0.5, 60), 100_000));
        // Speakers that say they are start as speakers.
        echo.plays_through("built-in", "speakers");
        assert!(echo.speakers());
    }

    #[test]
    fn what_is_heard_has_the_last_word() {
        // Headphones plugged into the dock's jack the speakers were on: the device stays, and
        // once the minute and a half it goes by is past the speakers, it's headphones.
        let mut echo = Echo::default();
        echo.plays_through("usb", "unknown");
        assert!(after(&mut echo, &call(0.2, 0.002, 0.5, 60), 0));
        assert!(!after(&mut echo, &call(0.0, 0.002, 0.5, 120), 60_000));
        // Speakers that say they are, that the microphone never hears (a display's, turned down).
        echo.plays_through("display", "speakers");
        assert!(echo.speakers());
        assert!(!after(&mut echo, &call(0.0, 0.002, 0.5, 90), 200_000));
    }

    #[test]
    fn the_measure_is_the_quiet_end_of_each() {
        assert!(hears(0.012, 0.002) && hears(0.008, 0.004));
        // As quiet in both, or louder by too little to be a voice.
        assert!(!hears(0.002, 0.002) && !hears(0.0022, 0.002) && !hears(0.0105, 0.01) && !hears(0.0012, 0.0002));
        assert_eq!(quantile(vec![0.5, 0.1, 0.3, 0.2, 0.4], 0.2), f64::from(0.2f32));
        assert_eq!(quantile(Vec::new(), 0.2), 0.0);
    }

    // What of the microphone's words repeats the Mac's.

    /// Something that happens during a listening, at a time (ms): a line settles, or what the Mac
    /// is in the middle of saying changes.
    enum Heard {
        Said(Said),
        Saying(&'static str),
    }

    fn theirs(text: &str, at: i64, until: i64) -> Heard {
        Heard::Said(Said { mine: false, text: text.into(), at, until })
    }

    fn mine(text: &str, at: i64, until: i64) -> Heard {
        Heard::Said(Said { mine: true, text: text.into(), at, until })
    }

    /// The transcript that comes of what's heard, each at the time given, with lines let go of a
    /// tenth of a second at a time, as the microphone's loudness comes; then hearing ends.
    fn transcript(mut heard: Vec<(i64, Heard)>, speakers: bool) -> Vec<String> {
        heard.sort_by_key(|(at, _)| *at);
        let mut repeats = Repeats::default();
        repeats.on_speakers(speakers);
        let mut out = Vec::new();
        let mut heard = heard.into_iter().peekable();
        let mut now = 0;
        while heard.peek().is_some() {
            while let Some((_, happened)) = heard.next_if(|(at, _)| *at <= now) {
                match happened {
                    Heard::Said(said) => out.extend(repeats.said(said, now)),
                    Heard::Saying(text) => repeats.saying(false, text, now),
                }
            }
            out.extend(repeats.due(now));
            now += 100;
        }
        out.extend(repeats.rest(now));
        out.into_iter().map(|s| format!("{}: {}", if s.mine { "You" } else { "Them" }, s.text)).collect()
    }

    const EXPORT: &str = "So the export runs every night at two and writes to the shared bucket.";

    #[test]
    fn a_line_of_yours_that_repeats_theirs_is_left_out_whichever_settles_first() {
        let them = format!("Them: {EXPORT}");
        // Theirs first, then the microphone's, a word misheard.
        let heard = vec![(5000, theirs(EXPORT, 0, 4000)), (5200, mine("So the export runs every night at two and rights to the shared bucket.", 100, 4100))];
        assert_eq!(transcript(heard, false), vec![them.clone()]);
        // The microphone's first: held back until theirs has come.
        let heard = vec![(4800, mine("So the export runs every night at two and writes to the shared buckets.", 100, 4100)), (6500, theirs(EXPORT, 0, 4000))];
        assert_eq!(transcript(heard, false), vec![them]);
    }

    #[test]
    fn it_matches_lines_cut_in_other_places() {
        // Three lines of theirs for one of yours.
        let heard = vec![
            (3000, theirs("The first thing we need is the export.", 0, 2500)),
            (4300, theirs("Then the import after that.", 2600, 3800)),
            (6000, theirs("And the reports at the end.", 3900, 5500)),
            (6100, mine("The first thing we need is the exports, then the import after that and the reports at the end.", 0, 5500)),
        ];
        assert_eq!(transcript(heard, false), vec!["Them: The first thing we need is the export.", "Them: Then the import after that.", "Them: And the reports at the end."]);
        // Three of yours for one of theirs, the shortest left out once the first has shown the
        // microphone hears the Mac.
        let heard = vec![
            (2200, mine("So the export runs every night.", 0, 1800)),
            (3000, mine("At two.", 1900, 2600)),
            (4600, mine("And writes to the shared bucket.", 2700, 4000)),
            (4800, theirs(EXPORT, 0, 4000)),
        ];
        assert_eq!(transcript(heard, false), vec![format!("Them: {EXPORT}")]);
    }

    #[test]
    fn what_they_are_still_saying_counts() {
        // A long stretch of theirs settles well after the line of yours that repeats its start.
        let said = "Before we go on I want to walk through the numbers for the third quarter, and then the hiring plan.";
        let heard = vec![
            (1500, Heard::Saying("Before we go on")),
            (3000, Heard::Saying("Before we go on I want to walk through the")),
            (4200, mine("Before we go on, I want to walk through the numbers.", 0, 3800)),
            (5000, Heard::Saying("Before we go on I want to walk through the numbers for the third quarter")),
            (9000, Heard::Saying("")),
            (9000, theirs(said, 0, 8000)),
        ];
        assert_eq!(transcript(heard, false), vec![format!("Them: {said}")]);
        // What they were saying when their source was lost, its words never settled, counts only
        // near when it was said: a minute on, the same words from the microphone are yours.
        let words = "Before we go on I want to walk through the numbers.";
        let heard = vec![(1000, Heard::Saying("Before we go on I want to walk through the numbers")), (63_000, mine(words, 60_000, 62_500))];
        assert_eq!(transcript(heard, true), vec![format!("You: {words}")]);
        let mut repeats = Repeats::default();
        repeats.saying(false, "Before we go on I want to walk through the numbers", 1000);
        repeats.saying(false, "", 1500);
        assert!(repeats.saying.is_none());
    }

    #[test]
    fn a_word_or_two_they_said_too_goes_once_its_shown_the_microphone_hears_them() {
        let heard = || vec![(1000, theirs("Yeah.", 300, 700)), (1100, mine("Yeah.", 300, 700))];
        // Nothing yet says the microphone hears the Mac: you may have said it too.
        assert_eq!(transcript(heard(), false), vec!["Them: Yeah.", "You: Yeah."]);
        // Loud speakers say so.
        assert_eq!(transcript(heard(), true), vec!["Them: Yeah."]);
        // And so does a close repeat a moment before, for a few minutes.
        let after = |gap: i64| {
            let mut heard = vec![(5000, theirs(EXPORT, 0, 4000)), (5200, mine(EXPORT, 0, 4000))];
            heard.extend([(gap + 1000, theirs("Okay.", gap + 300, gap + 700)), (gap + 1100, mine("Okay.", gap + 300, gap + 700))]);
            transcript(heard, false)
        };
        assert_eq!(after(60_000), vec![format!("Them: {EXPORT}"), "Them: Okay.".into()]);
        assert_eq!(after(PROVEN_MS + 10_000), vec![format!("Them: {EXPORT}"), "Them: Okay.".into(), "You: Okay.".into()]);
        // A looser repeat too: words misheard in the middle are theirs.
        let launch = "We moved the launch to March because the vendor needs more time for the testing, so plan for that.";
        let heard = || vec![(5000, theirs(launch, 0, 4000)), (5100, mine("We moved the launch to March because the vendor knees bore thyme four tea sting, so plan for that.", 0, 4000))];
        assert_eq!(transcript(heard(), false).len(), 2);
        assert_eq!(transcript(heard(), true), vec![format!("Them: {launch}")]);
    }

    #[test]
    fn your_own_words_stay() {
        // Talking over them, a few words in common.
        let heard = vec![(4000, theirs("I think the plan is to ship on Friday.", 0, 3500)), (3800, mine("I think we should wait a week.", 500, 3000))];
        assert_eq!(transcript(heard, true), vec!["You: I think we should wait a week.", "Them: I think the plan is to ship on Friday."]);
        // Saying theirs back once they've finished.
        let heard = vec![(2000, theirs("Every night at two.", 0, 1500)), (5600, mine("Every night at two?", 4000, 5200))];
        assert_eq!(transcript(heard, true), vec!["Them: Every night at two.", "You: Every night at two?"]);
        // Answering as they finish: the microphone has both in one line, and your answer stays.
        let heard = vec![(2000, theirs("Can you send it over?", 0, 1500)), (5600, mine("Can you send it over? Yes, I'll send it tonight with the notes.", 0, 5200))];
        assert_eq!(transcript(heard, true), vec!["Them: Can you send it over?", "You: Yes, I'll send it tonight with the notes."]);
        // However short the answer, once it goes on past the end of theirs.
        let heard = vec![(2000, theirs("Can you send it over?", 0, 1500)), (3400, mine("Can you send it over? No, I can't.", 0, 3000))];
        assert_eq!(transcript(heard, true), vec!["Them: Can you send it over?", "You: No, I can't."]);
        // Where it ends with theirs, a word or two after the repeat is theirs misheard.
        let heard = vec![(2300, theirs("Can you send it over tonight?", 0, 1800)), (2400, mine("Can you send it over to night?", 0, 1900))];
        assert_eq!(transcript(heard, true), vec!["Them: Can you send it over tonight?"]);
        // And before they start.
        let heard = vec![(5000, theirs(EXPORT, 2000, 6000)), (6300, mine(&format!("Fine, and what about backups? {EXPORT}"), 0, 6000))];
        assert_eq!(transcript(heard, true), vec![format!("Them: {EXPORT}"), "You: Fine, and what about backups?".into()]);
        // A line of yours with nobody else talking.
        assert_eq!(transcript(vec![(2000, mine("Let's start with the roadmap.", 0, 1500))], true), vec!["You: Let's start with the roadmap."]);
    }

    #[test]
    fn lines_are_written_in_the_order_they_settled() {
        // Theirs, settling while yours is held back, waits behind it.
        let heard = vec![(1000, mine("What does it cost?", 0, 800)), (2500, theirs("About fifty a month.", 1200, 2200)), (2600, mine("Per seat?", 2300, 2550))];
        assert_eq!(transcript(heard, false), vec!["You: What does it cost?", "Them: About fifty a month.", "You: Per seat?"]);
        // What's held back when hearing ends is written then.
        let mut repeats = Repeats::default();
        assert_eq!(repeats.said(Said { mine: true, text: "Thanks, talk soon.".into(), at: 0, until: 900 }, 1000), vec![]);
        assert_eq!(repeats.rest(1200).len(), 1);
        assert_eq!(repeats.rest(1300), vec![]);
        // Judged then as on speakers if that was last said.
        repeats.on_speakers(true);
        repeats.said(Said { mine: false, text: "Okay.".into(), at: 2000, until: 2400 }, 2800);
        repeats.said(Said { mine: true, text: "Okay.".into(), at: 2000, until: 2400 }, 2900);
        assert_eq!(repeats.rest(3000), vec![]);
    }

    #[test]
    fn words_are_compared_without_their_case_or_punctuation() {
        assert_eq!(words("It's — Okay, OK?"), vec![("It's", "its".into()), ("Okay,", "okay".into()), ("OK?", "ok".into())]);
        let plain = |s: &str| words(s).into_iter().map(|(_, w)| w).collect::<Vec<_>>();
        assert_eq!(runs(&plain("a b c x d e"), &plain("a b c d e")), vec![(0, 3), (4, 2)]);
        // A run isn't broken up where the words it starts with come twice.
        assert_eq!(runs(&plain("we can ship friday"), &plain("well we can we can ship it")), vec![(0, 3)]);
        assert_eq!(runs(&plain("a b"), &plain("")), vec![]);
        assert_eq!(verdict("", &plain("a b"), true, (false, false)), Verdict::Yours);
    }
}
