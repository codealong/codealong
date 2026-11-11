//! Speakers or headphones, worked out from the sound: whether the microphone hears what the Mac
//! plays. A listening that hears both (listen.rs) has the others in a meeting twice over on
//! speakers, once from the Mac and once, quieter, through the microphone as you; the helper can
//! take the Mac's audio out of the microphone (ear.rs `Ear::echo`), and this says when to.
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
//! start with it taken out, and a change of device starts over. Once it's being taken out there
//! is nothing left to hear of it, so it stays until the device changes.

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
    /// Asked for from the start (`Ask::speakers`): there's nothing to work out.
    always: bool,
    cancelling: bool,
    /// What the Mac plays through, by its id, as last said.
    output: Option<String>,
    /// The bin number of the first bin kept.
    first: i64,
    bins: VecDeque<Bin>,
    judged_at: i64,
}

impl Echo {
    pub fn new(always: bool) -> Echo {
        Echo { always, cancelling: always, ..Echo::default() }
    }

    /// Whether the Mac's audio is to be taken out of the microphone now.
    pub fn cancelling(&self) -> bool {
        self.cancelling
    }

    /// What the Mac plays through, as the helper says it (its id and `kind`: "speakers",
    /// "headphones" or "unknown"): said as it starts hearing, and when it changes. A change
    /// starts over, from what the device says of itself. Returns whether to take the Mac's
    /// audio out, for the helper to be told either way: one started again knows nothing.
    pub fn plays_through(&mut self, device: &str, kind: &str) -> bool {
        if self.output.as_deref() != Some(device) {
            self.output = Some(device.to_string());
            self.bins.clear();
            self.judged_at = 0;
            self.cancelling = self.always || kind == "speakers";
        }
        self.cancelling
    }

