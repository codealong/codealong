//! Secrets kept for Codealong (Jev's API key, this device's key), by service name: in the login
//! keychain on macOS, through `security`, and on Linux in the desktop's Secret Service (GNOME
//! Keyring, KWallet), through libsecret's `secret-tool`, which a desktop without one makes fail.
//! Both are programs, so both ways compile and are tested on either OS. A secret never goes on a
//! command line: `security -i` and `secret-tool store` read it from stdin. The keychain's own
//! lookups are here too, for what other programs keep in it (Claude Code's sign-in).

use std::time::Duration;

use anyhow::Result;

use super::Os;
use crate::procs;

/// A locked keychain can hold `security` at a prompt; don't wait on it forever.
const TIMEOUT: Duration = Duration::from_secs(15);

/// The account Codealong files its items under: your user name.
pub fn account() -> String {
    std::env::var("USER").ok().filter(|u| !u.is_empty() && u.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))).unwrap_or_else(|| "codealong".into())
}

/// The secret kept for `service`, if there's one.
pub fn read(service: &str) -> Option<String> {
    let out = match super::current() {
        Os::MacOs => keychain_read(service, &account(), TIMEOUT)?,
        Os::Linux => procs::run("secret-tool", &["lookup", "service", service, "account", &account()], None, TIMEOUT).ok()?,
    };
    Some(out.trim().to_string()).filter(|s| !s.is_empty())
}

/// Whether there's one: the keychain says without the secret; the Secret Service only with it.
pub fn has(service: &str) -> bool {
    match super::current() {
        Os::MacOs => keychain_has(service, &account(), TIMEOUT),
        Os::Linux => read(service).is_some(),
    }
}

/// Keeps `secret` for `service`, in place of what was there. The caller checks it can be written
/// in `security`'s quotes.
pub fn save(service: &str, secret: &str) -> Result<()> {
    match super::current() {
        Os::MacOs => {
            let command = format!("add-generic-password -U -a \"{}\" -s \"{service}\" -l \"{service}\" -w \"{secret}\"\n", account());
            let out = procs::run_stdin("/usr/bin/security", &["-i"], &command, TIMEOUT)?;
            // `security -i` exits cleanly whatever its command did; errors come back as text.
            if out.to_lowercase().contains("error") {
                anyhow::bail!("The keychain didn't take it.");
            }
            Ok(())
        }
        Os::Linux => procs::run_stdin("secret-tool", &["store", &format!("--label={service}"), "service", service, "account", &account()], secret, TIMEOUT).map(drop),
    }
}

/// Takes away what's kept for `service`; nothing there is no error.
pub fn delete(service: &str) {
    let _ = match super::current() {
        Os::MacOs => procs::run("/usr/bin/security", &["delete-generic-password", "-a", &account(), "-s", service], None, TIMEOUT),
        Os::Linux => procs::run("secret-tool", &["clear", "service", service, "account", &account()], None, TIMEOUT),
    };
}

/// Whether the macOS keychain has an item for `service` and `account`, reading no secret.
pub fn keychain_has(service: &str, account: &str, timeout: Duration) -> bool {
    procs::run("/usr/bin/security", &["find-generic-password", "-a", account, "-s", service], None, timeout).is_ok()
}

/// The secret of the macOS keychain's item for `service` and `account`. A missing item fails
/// (exit 44); stdout, the secret, is never part of an error.
pub fn keychain_read(service: &str, account: &str, timeout: Duration) -> Option<String> {
    procs::run("/usr/bin/security", &["find-generic-password", "-a", account, "-w", "-s", service], None, timeout).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::os::as_if;

    /// What each OS's store is asked, and that the secret goes on stdin.
    #[test]
    fn a_secret_goes_to_each_oses_store_on_stdin() {
        let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let log = sent.clone();
        procs::stand_in::set(Some(Box::new(move |program, args, input, _| {
            log.borrow_mut().push(format!("{program} {} <{}", args.join(" "), input.unwrap_or("").trim()));
            Ok(if args.first() == Some(&"lookup") { "s3cret\n".into() } else { String::new() })
        })));
        let a = account();
        let linux = as_if(Os::Linux, || {
            save("Codealong TypeSafe", "s3cret").unwrap();
            (read("Codealong TypeSafe"), has("Codealong TypeSafe"))
        });
        as_if(Os::MacOs, || save("Codealong TypeSafe", "s3cret").unwrap());
        procs::stand_in::set(None);
        assert_eq!(linux, (Some("s3cret".into()), true));
        let sent = sent.borrow();
        assert_eq!(sent[0], format!("secret-tool store --label=Codealong TypeSafe service Codealong TypeSafe account {a} <s3cret"));
        assert!(sent[1].starts_with("secret-tool lookup service Codealong TypeSafe account "));
        assert!(!sent.iter().any(|l| l.split('<').next().unwrap().contains("s3cret")), "never on a command line: {sent:?}");
        assert_eq!(sent.last().unwrap(), &format!("/usr/bin/security -i <add-generic-password -U -a \"{a}\" -s \"Codealong TypeSafe\" -l \"Codealong TypeSafe\" -w \"s3cret\""));
    }
}
