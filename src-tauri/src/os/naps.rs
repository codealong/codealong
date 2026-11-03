//! App Nap: whether macOS may nap an app while other apps' windows cover it, and keeping one
//! awake (Settings › General's Keep … awake, for your terminal). Napped, every thread of an app
//! runs at the lowest priority there is (4, where the agents' work runs at 31 and up), and on a
//! busy Mac a napped terminal took seconds to answer Codealong: Ghostty 21 s over one question
//! (2026-10-12), Rex's app over 10 s to go to a session's tab (2026-11-03). Coming forward wakes
//! it; an Apple Event doesn't (`open -g` left Rex napping).
//!
//! An app is kept awake by `NSAppSleepDisabled` in its defaults, or in the global ones; its own
//! wins, a NO too. An app reads it as it starts: set while Ghostty ran, it changed nothing until
//! Ghostty restarted, and a test app launched with it never napped. So keeping one awake takes
//! once it opens again. It's the app's own setting, which `defaults` changes too, so it's asked
//! of the system each time and isn't kept in config.toml.
//!
//! Only macOS naps apps: elsewhere nothing naps, and keeping an app awake isn't offered.

use std::time::Duration;

use anyhow::Result;

use super::{current, Os};
use crate::procs;

const DEFAULTS: &str = "/usr/bin/defaults";
const KEY: &str = "NSAppSleepDisabled";

/// How long `defaults` gets: it answers at once.
const WAIT: Duration = Duration::from_secs(5);

/// Whether macOS may nap the app with this bundle ID (see the module header).
pub fn naps(bundle: &str) -> bool {
    match current() {
        Os::MacOs => napping(read(bundle), read("-g")),
        // Nothing naps there.
        Os::Linux => false,
    }
}

/// Whether the app is kept awake, where apps nap: None off macOS.
pub fn awake(bundle: &str) -> Option<bool> {
    (current() == Os::MacOs).then(|| !naps(bundle))
}

/// Keeps the app awake behind other windows, or lets macOS nap it again, from the next time it
/// opens. Let nap, its own setting goes, unless the global one keeps every app awake, when its
/// own NO is what lets it nap.
pub fn keep_awake(bundle: &str, awake: bool) -> Result<()> {
    if current() != Os::MacOs {
        return Err(super::only_on_macos("Keeping an app awake"));
    }
    if awake {
        procs::run(DEFAULTS, &["write", bundle, KEY, "-bool", "YES"], None, WAIT)?;
        return Ok(());
    }
    // Nothing to delete is no failure: it's let nap all the same.
    let _ = procs::run(DEFAULTS, &["delete", bundle, KEY], None, WAIT);
    if !naps(bundle) {
        procs::run(DEFAULTS, &["write", bundle, KEY, "-bool", "NO"], None, WAIT)?;
    }
    Ok(())
}

/// The setting in a domain's defaults: None when it has none, or `defaults` can't say.
fn read(domain: &str) -> Option<bool> {
    procs::run(DEFAULTS, &["read", domain, KEY], None, WAIT).ok().and_then(|v| sleep_disabled(&v))
}

/// Whether an app naps, from its own `NSAppSleepDisabled` and the global one: its own wins,
/// a NO too, as an app's defaults do over the global ones.
fn napping(own: Option<bool>, global: Option<bool>) -> bool {
    !own.or(global).unwrap_or(false)
}

