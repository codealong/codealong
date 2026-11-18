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

    /// What the Mac plays through, by its id, as last said: None until it's been said.
    pub fn output(&self) -> Option<&str> {
        self.output.as_deref()
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
// the microphone, the two transcribed apart and differently. The microphone hears the Mac faintly,
// from across the desk, so what it writes is mostly theirs misheard: a few of their words, the
// ends of their sentences, "Kestral" for "Kestrel", "the at last report" for "the Atlas report", one line
// of yours for three of theirs or the other way about. Words compared as they're written miss
// most of that: on the first hour of a real call on speakers (2026-11-17) some fifty such lines
// were written as yours. So a line of yours is compared with the words of theirs said about when
// it was by how they sound (`sound`), letter by letter, where a word split in two or run together
// costs nothing, a vowel misheard little, and a word of theirs the microphone didn't catch less
// than a word of yours that isn't theirs (`heard_as`). What sounds like theirs is left out of the
// transcript. Theirs are never left out: the Mac's sound is the clean one, and nothing of yours
// is in it.
//
// The two settle in either order, so a line of yours is held back a moment (`HOLD_MS`) for
// theirs to come, and what they're still in the middle of saying counts too, since a long stretch
// of theirs settles only once it ends. Lines are written in the order they settled, so one of
// theirs that settles meanwhile waits behind it.
//
// A line of yours repeats theirs when all of it sounds like a stretch of theirs (`most`), or when
// it starts or ends with a close repeat (`EXACT`) and has words of yours past it: you answering as
// they finish, or starting before they do, which is kept. Nothing is taken from the middle of a
// line of yours: their words amid yours are you saying them. A line that sounds very like theirs
// (`CLOSE`) proves the microphone hears the Mac, as loud speakers do (`Echo::speakers`). With
// neither, nothing is left out; once there's either, a looser repeat is too, and a word or two
// ("Yeah.") they said at that moment as well. What the microphone is in the middle of hearing is
// shown without what repeats theirs too (`shown`), for the strip that shows it as it's said.

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
/// themselves, unless what the Mac plays through changes: a call's sound stays where it is. On the
/// hour on speakers close repeats came up to twenty minutes apart.
const PROVEN_MS: i64 = 30 * 60_000;

/// How many words of yours before or after a repeat are yours, kept as your line: you talking as
/// the others stop or start. Fewer are taken for the microphone mishearing the ends of theirs,
/// unless they're said apart from them (`APART_MS`). A repeat with words of yours past it is this
/// long at least, too: a phrase of theirs you say yourself is shorter.
const OWN_WORDS: usize = 4;

/// How much sooner than the Mac's words of the time a line of yours must start, or later end, for
/// the words of yours there to be yours however few ("No, I can't." as they finish): the
/// microphone's mishearing of the ends of theirs starts and ends with them.
const APART_MS: i64 = 700;

/// How far a repeat at one end of a line of yours may be from the same end of theirs and have been
/// heard as they said it: the two sources' times were up to a second apart on a real call.
const SAME_TIME_MS: i64 = 1500;

/// How long before the first of the words they're in the middle of saying (`saying`) they began:
/// the model has them about a second behind the sound, and a word or two in.
const SAYING_LAG_MS: i64 = 2000;

/// How far back what the Mac said is compared with what the microphone is in the middle of
/// hearing (`shown`), which began a few words ago.
const SHOWN_MS: i64 = 5000;

/// The least a line weighs as it sounds (`weight`) to be compared by its sound: two short words or
/// so. A sound shorter than that is found by chance in most of what anyone says, so a word or two
/// is compared as written.
const SHORTEST: f64 = 5.0;

/// How much of a line's sound may differ from a stretch of theirs, for its weight, for it to be
/// theirs misheard (`heard_as`): less for a short line, which chance matches more easily. On the
/// hour on speakers, sorted by hand, the lines of yours that were theirs misheard came out mostly
/// under 0.4, and none of your own under 0.42, the short ones none under 0.43.
fn most(weight: f64) -> f64 {
    if weight < SHORTEST {
        EXACT
    } else if weight < 8.0 {
        0.3
    } else {
        0.42
    }
}

/// As close as a line of yours, this long at least, must sound to theirs to prove the microphone
/// hears the Mac: none of your own lines on the hour came near it.
const CLOSE: f64 = 0.2;
const CLOSE_WEIGHT: f64 = 10.0;

/// As close as a repeat must sound to theirs for words of yours past it to be kept and the repeat
/// left out, where it's less than half the line.
const EXACT: f64 = 0.15;

/// What they're in the middle of saying: its words so far, about when they began, and when they
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
    /// What the Mac plays through, as last said.
    output: Option<String>,
}

