//! `[appearance]` in config.toml: theme, window zoom, macOS glass, keyboard shortcut visibility,
//! and how tool calls appear in conversations.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AppearanceCfg {
    #[serde(default)]
    pub theme: Theme,
    /// How big the board's windows draw, in percent; ⌘= and ⌘- step it (zoom.rs).
    #[serde(default = "default_zoom")]
    pub zoom: u32,
    /// Glass (glass.rs): the main window and the project windows show what's behind them through
    /// macOS's own material, Liquid Glass from macOS 26 and the sidebar's vibrancy before it, with
    /// what was gray in them see-through over it. On by default on macOS, the only OS that has it.
    #[serde(default = "default_glass")]
    pub glass: bool,
    /// Keyboard shortcuts show beside the buttons and rows they work, and in the lines that say
    /// them, all the time: on by default. Off, they show while ⌘ or ⌃ is held (src/keysShown.ts),
    /// and tooltips and menus say them either way.
    #[serde(default = "default_show_keys")]
    pub show_keys: bool,
    /// How tool calls and intermediate work appear in a session's conversation (`src/toolDisplay.ts`,
    /// `src/turnFolds.ts`): folded into a "Worked for …" line per finished turn by default, compact
    /// 1-line tool summaries, detailed tool lines, or zen (prompts and final replies only).
    #[serde(default)]
    pub tools: ToolDisplay,
}

impl Default for AppearanceCfg {
    fn default() -> Self {
        AppearanceCfg { theme: Theme::default(), zoom: default_zoom(), glass: default_glass(), show_keys: default_show_keys(), tools: ToolDisplay::default() }
    }
}

/// How tool calls appear in a session's conversation (`[appearance] tools`).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ToolDisplay {
    /// Finished turns collapse their tool calls and intermediate replies into one expandable line.
    #[default]
    Folded,
    /// Consecutive tool calls collapse into one summary line each; intermediate replies stay visible.
    Compact,
    /// Individual tool lines, folding runs of three or more while keeping the latest call in view.
    Detailed,
    /// Prompts and final replies only; approvals, questions, errors, artifacts and sent files stay.
    Zen,
}

impl ToolDisplay {
    pub fn as_str(self) -> &'static str {
        match self {
            ToolDisplay::Folded => "folded",
            ToolDisplay::Compact => "compact",
            ToolDisplay::Detailed => "detailed",
            ToolDisplay::Zen => "zen",
        }
    }
}

/// Light or dark. `System` follows macOS, and changes when macOS does.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

impl Theme {
    pub fn as_str(self) -> &'static str {
        match self {
            Theme::System => "system",
            Theme::Light => "light",
            Theme::Dark => "dark",
        }
    }
}

/// Keys show all the time unless config.toml turns them off.
pub fn default_show_keys() -> bool {
    true
}

/// Glass is on unless config.toml turns it off, on macOS; elsewhere there's none to turn on.
pub(super) fn default_glass() -> bool {
    crate::os::is_macos()
}

pub(super) fn default_zoom() -> u32 {
    crate::zoom::ACTUAL
}