/// `NSAppSleepDisabled` as `defaults read` prints it: 1 or 0 for a boolean, or a string that
/// `boolForKey` reads (`defaults write` without `-bool` stores `YES` as one). Anything else
/// counts as unset.
fn sleep_disabled(printed: &str) -> Option<bool> {
    match printed.trim().to_ascii_lowercase().as_str() {
        "1" | "yes" | "true" => Some(true),
        "0" | "no" | "false" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::rc::Rc;

    use super::*;
    use crate::os::as_if;

    #[test]
    fn an_app_naps_unless_its_own_default_or_the_global_one_says_not_to() {
        assert_eq!(["1\n", "YES", "true", "0\n", "NO", "", "maybe"].map(sleep_disabled), [Some(true), Some(true), Some(true), Some(false), Some(false), None, None]);
        assert!(napping(None, None));
        assert!(!napping(Some(true), None));
        assert!(!napping(None, Some(true)));
        // The app's own NO wins over the global YES.
        assert!(napping(Some(false), Some(true)));
        assert!(!napping(Some(true), Some(false)));
    }

    /// `defaults` as a stand-in over the domains given, each with its setting or none, writing
    /// and deleting them as it would; returns what it was asked, in order.
    fn played(domains: &[(&str, Option<&str>)], act: impl FnOnce()) -> Vec<String> {
        let held: Rc<RefCell<HashMap<String, String>>> = Rc::new(RefCell::new(domains.iter().filter_map(|(d, v)| Some((d.to_string(), (*v)?.to_string()))).collect()));
        let asked = Rc::new(RefCell::new(Vec::new()));
        let (h, a) = (held.clone(), asked.clone());
        procs::stand_in::set(Some(Box::new(move |program, args, _, _| {
            assert_eq!(program, DEFAULTS);
            a.borrow_mut().push(args.join(" "));
            let mut held = h.borrow_mut();
            match args {
                ["read", domain, KEY] => held.get(*domain).cloned().ok_or_else(|| anyhow::anyhow!("The domain/default pair of ({domain}, {KEY}) does not exist")),
                ["write", domain, KEY, "-bool", v] => {
                    held.insert(domain.to_string(), if *v == "YES" { "1" } else { "0" }.to_string());
                    Ok(String::new())
                }
                ["delete", domain, KEY] => held.remove(*domain).map(|_| String::new()).ok_or_else(|| anyhow::anyhow!("not found")),
                _ => panic!("unexpected defaults {args:?}"),
            }
        })));
        as_if(Os::MacOs, act);
        procs::stand_in::set(None);
        let out = asked.borrow().clone();
        out
    }

    #[test]
    fn keeping_an_app_awake_writes_its_own_default_and_letting_it_nap_takes_it_away() {
        const REX: &str = "com.superlogical.rex";
        // Napping as macOS has it by default; kept awake, its own YES.
        let asked = played(&[], || {
            assert_eq!(awake(REX), Some(false));
            keep_awake(REX, true).unwrap();
        });
        assert_eq!(asked, ["read com.superlogical.rex NSAppSleepDisabled", "read -g NSAppSleepDisabled", "write com.superlogical.rex NSAppSleepDisabled -bool YES"]);
        let asked = played(&[(REX, Some("1"))], || assert_eq!(awake(REX), Some(true)));
        assert_eq!(asked.len(), 2);
        // Let nap: its own setting goes, and with no global one that's all.
        let asked = played(&[(REX, Some("1"))], || {
            keep_awake(REX, false).unwrap();
            assert_eq!(awake(REX), Some(false));
        });
        assert_eq!(asked[..3], ["delete com.superlogical.rex NSAppSleepDisabled", "read com.superlogical.rex NSAppSleepDisabled", "read -g NSAppSleepDisabled"]);
        assert_eq!(asked.len(), 5, "no write: {asked:?}");
        // The global one keeps every app awake: its own NO lets this one nap.
        let asked = played(&[(REX, Some("1")), ("-g", Some("1"))], || {
            keep_awake(REX, false).unwrap();
            assert_eq!(awake(REX), Some(false));
        });
        assert_eq!(asked[3], "write com.superlogical.rex NSAppSleepDisabled -bool NO");
        // With none set, letting it nap deletes nothing, and fails at nothing.
        played(&[], || keep_awake(REX, false).unwrap());
    }

    #[test]
    fn off_macos_nothing_naps_and_nothing_is_kept_awake() {
        let asked = Rc::new(RefCell::new(0));
        let a = asked.clone();
        procs::stand_in::set(Some(Box::new(move |_, _, _, _| {
            *a.borrow_mut() += 1;
            Ok(String::new())
        })));
        let (naps, awake, kept) = as_if(Os::Linux, || (naps("x"), awake("x"), keep_awake("x", true).map_err(|e| e.to_string())));
        procs::stand_in::set(None);
        assert_eq!((naps, awake, kept), (false, None, Err("Keeping an app awake is only on macOS.".to_string())));
        assert_eq!(*asked.borrow(), 0);
    }
}
