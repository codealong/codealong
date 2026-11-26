//! `[sessions]` keys set one at a time from outside Settings: the quick panel's switch for
//! whether a session it starts is shown as it starts (`show_started`).

use anyhow::Result;

/// Sets `[sessions] show_started` in config.toml and keeps everything else as written. The file
/// is read under its lock (`update`) and written as Settings writes it (`with_config`), so a
/// change saved a moment before isn't lost, and a file without a `[sessions]` table gets one,
/// commented as a new file would be.
pub fn save_show_started(on: bool) -> Result<()> {
    super::update(|text| {
        let mut cfg = super::parse(text)?;
        cfg.sessions.show_started = on;
        Ok((super::with_config(text, &cfg)?, ()))
    })
}

#[cfg(test)]
mod tests {
    use super::super::{parse, with_config};

    #[test]
    fn the_switch_reads_off_until_set_and_writes_back() {
        assert!(!parse("").unwrap().sessions.show_started, "off by default");
        let mut on = parse("").unwrap();
        on.sessions.show_started = true;
        let out = with_config("", &on).unwrap();
        assert!(out.contains("\nshow_started = true\n"), "{out}");
        assert!(parse(&out).unwrap().sessions.show_started);
        // Turned off again, the key stays, saying so.
        let off = with_config(&out, &parse("").unwrap()).unwrap();
        assert!(off.contains("\nshow_started = false\n"), "{off}");
    }
}