impl Repeats {
    /// Whether you're on speakers, as the loudness of the two says (`Echo::speakers`), and what
    /// the Mac plays through (`Echo::output`): a close repeat proves nothing of another device.
    /// They stay as last said: an ending lets go of what tells them before the helper's last
    /// lines come.
    pub fn on_speakers(&mut self, speakers: bool, output: Option<&str>) {
        self.speakers = speakers;
        if output.is_some() && output != self.output.as_deref() {
            if self.output.is_some() {
                self.repeated_at = None;
            }
            self.output = output.map(str::to_string);
        }
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

    /// What to show of what the microphone is in the middle of hearing, `text`: all of it, or once
    /// the microphone is known to hear the Mac, what of it doesn't repeat what they're saying or
    /// just said. Nothing, when all of it does.
    pub fn shown(&self, text: &str, now: i64) -> String {
        if !self.proven(now) {
            return text.to_string();
        }
        let said = Said { mine: true, text: text.to_string(), at: now - SHOWN_MS, until: now };
        match verdict(&said, &self.theirs_near(&said), true) {
            Verdict::Yours => said.text,
            Verdict::Repeats { kept, .. } => kept.unwrap_or_default(),
        }
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

    /// Whether it's known by `now` that the microphone hears the Mac.
    fn proven(&self, now: i64) -> bool {
        self.speakers || self.repeated_at.is_some_and(|at| now - at < PROVEN_MS)
    }

    /// The Mac's words said about when line `said` of yours was, in order.
    fn theirs_near(&self, said: &Said) -> Vec<Theirs> {
        let near = |at: i64, until: i64| at <= said.until + NEAR_MS && until >= said.at - NEAR_MS;
        let mut heard: Vec<&Said> = self.theirs.iter().filter(|t| near(t.at, t.until)).collect();
        heard.sort_by_key(|t| t.at);
        let mut theirs: Vec<Theirs> = heard.iter().flat_map(|t| words(&t.text).into_iter().map(move |(_, plain)| Theirs { plain, at: t.at, until: t.until })).collect();
        // What they're still saying, while it's still changing near this line: one that stopped
        // changing long ago is one whose source was lost before its words settled. Its last words
        // aren't in yet, so nothing of yours is past them.
        if let Some(saying) = self.saying.as_ref().filter(|s| near(s.since, s.changed)) {
            theirs.extend(words(&saying.text).into_iter().map(|(_, plain)| Theirs { plain, at: saying.since, until: i64::MAX }));
        }
        theirs
    }

    /// A line as it's to be written, if any of it is: one of yours without what repeats theirs.
    fn judged(&mut self, said: Said, now: i64) -> Option<Said> {
        if !said.mine {
            return Some(said);
        }
        let theirs = self.theirs_near(&said);
        match verdict(&said, &theirs, self.proven(now)) {
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
    /// It repeats them, but for the words of yours before or after, if any. `close` when it
    /// sounds so like theirs that it proves the microphone hears the Mac.
    Repeats { kept: Option<String>, close: bool },
}

/// One of the Mac's words of the time a line of yours was said, as compared, with when the line
/// it's in was said.
struct Theirs {
    plain: String,
    at: i64,
    until: i64,
}

/// What of line `said` repeats `theirs`, the Mac's words of the time in order, given whether it's
/// proven the microphone hears the Mac.
fn verdict(said: &Said, theirs: &[Theirs], proven: bool) -> Verdict {
    let mine = words(&said.text);
    let n = mine.len();
    if n == 0 {
        return Verdict::Yours;
    }
    let sounds = Sounds::of(mine.iter().map(|(_, plain)| plain.as_str()));
    let all = sounds.weight(0..n);
    let repeats = |close: bool, kept: Option<String>| if proven || close { Verdict::Repeats { kept, close } } else { Verdict::Yours };
    let heard = Sounds::of(theirs.iter().map(|t| t.plain.as_str()));
    // A word or two: all of them theirs at the time as written, or as they sound, all but exactly.
    if all < SHORTEST {
        let theirs_too = mine.iter().all(|(_, word)| theirs.iter().any(|t| t.plain == *word)) || heard_as(&sounds.letters, &heard.letters).0 <= all * most(all);
        return if theirs_too { repeats(false, None) } else { Verdict::Yours };
    }
    // When the words of theirs at letters `span` of `heard` were said.
    let when = |span: (usize, usize)| {
        let said: Vec<&Theirs> = (heard.word(span.0)..=heard.word(span.1 - 1)).map(|w| &theirs[w]).collect();
        (said.iter().map(|t| t.at).min().unwrap_or(i64::MAX), said.iter().map(|t| t.until).max().unwrap_or(i64::MIN))
    };
    let join = |part: &[(&str, String)]| part.iter().map(|(w, _)| *w).collect::<Vec<_>>().join(" ");
    // You answering as they finish, or starting before they do: the stretch of the line that sounds
    // most like theirs is at one end of it, and the words past it are yours, by how many there are
    // or by being said apart from what it repeats.
    if let Some((first, last)) = likest(&sounds.letters, &heard.letters).map(|(from, to)| sounds.words_within(from, to)) {
        let (before, after) = (first, n - last);
        if (before == 0) != (after == 0) && last - first >= OWN_WORDS {
            let weight = sounds.weight(first..last);
            if let (cost, Some(span)) = heard_as(sounds.letters(first..last), &heard.letters) {
                let (from, until) = when(span);
                let own = if before > 0 { before >= OWN_WORDS || said.at + APART_MS <= from } else { after >= OWN_WORDS || said.until >= until.saturating_add(APART_MS) };
                // And the repeat was heard as they said it, at the line's end or its start: one
                // said after they did is you taking up their words ("we're certainly taking on new
                // projects" once they've asked whether you are).
                let as_they_said = if before > 0 { said.until <= until.saturating_add(SAME_TIME_MS) } else { said.at + SAME_TIME_MS >= from };
                let ratio = cost / weight;
                if own && as_they_said && (ratio <= EXACT || (ratio <= most(weight) && weight * 2.0 >= all)) {
                    let close = ratio <= CLOSE && weight >= CLOSE_WEIGHT && weight * 4.0 >= all * 3.0;
                    return repeats(close, Some(if before > 0 { join(&mine[..first]) } else { join(&mine[last..]) }));
                }
            }
        }
    }
    // All of it theirs, misheard, and over by the time they'd said it. (Not begun by then too: the
    // speech model's stretch of a faint sound can start long before the words it makes out.)
    let (cost, span) = heard_as(&sounds.letters, &heard.letters);
    let ratio = cost / all;
    if ratio > most(all) || span.is_some_and(|span| said.until > when(span).1.saturating_add(SAME_TIME_MS)) {
        return Verdict::Yours;
    }
    repeats(ratio <= CLOSE && all >= CLOSE_WEIGHT, None)
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

const DIGITS: [&str; 10] = ["zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine"];

/// A word as compared (`words`) as it sounds, roughly: numbers spelled out, and letters that sound
/// alike, or that the speech model takes for each other, made one (b and p, d and t, g and k, s
/// and z, f, ph and v, c as it's said), h and doubled letters dropped. It's not how English is
/// said, nor meant to be: both sides of a comparison are spelled the same way, so what matters is
/// that a word misheard comes out much as the word does.
fn sound(word: &str) -> Vec<char> {
    let mut plain: String = word.chars().map(|c| c.to_digit(10).map_or_else(|| c.to_string(), |d| DIGITS[d as usize].to_string())).collect();
    for (silent, sounds) in [("kn", "n"), ("wr", "r"), ("ps", "s"), ("pn", "n")] {
        if let Some(rest) = plain.strip_prefix(silent) {
            plain = format!("{sounds}{rest}");
        }
    }
    for (spelled, sounds) in [("ph", "f"), ("gh", ""), ("ck", "k"), ("qu", "kw"), ("x", "ks"), ("wh", "w"), ("th", "t"), ("sh", "s"), ("tch", "k"), ("ch", "k")] {
        plain = plain.replace(spelled, sounds);
    }
    let letters: Vec<char> = plain.chars().collect();
    let mut out: Vec<char> = Vec::new();
    for (i, &letter) in letters.iter().enumerate() {
        let letter = match letter {
            'c' if matches!(letters.get(i + 1), Some('e' | 'i' | 'y')) => 's',
            'c' | 'q' | 'g' => 'k',
            'z' => 's',
            'b' => 'p',
            'd' => 't',
            'v' => 'f',
            'y' if i > 0 => 'i',
            'h' => continue,
            other => other,
        };
        if out.last() != Some(&letter) {
            out.push(letter);
        }
    }
    out
}

fn vowel(letter: char) -> bool {
    matches!(letter, 'a' | 'e' | 'i' | 'o' | 'u')
}

/// How much a letter of a sound counts: a vowel half what another does, being the likeliest misheard.
fn weight(letter: char) -> f64 {
    if vowel(letter) {
        0.5
    } else {
        1.0
    }
}

/// What it costs to hear letter `a` as `b`.
fn misheard(a: char, b: char) -> f64 {
    if a == b {
        0.0
    } else if vowel(a) && vowel(b) {
        0.4
    } else {
        1.0
    }
}

/// What a letter of theirs the microphone didn't catch costs, for its weight: less than one of
/// yours that isn't theirs, the microphone catching only some of what the Mac plays.
const UNCAUGHT: f64 = 0.6;

/// Words as they sound, run together, with where each starts.
struct Sounds {
    letters: Vec<char>,
    /// Where each word's letters start, and then where the last ends.
    starts: Vec<usize>,
}

impl Sounds {
    fn of<'a>(words: impl Iterator<Item = &'a str>) -> Sounds {
        let mut out = Sounds { letters: Vec::new(), starts: Vec::new() };
        for word in words {
            out.starts.push(out.letters.len());
            out.letters.extend(sound(word));
        }
        out.starts.push(out.letters.len());
        out
    }

    fn letters(&self, words: std::ops::Range<usize>) -> &[char] {
        &self.letters[self.starts[words.start]..self.starts[words.end]]
    }

    fn weight(&self, words: std::ops::Range<usize>) -> f64 {
        self.letters(words).iter().map(|&l| weight(l)).sum()
    }

    /// The word letter `at` is in.
    fn word(&self, at: usize) -> usize {
        self.starts.partition_point(|&start| start <= at) - 1
    }

    /// The words of letters `from` to `to`: those with at least half their sound in them.
    fn words_within(&self, from: usize, to: usize) -> (usize, usize) {
        let words = self.starts.len() - 1;
        let within = |w: usize| {
            let (start, end) = (self.starts[w], self.starts[w + 1]);
            let inside: f64 = self.letters[start.max(from)..end.min(to).max(start.max(from))].iter().map(|&l| weight(l)).sum();
            end > start && inside * 2.0 >= self.weight(w..w + 1)
        };
        match ((0..words).find(|&w| within(w)), (0..words).rev().find(|&w| within(w))) {
            (Some(first), Some(last)) => (first, last + 1),
            _ => (0, 0),
        }
    }
}

/// The least it costs to hear `mine` as some stretch of `theirs`, and which letters of `theirs`
/// that is, None where it's best heard as nothing of theirs at all: each letter of yours misheard
/// as one of theirs costs `misheard`, one that isn't theirs its weight, and one of theirs not
/// caught `UNCAUGHT` of its weight. What comes before and after the stretch in theirs costs
/// nothing: the microphone caught a part of what they said.
fn heard_as(mine: &[char], theirs: &[char]) -> (f64, Option<(usize, usize)>) {
    // For the letters of yours so far, heard as theirs up to each letter: the least it costs, and
    // where in theirs it starts.
    let mut cost = vec![0.0; theirs.len() + 1];
    let mut from: Vec<usize> = (0..=theirs.len()).collect();
    let (mut next_cost, mut next_from) = (cost.clone(), from.clone());
    for &a in mine {
        next_cost[0] = cost[0] + weight(a);
        next_from[0] = 0;
        for (j, &b) in theirs.iter().enumerate() {
            let options = [(cost[j] + misheard(a, b), from[j]), (cost[j + 1] + weight(a), from[j + 1]), (next_cost[j] + UNCAUGHT * weight(b), next_from[j])];
            let (c, f) = options.into_iter().min_by(|x, y| x.0.total_cmp(&y.0)).unwrap_or((f64::MAX, 0));
            next_cost[j + 1] = c;
            next_from[j + 1] = f;
        }
        std::mem::swap(&mut cost, &mut next_cost);
        std::mem::swap(&mut from, &mut next_from);
    }
    let Some((end, &least)) = cost.iter().enumerate().min_by(|x, y| x.1.total_cmp(y.1)) else { return (0.0, None) };
    (least, (from[end] < end).then_some((from[end], end)))
}

/// The stretch of `mine` that sounds most like some stretch of `theirs`, as the letters of `mine`
/// it runs over; None where nothing does. A letter heard right counts its weight for it, and what
/// `heard_as` would cost counts against it.
fn likest(mine: &[char], theirs: &[char]) -> Option<(usize, usize)> {
    // For the letters of yours up to the one at hand, heard as theirs up to each letter: the best
    // score of a stretch ending there, and the letter of yours it starts at.
    let mut score = vec![0.0; theirs.len() + 1];
    let mut from = vec![0; theirs.len() + 1];
    let (mut next_score, mut next_from) = (score.clone(), from.clone());
    let mut best = (0.0, 0, 0);
    for (i, &a) in mine.iter().enumerate() {
        next_score[0] = 0.0;
        next_from[0] = i + 1;
        for (j, &b) in theirs.iter().enumerate() {
            let heard = if a == b { weight(a) } else { -misheard(a, b) };
            let options = [(score[j] + heard, from[j]), (score[j + 1] - weight(a), from[j + 1]), (next_score[j] - UNCAUGHT * weight(b), next_from[j])];
            let (s, f) = options.into_iter().max_by(|x, y| x.0.total_cmp(&y.0)).unwrap_or((0.0, 0));
            // Nothing so far is better than a stretch that does worse than nothing.
            let (s, f) = if s > 0.0 { (s, f) } else { (0.0, i + 1) };
            next_score[j + 1] = s;
            next_from[j + 1] = f;
            if s > best.0 {
                best = (s, f, i + 1);
            }
        }
        std::mem::swap(&mut score, &mut next_score);
        std::mem::swap(&mut from, &mut next_from);
    }
    (best.0 > 0.0).then_some((best.1, best.2))
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
        repeats.on_speakers(speakers, None);
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
        // Nor once the Mac plays through something else (headphones plugged in).
        let mut repeats = Repeats::default();
        repeats.on_speakers(false, Some("usb"));
        repeats.said(Said { mine: false, text: EXPORT.into(), at: 0, until: 4000 }, 4500);
        repeats.said(Said { mine: true, text: EXPORT.into(), at: 0, until: 4000 }, 4600);
        assert_eq!(repeats.due(8000), vec![]);
        let okay = |repeats: &mut Repeats, at: i64| {
            repeats.said(Said { mine: false, text: "Okay.".into(), at, until: at + 400 }, at + 800);
            repeats.said(Said { mine: true, text: "Okay.".into(), at, until: at + 400 }, at + 900);
            repeats.due(at + 5000).len()
        };
        repeats.on_speakers(false, None);
        assert_eq!(okay(&mut repeats, 10_000), 0, "the helper not there to say leaves it as it was");
        repeats.on_speakers(false, Some("airpods"));
        assert_eq!(okay(&mut repeats, 20_000), 1);
        // A looser repeat too: words misheard in the middle are theirs.
        let launch = "We moved the launch to March because the vendor needs more time for the testing, so plan for that.";
        let heard = || vec![(5000, theirs(launch, 0, 4000)), (5100, mine("We moved the launch to March because the window is the right one for us, so plan for that.", 0, 4000))];
        assert_eq!(transcript(heard(), false).len(), 2);
        assert_eq!(transcript(heard(), true), vec![format!("Them: {launch}")]);
        // Words misheard as words that sound like them are close, and prove it themselves.
        let heard = vec![(5000, theirs(launch, 0, 4000)), (5100, mine("We moved the launch to March because the vendor knees bore thyme four tea sting, so plan for that.", 0, 4000))];
        assert_eq!(transcript(heard, false), vec![format!("Them: {launch}")]);
    }

    #[test]
    fn what_the_microphone_mishears_of_theirs_is_left_out() {
        // As the microphone wrote down the others on a real call on speakers, made up: the end of
        // what they said, words misheard as others that sound like them, words split or run
        // together, words it didn't catch.
        for (them, you) in [
            ("We shipped the beta right at the end of the quarter.", "end of the quarter."),
            ("Honestly that's the upside, since we're a tiny crew and we're so quick.", "We are so quick."),
            ("It was minimal.", "It was criminal."),
            ("Team size, is that right?", "Team sites?"),
            ("Is it, um, is it, uh, per-seat, uh, Dana?", "Is it a per seat, Dana?"),
            ("How does Kestrel bill for it?", "How does Kestral bill?"),
            ("That's sort of their process.", "sort of the process."),
            ("I've got to run, folks.", "I've done a run, folks."),
            ("Pull the numbers from the Atlas Bayside report.", "the at last bay side report."),
            ("We mapped it all out, and that's what's going to happen.", "what it's gonna happen."),
        ] {
            let heard = vec![(4000, theirs(them, 0, 3500)), (4100, mine(you, 1000, 3500))];
            assert_eq!(transcript(heard, true), vec![format!("Them: {them}")], "{you}");
        }
        // All of it, though it starts a moment sooner: the words before what matched are theirs too.
        let heard = vec![(3000, theirs("Right, that, that is odd.", 1000, 2500)), (3100, mine("Right, that is odd.", 0, 2500))];
        assert_eq!(transcript(heard, true), vec!["Them: Right, that, that is odd."]);
        // One that sounds as close as these proves the microphone hears them by itself.
        let heard = vec![(4000, theirs("Is it, um, is it, uh, per-seat, uh, Dana?", 0, 3500)), (4100, mine("Is it a per seat, Dana?", 1000, 3500))];
        assert_eq!(transcript(heard, false), vec!["Them: Is it, um, is it, uh, per-seat, uh, Dana?"]);
        // What they're in the middle of saying is shown without what the microphone repeats of it.
        let mut repeats = Repeats::default();
        repeats.saying(false, "if you send it over first we could come ready with, like, this is just the draft", 10_000);
        assert_eq!(repeats.shown("like this is just the draft", 10_200), "like this is just the draft", "nothing proven yet");
        repeats.on_speakers(true, None);
        assert_eq!(repeats.shown("like this is just the draft", 10_200), "");
        assert_eq!(repeats.shown("Sure, I'll send the deck over this afternoon.", 10_200), "Sure, I'll send the deck over this afternoon.");
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
        // Whatever they go on to say while it's held back: past the end of what it repeats is
        // what counts, not of everything they said around then.
        let heard = vec![
            (2000, theirs("Can you send it over?", 0, 1500)),
            (3400, mine("Can you send it over? No, I can't.", 0, 3000)),
            (4600, Heard::Saying("All right")),
            (5000, Heard::Saying("")),
            (5000, theirs("All right.", 3600, 4200)),
        ];
        assert_eq!(transcript(heard, true), vec!["Them: Can you send it over?", "You: No, I can't.", "Them: All right."]);
        let heard = vec![(2000, theirs("Can you send it over?", 0, 1500)), (3400, mine("Can you send it over? No, I can't.", 0, 3000)), (4600, Heard::Saying("All right then"))];
        assert_eq!(transcript(heard, true), vec!["Them: Can you send it over?", "You: No, I can't."]);
        // Where it ends with theirs, a word or two after the repeat is theirs misheard.
        let heard = vec![(2300, theirs("Can you send it over tonight?", 0, 1800)), (2400, mine("Can you send it over to night?", 0, 1900))];
        assert_eq!(transcript(heard, true), vec!["Them: Can you send it over tonight?"]);
        // And before they start.
        let heard = vec![(5000, theirs(EXPORT, 2000, 6000)), (6300, mine(&format!("Fine, and what about backups? {EXPORT}"), 0, 6000))];
        assert_eq!(transcript(heard, true), vec![format!("Them: {EXPORT}"), "You: Fine, and what about backups?".into()]);
        // A line of yours with nobody else talking.
        assert_eq!(transcript(vec![(2000, mine("Let's start with the roadmap.", 0, 1500))], true), vec!["You: Let's start with the roadmap."]);
        // Their words amid yours, as you take them up: nothing is taken from the middle of a line.
        let them = "Could you say a bit more on the pricing model side?";
        let you = "Yeah, we have a whole pricing model page for that.";
        let heard = vec![(4000, theirs(them, 0, 3500)), (6000, mine(you, 3000, 5500))];
        assert_eq!(transcript(heard, true), vec![format!("Them: {them}"), format!("You: {you}")]);
        // Taking up their words once they've said them, at the end of a line of yours.
        let them = "I've got lots of ideas, I just don't know how much room you have for taking on new projects or not.";
        let you = "I mean, we're certainly taking on new projects.";
        let heard = vec![(9000, theirs(them, 0, 8500)), (12_500, mine(you, 9500, 12_000))];
        assert_eq!(transcript(heard, true), vec![format!("Them: {them}"), format!("You: {you}")]);
        // Short answers over a long stretch of theirs: a sound that short is found anywhere.
        let them = "So it walks through each link in the document, checks every one of them, and then whoever reviews it gets a list.";
        for you in ["Got it.", "Okay, yeah, yeah, yeah.", "You muted.", "Let's do it.", "Sounds great to me."] {
            let heard = vec![(9000, theirs(them, 0, 8500)), (6000, mine(you, 4000, 5500))];
            assert_eq!(transcript(heard, true), vec![format!("You: {you}"), format!("Them: {them}")], "{you}");
        }
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
        repeats.on_speakers(true, None);
        repeats.said(Said { mine: false, text: "Okay.".into(), at: 2000, until: 2400 }, 2800);
        repeats.said(Said { mine: true, text: "Okay.".into(), at: 2000, until: 2400 }, 2900);
        assert_eq!(repeats.rest(3000), vec![]);
    }

    #[test]
    fn words_are_compared_without_their_case_or_punctuation_and_by_how_they_sound() {
        assert_eq!(words("It's — Okay, OK?"), vec![("It's", "its".into()), ("Okay,", "okay".into()), ("OK?", "ok".into())]);
        assert_eq!(verdict(&Said { mine: true, text: String::new(), at: 0, until: 0 }, &[], true), Verdict::Yours);
        let sounds = |line: &str| Sounds::of(words(line).iter().map(|(_, w)| w.as_str())).letters;
        // Run together or apart, the same.
        assert_eq!(sounds("tonight"), sounds("to night"));
        assert_eq!(sounds("Bayside"), sounds("bay side"));
        assert_eq!(sounds("per-seat"), sounds("per seat"));
        // Letters that sound alike are one, and numbers are their words.
        assert_eq!(sounds("phone"), sounds("fone"));
        assert_eq!(sounds("2 sets"), sounds("two sets"));
        assert_eq!(sound("knee"), sound("nee"));
        // What hearing one as a stretch of another costs, for its weight.
        let ratio = |mine: &str, theirs: &str| {
            let mine = sounds(mine);
            heard_as(&mine, &sounds(theirs)).0 / mine.iter().map(|&l| weight(l)).sum::<f64>()
        };
        assert_eq!(ratio("the quarter", "at the end of the quarter we left"), 0.0);
        assert!(ratio("Kestral", "Kestrel") < 0.1, "{}", ratio("Kestral", "Kestrel"));
        assert!(ratio("it was criminal", "it was minimal") < most(10.0), "{}", ratio("it was criminal", "it was minimal"));
        assert!(ratio("we should wait a week", "the plan is to ship on Friday") > 0.5);
        assert_eq!(heard_as(&sounds("plan"), &sounds("the plan is")).1, Some((2, 6)));
        assert_eq!(heard_as(&sounds("plan"), &[]), (3.5, None));
        // The stretch of a line that sounds most like theirs.
        let line = sounds("can you send it over no I can't");
        let (from, to) = likest(&line, &sounds("Can you send it over?")).unwrap();
        let words_of = Sounds::of(words("can you send it over no I can't").iter().map(|(_, w)| w.as_str()));
        assert_eq!(words_of.words_within(from, to), (0, 5));
        assert_eq!(likest(&line, &[]), None);
    }
}