    /// How loud a source was at `at` (ms): the microphone (`mine`) or what the Mac plays. True
    /// when this shows the microphone hears the Mac: its audio is taken out from here on.
    pub fn heard(&mut self, mine: bool, level: f64, at: i64) -> bool {
        if self.cancelling {
            return false;
        }
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
        let Some(at_bin) = usize::try_from(bin - self.first).ok().and_then(|i| self.bins.get_mut(i)) else { return false };
        let slot = if mine { &mut at_bin.you } else { &mut at_bin.them };
        let level = level.clamp(0.0, 1.0) as f32;
        *slot = Some(slot.map_or(level, |was| was.max(level)));
        if bin - self.judged_at < JUDGED_EVERY {
            return false;
        }
        self.judged_at = bin;
        // Not the bins still filling: the newest are judged once what follows them is in.
        let settled = self.bins.len().saturating_sub(REACH + 1);
        self.cancelling = quiets(self.bins.iter().take(settled).copied().collect::<Vec<_>>().as_slice()).is_some_and(|(loud, silent)| hears(loud, silent));
        self.cancelling
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

    /// Whether it came to take the Mac's audio out, and after how many seconds of the call.
    fn found(mut echo: Echo, call: &[(bool, f64, i64)]) -> Option<i64> {
        call.iter().find(|&&(mine, level, at)| echo.heard(mine, level, at)).map(|&(_, _, at)| (at - 1_000_000) / 1000)
    }

    #[test]
    fn a_microphone_that_hears_the_mac_is_on_speakers_however_much_you_talk() {
        for (echo, room, you_talk) in [(0.2, 0.002, 0.5), (0.05, 0.002, 0.5), (0.05, 0.004, 0.5), (0.2, 0.002, 0.0), (0.2, 0.002, 0.7), (0.1, 0.01, 0.5)] {
            let at = found(Echo::new(false), &call(echo, room, you_talk, 90));
            assert!(at.is_some_and(|s| s <= 30), "echo {echo}, room {room}, you talk {you_talk}: {at:?}");
        }
    }

    #[test]
    fn what_of_the_mac_is_lost_in_the_room_isnt_called_speakers() {
        // Heard so faintly that it's hardly over the room's own noise: nothing a transcript
        // would have twice, and not enough to go by.
        for (echo, room) in [(0.01, 0.004), (0.02, 0.01)] {
            assert_eq!(found(Echo::new(false), &call(echo, room, 0.5, 90)), None, "echo {echo}, room {room}");
        }
    }

    #[test]
    fn one_that_doesnt_is_on_headphones_whatever_the_room() {
        for (echo, room, you_talk) in [(0.0, 0.002, 0.5), (0.0, 0.01, 0.5), (0.0, 0.03, 0.7), (0.0, 0.002, 0.0), (0.004, 0.002, 0.5)] {
            assert_eq!(found(Echo::new(false), &call(echo, room, you_talk, 90)), None, "echo {echo}, room {room}, you talk {you_talk}");
        }
    }

    #[test]
    fn a_rooms_own_noise_that_comes_and_goes_isnt_the_macs() {
        // Typing, on headphones: a fifth of a second of it, a fifth of quiet, and so on, whatever
        // the others are saying. As loud while the Mac plays as while it doesn't, measured alike.
        for (burst, room) in [(0.05, 0.002), (0.2, 0.002), (0.02, 0.006)] {
            let typing: Vec<_> = call(0.0, room, 0.0, 120).into_iter().map(|(mine, level, at)| (mine, if mine && (at / 200) % 2 == 0 { level + burst } else { level }, at)).collect();
            assert_eq!(found(Echo::new(false), &typing), None, "bursts of {burst} in a room at {room}");
        }
        // And with you talking in the pauses as well.
        let both: Vec<_> = call(0.0, 0.002, 0.5, 120).into_iter().map(|(mine, level, at)| (mine, if mine && (at / 200) % 2 == 0 { level + 0.05 } else { level }, at)).collect();
        assert_eq!(found(Echo::new(false), &both), None);
    }

    #[test]
    fn nothing_is_decided_until_theres_enough_of_both_to_go_by() {
        // Three seconds of a call is too little, on speakers as loud as they come.
        assert_eq!(found(Echo::new(false), &call(0.3, 0.002, 0.5, 3)), None);
        // Only the microphone, or only the Mac: nothing to compare.
        let both = call(0.3, 0.002, 0.5, 90);
        for mine in [true, false] {
            let one: Vec<_> = both.iter().copied().filter(|&(m, _, _)| m == mine).collect();
            assert_eq!(found(Echo::new(false), &one), None, "only {}", if mine { "the microphone" } else { "the Mac" });
        }
        // The Mac never silent (music under the talk): no quiet to compare with, so nothing's said.
        let never: Vec<_> = both.iter().map(|&(mine, level, at)| (mine, if mine { level } else { level.max(0.05) }, at)).collect();
        assert_eq!(found(Echo::new(false), &never), None);
        assert_eq!(quiets(&[]), None);
    }

    #[test]
    fn what_the_mac_plays_through_is_the_first_guess_and_a_change_starts_over() {
        let mut echo = Echo::new(false);
        assert!(!echo.cancelling());
        // A sound card says nothing of what's plugged into it: listened for.
        assert!(!echo.plays_through("usb", "unknown"));
        let speakers = call(0.2, 0.002, 0.5, 60);
        assert!(speakers.iter().any(|&(mine, level, at)| echo.heard(mine, level, at)));
        assert!(echo.cancelling());
        // Taken out, there's nothing left to hear of it: it stays, the same device said again
        // (the helper started again) included.
        assert!(echo.plays_through("usb", "unknown"));
        assert!(!speakers.iter().any(|&(mine, level, at)| echo.heard(mine, level, at + 100_000)));
        assert!(echo.cancelling());
        // Headphones plugged in: over, and listened for again; nothing's heard through them.
        assert!(!echo.plays_through("airpods", "headphones"));
        assert!(!call(0.0, 0.002, 0.5, 60).iter().any(|&(mine, level, at)| echo.heard(mine, level, at + 200_000)));
        // Speakers that say they are start with it taken out.
        assert!(echo.plays_through("built-in", "speakers"));
        assert!(echo.cancelling());
    }

    #[test]
    fn asked_for_from_the_start_its_always_taken_out() {
        let mut echo = Echo::new(true);
        assert!(echo.cancelling());
        assert!(echo.plays_through("airpods", "headphones"));
        assert!(!call(0.0, 0.002, 0.5, 30).iter().any(|&(mine, level, at)| echo.heard(mine, level, at)));
        assert!(echo.cancelling());
    }

    #[test]
    fn the_measure_is_the_quiet_end_of_each() {
        assert!(hears(0.012, 0.002) && hears(0.008, 0.004));
        // As quiet in both, or louder by too little to be a voice.
        assert!(!hears(0.002, 0.002) && !hears(0.0022, 0.002) && !hears(0.0105, 0.01) && !hears(0.0012, 0.0002));
        assert_eq!(quantile(vec![0.5, 0.1, 0.3, 0.2, 0.4], 0.2), f64::from(0.2f32));
        assert_eq!(quantile(Vec::new(), 0.2), 0.0);
    }
}
