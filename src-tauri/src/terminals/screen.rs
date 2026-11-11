//! One terminal's emulator: Ghostty's (libghostty-vt), fed what its process prints, and the
//! screen that comes of it as the windows draw it (`Frame`): the rows that changed, each as runs
//! of cells drawn alike, every color resolved to RGB, with the cursor. The windows only draw:
//! what a key, a click, a paste or the wheel sends the program is worked out here, against the
//! terminal's modes as they are (application cursor keys, the Kitty keyboard protocol, mouse
//! reporting, bracketed paste), by libghostty's own encoders, and so is a selection.
//!
//! libghostty's types may not leave the thread that made them, so a `Screen` is made and used on
//! its terminal's thread (terminals.rs `run`).

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use libghostty_vt::fmt::{Extras, Format, Formatter};
use libghostty_vt::key::{self, Mods};
use libghostty_vt::mouse;
use libghostty_vt::paste;
use libghostty_vt::render::{CellIterator, CursorVisualStyle, Dirty, RenderState, RowIterator};
use libghostty_vt::screen::CellWide;
use libghostty_vt::selection::gesture::{Autoscroll, AutoscrollTickEvent, DragEvent, Geometry, Gesture, PressEvent, ReleaseEvent};
use libghostty_vt::style::{Palette, RgbColor, StyleColor, Underline};
use libghostty_vt::terminal::{ClipboardMime, ColorScheme, Mode, Point, PointCoordinate, ScrollViewport, SizeReportSize, Terminal};
use serde::{Deserialize, Serialize};

use super::keys;
use super::pty::Size;

/// The terminfo entry programs are told the terminal is, which every Mac and Linux has.
pub const TERM: &str = "xterm-256color";
/// How much scrollback a terminal keeps.
const SCROLLBACK_LINES: usize = 10_000;
/// How long two clicks may be apart to count as a double click, as macOS's default.
const CLICK_INTERVAL: Duration = Duration::from_millis(500);
/// How often a selection dragged past the top or bottom scrolls the view a row on, as Ghostty
/// does (its termio thread's `selection_scroll_ms`).
pub const AUTOSCROLL_EVERY: Duration = Duration::from_millis(15);
/// The most rows a selection dragged far past the edge scrolls in one tick: a row more for each
/// two rows' height the pointer is beyond it, so a long way back comes quicker than Ghostty's one.
const AUTOSCROLL_MOST: u32 = 8;

/// The most of a sequence still being read that a snapshot keeps: more than any a program sends
/// whole (a long title, a hyperlink), so one cut partway goes on in the server it's handed to.
const CONTINUATION_BYTES: usize = 64 * 1024;

/// The longest a program may hold the screen still while it redraws (synchronized output): one
/// that never lets go, having ended or hung halfway, is drawn after this as it stands, as
/// libghostty's docs and Ghostty itself do.
const HOLD_MAX: Duration = Duration::from_secs(1);

/// A screen as the windows draw it, or what changed of it since the last.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Frame {
    /// The terminal's id.
    pub id: String,
    /// Counts up with every frame of the terminal: a window that misses one asks for the whole.
    pub seq: u64,
    pub cols: u16,
    pub rows: u16,
    /// Every row is here: a window starts from it rather than from what it showed.
    pub full: bool,
    /// The rows that changed. A window clears each to the background and draws its runs.
    pub lines: Vec<Line>,
    /// The looks the runs name, by index.
    pub looks: Vec<Look>,
    /// Where the cursor shows, when it does.
    pub cursor: Option<Caret>,
    /// The default colors, as 0xRRGGBB.
    pub fg: u32,
    pub bg: u32,
    /// Where the view is in the scrollback.
    pub scroll: Scroll,
    /// The program asks for the mouse: clicks and the wheel go to it (Shift selects anyway).
    pub mouse: bool,
}

/// A row's cells that aren't blank, in runs drawn alike.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Line {
    pub y: u16,
    pub runs: Vec<Run>,
}

/// Cells side by side drawn alike, from column `x`: one entry a cell, its text (a grapheme), " "
/// for a blank one and "" for the second half of a wide character before it, or a wide one
/// wrapped to the next line, neither of which is drawn.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Run {
    pub x: u16,
    #[serde(rename = "l")]
    pub look: u16,
    #[serde(rename = "c")]
    pub cells: Vec<String>,
}

/// How cells are drawn: colors as 0xRRGGBB (None for the default), and `flags` (`BOLD`…).
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct Look {
    #[serde(rename = "f", skip_serializing_if = "Option::is_none")]
    pub fg: Option<u32>,
    #[serde(rename = "b", skip_serializing_if = "Option::is_none")]
    pub bg: Option<u32>,
    /// The underline's own color, when it has one.
    #[serde(rename = "u", skip_serializing_if = "Option::is_none")]
    pub ul: Option<u32>,
    #[serde(rename = "a", skip_serializing_if = "is_zero")]
    pub flags: u16,
}

fn is_zero(n: &u16) -> bool {
    *n == 0
}

pub const BOLD: u16 = 1;
pub const ITALIC: u16 = 1 << 1;
pub const FAINT: u16 = 1 << 2;
pub const BLINK: u16 = 1 << 3;
pub const STRIKE: u16 = 1 << 4;
pub const OVERLINE: u16 = 1 << 5;
pub const SELECTED: u16 = 1 << 6;
pub const LINK: u16 = 1 << 7;
/// The underline's kind, in three bits: 1 single, 2 double, 3 curly, 4 dotted, 5 dashed.
pub const UNDERLINE_SHIFT: u16 = 8;

/// The cursor, where it shows.
#[derive(Serialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Caret {
    pub x: u16,
    pub y: u16,
    /// "block", "bar", "underline" or "hollow".
    pub shape: &'static str,
    pub blink: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<u32>,
    /// A password is being typed (the terminal's echo is off).
    pub password: bool,
}

/// Where the view is in the scrollback, in rows: `offset` from the top of `total`, `len` shown.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Scroll {
    pub total: u64,
    pub offset: u64,
    pub len: u64,
}

/// A window's colors, which programs that ask are told (OSC 10, 11 and 4) and draw with.
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Theme {
    pub fg: u32,
    pub bg: u32,
    #[serde(default)]
    pub cursor: Option<u32>,
    /// The 16 ANSI colors; fewer leaves the rest Ghostty's.
    #[serde(default)]
    pub palette: Vec<u32>,
    /// Programs that ask whether the theme is dark are told this (CSI ? 996 n).
    #[serde(default)]
    pub dark: bool,
}

/// What a window sends a terminal. Pixel positions are from the grid's top left.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Input {
    /// A key, by `KeyboardEvent.code`, with the text it typed.
    Key {
        code: String,
        #[serde(default)]
        text: Option<String>,
        /// The character the key gives with no modifiers on your layout, where the window knows it
        /// (on AZERTY the key that is Q on a US keyboard gives a): what Ctrl and a letter go by.
        #[serde(default)]
        unshifted: Option<String>,
        #[serde(default)]
        mods: Mods4,
        #[serde(default)]
        action: KeyAction,
    },
    /// Text typed some other way: an input method's, or a dead key's.
    Text { text: String },
    /// A mouse button or the pointer moving.
    Mouse {
        action: MouseAction,
        #[serde(default)]
        button: Option<MouseButton>,
        x: f64,
        y: f64,
        #[serde(default)]
        mods: Mods4,
    },
    /// The wheel, in rows (up is negative), at a position.
    Wheel {
        rows: f64,
        x: f64,
        y: f64,
        #[serde(default)]
        mods: Mods4,
    },
    /// The window showing it took or lost the keyboard.
    Focus { on: bool },
    /// The view moved in the scrollback.
    Scroll { to: ScrollTo },
    /// Selects everything, or nothing.
    Select { all: bool },
}

#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(default)]
pub struct Mods4 {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool,
}

impl Mods4 {
    fn mods(self) -> Mods {
        let mut m = Mods::empty();
        m.set(Mods::SHIFT, self.shift);
        m.set(Mods::CTRL, self.ctrl);
        m.set(Mods::ALT, self.alt);
        m.set(Mods::SUPER, self.meta);
        m
    }
}

#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum KeyAction {
    #[default]
    Press,
    Repeat,
    Release,
}

#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum MouseAction {
    Down,
    Up,
    Move,
}

#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum MouseButton {
    Left,
    Middle,
    Right,
}

#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ScrollTo {
    Top,
    Bottom,
    PageUp,
    PageDown,
    /// The prompt before the one at the top of the view, which needs the shell to mark prompts.
    PromptUp,
    PromptDown,
}

/// What the terminal's program did that Codealong shows beside the screen, noted as the program
/// prints it (libghostty calls back from inside `vt_write`).
#[derive(Default)]
pub struct Noted {
    pub title: bool,
    pub bell: bool,
    pub pwd: bool,
}

/// One terminal's emulator and what it encodes input with.
pub struct Screen {
    term: Terminal<'static, 'static>,
    render: RenderState<'static>,
    rows: RowIterator<'static>,
    cells: CellIterator<'static>,
    key_encoder: key::Encoder<'static>,
    key_event: key::Event<'static>,
    mouse_encoder: mouse::Encoder<'static>,
    mouse_event: mouse::Event<'static>,
    gesture: Gesture<'static>,
    press: PressEvent<'static>,
    release: ReleaseEvent<'static>,
    drag: DragEvent<'static>,
    tick: AutoscrollTickEvent<'static>,
    /// Where the pointer was as a selection was last dragged, and whether Option made it a
    /// rectangle: a selection dragged past the top or bottom goes on from there as the view
    /// scrolls (`autoscroll`).
    dragged_at: Option<(f64, f64, bool)>,
    /// When the screen was made: a click's time is counted from it.
    born: Instant,
    /// When the left button last began a selection, counted from `born`: a Shift-click as soon
    /// after as a double-click's second press counts as that, not as taking the selection on.
    pressed_at: Option<Duration>,
    /// The buttons held, which a drag goes by.
    held: Vec<MouseButton>,
    /// A selection is being made with the left button.
    selecting: bool,
    size: Rc<Cell<Size>>,
    dark: Rc<Cell<bool>>,
    noted: Rc<RefCell<Noted>>,
    /// What a key or a click came to, for the program: written out by the caller.
    out: Vec<u8>,
    seq: u64,
    /// What the last frame said beside its rows.
    around: Option<Around>,
    /// Since when the program has held the screen still while it redraws (`held`).
    hold: Rc<Cell<Option<Instant>>>,
}

impl Screen {
    /// A screen of `size`, which writes what the program is answered with (a cursor position
    /// asked for, the colors) through `write`.
    pub fn new(size: Size, write: impl Fn(&[u8]) + 'static) -> libghostty_vt::error::Result<Screen> {
        Screen::around(Terminal::new(size.cols, size.rows)?, size, write)
    }

    /// The screen a snapshot holds (`snapshot`), as another of Codealong's servers took it: what
    /// it showed and its scrollback, the modes the program set (its keyboard protocol, bracketed
    /// pastes, the mouse) and a sequence it was halfway through, so the program carries on as if
    /// nothing happened. At `size`, which may have changed since.
    pub fn restore(snapshot: &[u8], size: Size, write: impl Fn(&[u8]) + 'static) -> libghostty_vt::error::Result<Screen> {
        let term = libghostty_vt::snapshot::Decoder::new_buf(snapshot)?.decode()?;
        Screen::around(term, size, write)
    }

    /// Everything the terminal holds, to hand to another server (`restore`). None when it can't
    /// be encoded.
    pub fn snapshot(&mut self) -> Option<Vec<u8>> {
        let mut out = Vec::new();
        self.term.encode_snapshot(&mut out).ok().map(|()| out)
    }

    fn around(mut term: Terminal<'static, 'static>, size: Size, write: impl Fn(&[u8]) + 'static) -> libghostty_vt::error::Result<Screen> {
        let size_cell = Rc::new(Cell::new(size));
        let dark = Rc::new(Cell::new(false));
        let noted = Rc::new(RefCell::new(Noted::default()));
        term.set_scrollback_max_lines(Some(SCROLLBACK_LINES))?.set_terminfo_name(TERM)?;
        // A snapshot taken partway through a sequence needs the part already read (`snapshot`).
        term.set_continuation_max_bytes(CONTINUATION_BYTES)?;
        term.resize(size.cols, size.rows, u32::from(size.cell_width), u32::from(size.cell_height))?;
        let (sizes, scheme) = (size_cell.clone(), dark.clone());
        let (titles, bells, pwds) = (noted.clone(), noted.clone(), noted.clone());
        let hold = Rc::new(Cell::new(None));
        let holding = hold.clone();
        term.on_pty_write(move |_, data| write(data))?
            .on_size(move |_| {
                let s = sizes.get();
                Some(SizeReportSize { rows: s.rows, columns: s.cols, cell_width: u32::from(s.cell_width), cell_height: u32::from(s.cell_height) })
            })?
            .on_xtversion(|_| Some("Codealong"))?
            .on_color_scheme(move |_| Some(if scheme.get() { ColorScheme::Dark } else { ColorScheme::Light }))?
            .on_title_changed(move |_| titles.borrow_mut().title = true)?
            .on_bell(move |_| bells.borrow_mut().bell = true)?
            .on_pwd_changed(move |_| pwds.borrow_mut().pwd = true)?
            .on_render_hold(move |_, held| holding.set(held.then(Instant::now)))?;
        let mut press = PressEvent::new()?;
        press.set_repeat_interval(CLICK_INTERVAL)?;
        Ok(Screen {
            term,
            render: RenderState::new()?,
            rows: RowIterator::new()?,
            cells: CellIterator::new()?,
            key_encoder: key::Encoder::new()?,
            key_event: key::Event::new()?,
            mouse_encoder: mouse::Encoder::new()?,
            mouse_event: mouse::Event::new()?,
            gesture: Gesture::new()?,
            press,
            release: ReleaseEvent::new()?,
            drag: DragEvent::new()?,
            tick: AutoscrollTickEvent::new()?,
            dragged_at: None,
            born: Instant::now(),
            pressed_at: None,
            held: Vec::new(),
            selecting: false,
            size: size_cell,
            dark,
            noted,
            out: Vec::new(),
            seq: 0,
            around: None,
            hold,
        })
    }

    /// What the program printed.
    pub fn feed(&mut self, bytes: &[u8]) {
        self.term.vt_write(bytes);
    }

    /// What the program did since it was last asked, beside printing.
    pub fn take_noted(&self) -> Noted {
        std::mem::take(&mut *self.noted.borrow_mut())
    }

    pub fn title(&self) -> String {
        self.term.title().map(|t| String::from_utf8_lossy(t).into_owned()).unwrap_or_default()
    }

    pub fn pwd(&self) -> String {
        self.term.pwd().map(|t| String::from_utf8_lossy(t).into_owned()).unwrap_or_default()
    }

    pub fn size(&self) -> Size {
        self.size.get()
    }

    /// A new size, reflowing what's on screen. The caller tells the pty.
    pub fn resize(&mut self, size: Size) {
        self.size.set(size);
        let _ = self.term.resize(size.cols, size.rows, u32::from(size.cell_width), u32::from(size.cell_height));
    }

    /// A window's colors: the defaults the screen is drawn with, and what programs that ask are
    /// told.
    pub fn theme(&mut self, theme: &Theme) {
        self.dark.set(theme.dark);
        let _ = self.term.set_default_fg_color(Some(rgb_of(theme.fg)));
        let _ = self.term.set_default_bg_color(Some(rgb_of(theme.bg)));
        let _ = self.term.set_default_cursor_color(theme.cursor.map(rgb_of));
        if let Ok(Palette(mut colors)) = self.term.default_color_palette() {
            for (slot, c) in colors.iter_mut().zip(&theme.palette) {
                *slot = rgb_of(*c);
            }
            let _ = self.term.set_default_color_palette(Some(Palette(colors)));
        }
        // Everything is drawn again in the new colors.
        if let Ok(snap) = self.render.update(&self.term) {
            let _ = snap.set_dirty(Dirty::Full);
        }
    }

    /// Takes what input came to, for the program.
    pub fn take_out(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.out)
    }

    /// Input from a window. Typing or pasting brings the view back to the bottom and clears the
    /// selection, as Ghostty does.
    pub fn input(&mut self, input: &Input) {
        match input {
            Input::Key { code, text, unshifted, mods, action } => self.key(code, text.as_deref(), unshifted.as_deref(), *mods, *action),
            Input::Text { text } => {
                if !text.is_empty() {
                    self.out.extend_from_slice(text.as_bytes());
                    self.typed();
                }
            }
            Input::Mouse { action, button, x, y, mods } => self.mouse(*action, *button, *x, *y, *mods),
            Input::Wheel { rows, x, y, mods } => self.wheel(*rows, *x, *y, *mods),
            Input::Focus { on } => {
                if self.term.mode(Mode::FOCUS_EVENT).unwrap_or(false) {
                    let mut buf = [0u8; 8];
                    let event = if *on { libghostty_vt::focus::Event::Gained } else { libghostty_vt::focus::Event::Lost };
                    if let Ok(n) = event.encode(&mut buf) {
                        self.out.extend_from_slice(&buf[..n]);
                    }
                }
            }
            Input::Scroll { to } => {
                let rows = isize::try_from(self.size.get().rows).unwrap_or(1).max(1);
                self.term.scroll_viewport(match to {
                    ScrollTo::Top => ScrollViewport::Top,
                    ScrollTo::Bottom => ScrollViewport::Bottom,
                    ScrollTo::PageUp => ScrollViewport::Delta(-rows),
                    ScrollTo::PageDown => ScrollViewport::Delta(rows),
                    ScrollTo::PromptUp => ScrollViewport::DeltaPrompt(-1),
                    ScrollTo::PromptDown => ScrollViewport::DeltaPrompt(1),
                });
            }
            Input::Select { all } => {
                // A Shift-click after takes nothing on from a click made before.
                self.gesture.reset(&self.term);
                if *all {
                    if let Ok(Some(sel)) = self.term.select_all() {
                        let _ = self.term.set_selection(Some(&sel));
                    }
                } else {
                    let _ = self.term.set_selection(None);
                }
            }
        }
    }

    /// Pastes `text` as the program asked for pastes (bracketed, or a paste event). Text that
    /// could run commands as it's pasted (a line break where pastes aren't bracketed) is refused
    /// unless `unsafe_ok`: false then, and nothing is written.
    pub fn paste(&mut self, text: &str, unsafe_ok: bool) -> bool {
        let mime = [ClipboardMime::new(b"text/plain")];
        let options = paste::Options::new().with_allow_unsafe(unsafe_ok);
        let pasted = self.term.paste(options, &mime, |_, w| w.write_all(text.as_bytes()));
        match pasted {
            Err(libghostty_vt::Error::Rejected) => false,
            _ => {
                self.typed();
                true
            }
        }
    }

    /// The selection's text, as copying it gives it.
    pub fn selection_text(&self) -> Option<String> {
        let mut f = Formatter::new_selection(&self.term).ok().flatten()?;
        f.set_format(Format::Plain).ok()?.set_unwrap(true).ok()?.set_trim(true).ok()?;
        let text = f.format_alloc(None).ok()?;
        Some(String::from_utf8_lossy(&text).into_owned()).filter(|t| !t.is_empty())
    }

    /// The screen as a terminal draws it from escape sequences, for another terminal to show it
    /// from where it is (terminals.rs `attach`): that terminal cleared, the scrollback and the
    /// screen with their colors, the modes the program set (its keyboard protocol, the mouse,
    /// bracketed pastes, the other screen, the Kitty keyboard protocol), the pen it's drawing with,
    /// and the cursor where it is. What the program prints next goes on from there.
    pub fn vt(&self) -> Vec<u8> {
        let mut out = b"\x1b[!p\x1b[H\x1b[2J\x1b[3J".to_vec();
        // Untrimmed: VT output then keeps the blank rows at the screen's end, and fills the rows
        // the screen does.
        let extras = Extras::MODES
            | Extras::KEYBOARD
            | Extras::KITTY_KEYBOARD
            | Extras::SCROLLING_REGION
            | Extras::TABSTOPS
            | Extras::PWD
            | Extras::CHARSETS
            | Extras::HYPERLINK
            | Extras::STYLE
            | Extras::CURSOR;
        let bytes = Formatter::new(&self.term).and_then(|mut f| {
            f.set_format(Format::Vt)?.set_trim(false)?.set_extras(extras)?;
            f.format_alloc(None)
        });
        if let Ok(bytes) = bytes {
            out.extend_from_slice(&bytes);
        }
        out
    }

    /// Everything the terminal holds as text, scrollback included: what a command printed.
    pub fn text(&self) -> String {
        let bytes = Formatter::new(&self.term).and_then(|mut f| {
            f.set_format(Format::Plain)?.set_unwrap(true)?.set_trim(true)?;
            f.format_alloc(None)
        });
        bytes.map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default()
    }

    /// Something was typed: the view goes back to the bottom and the selection goes, as in
    /// Ghostty, and so does where it began: a Shift-click after starts afresh.
    pub fn typed(&mut self) {
        self.term.scroll_viewport(ScrollViewport::Bottom);
        if self.term.selection().ok().flatten().is_some() {
            let _ = self.term.set_selection(None);
        }
        self.gesture.reset(&self.term);
    }

    fn key(&mut self, code: &str, text: Option<&str>, layout: Option<&str>, mods: Mods4, action: KeyAction) {
        let (physical, us) = keys::of_code(code);
        // The layout's character where the window gave one, a US keyboard's where it didn't, and
        // the key that character is on a US keyboard, which the encoder goes by: on AZERTY the
        // key in Q's place gives a, so Ctrl and it is Ctrl+A, as in Ghostty.
        let layout = layout.and_then(|l| {
            let mut chars = l.chars();
            chars.next().filter(|_| chars.next().is_none())
        });
        let unshifted = layout.unwrap_or(us);
        // The keypad's keys stay themselves, which application keypad mode and the Kitty protocol
        // tell apart from the digits and symbols they type.
        let key = if code.starts_with("Numpad") { physical } else { layout.and_then(keys::of_char).unwrap_or(physical) };
        let mods = mods.mods();
        let text = text.filter(|t| !t.is_empty() && action != KeyAction::Release);
        // The modifiers the text already took in: Shift for a capital, and on macOS Option for
        // the character it types (Ghostty's `macos-option-as-alt` off, its default).
        let mut consumed = Mods::empty();
        if text.is_some() {
            if mods.contains(Mods::SHIFT) {
                consumed |= Mods::SHIFT;
            }
            if mods.contains(Mods::ALT) && crate::os::current() == crate::os::Os::MacOs {
                consumed |= Mods::ALT;
            }
        }
        self.key_event
            .set_action(match action {
                KeyAction::Press => key::Action::Press,
                KeyAction::Repeat => key::Action::Repeat,
                KeyAction::Release => key::Action::Release,
            })
            .set_key(key)
            .set_mods(mods)
            .set_consumed_mods(consumed)
            .set_unshifted_codepoint(unshifted)
            .set_utf8(text);
        let before = self.out.len();
        let _ = self.key_encoder.set_options_from_terminal(&self.term).encode_to_vec(&self.key_event, &mut self.out);
        // A key libghostty doesn't know that still typed something.
        if self.out.len() == before && key == key::Key::Unidentified {
            if let Some(t) = text {
                self.out.extend_from_slice(t.as_bytes());
            }
        }
        if self.out.len() > before {
            self.typed();
        }
    }

    fn point(&self, x: f64, y: f64) -> Point {
        Point::Viewport(self.cell(x, y))
    }

    /// The cell of the view at a pointer's position, the nearest one when it's outside.
    fn cell(&self, x: f64, y: f64) -> PointCoordinate {
        let s = self.size.get();
        let col = (x / f64::from(s.cell_width.max(1))).floor().clamp(0.0, f64::from(s.cols.saturating_sub(1)));
        let row = (y / f64::from(s.cell_height.max(1))).floor().clamp(0.0, f64::from(s.rows.saturating_sub(1)));
        PointCoordinate { x: col as u16, y: row as u32 }
    }

    /// The screen as a selection's drag measures it: past its height, the drag scrolls.
    fn geometry(&self) -> Geometry {
        let s = self.size.get();
        Geometry { columns: s.cols.into(), cell_width: u32::from(s.cell_width), padding_left: 0, screen_height: u32::from(s.rows) * u32::from(s.cell_height) }
    }

    /// Whether a selection is being dragged past the top or bottom, which scrolls the view on
    /// (`autoscroll`) for as long as the button's held there, as in Ghostty.
    pub fn autoscrolling(&self) -> bool {
        self.selecting && self.dragged_at.is_some() && matches!(self.gesture.autoscroll(&self.term), Ok(Autoscroll::Up | Autoscroll::Down))
    }

    /// Scrolls the view toward where a selection is being dragged past the top or bottom, and
    /// takes the selection on to the row that comes into view there: a row, and a row more for
    /// each two rows' height the pointer is beyond the edge. libghostty has no timer of its own:
    /// the terminal's thread calls this every `AUTOSCROLL_EVERY` while `autoscrolling`. Once the
    /// view is as far as it goes, it stops until the pointer moves again, so a button let go
    /// where no window heard it (a window closed mid-drag) doesn't tick on for good.
    pub fn autoscroll(&mut self) {
        let Some((x, y, rectangle)) = self.dragged_at else { return };
        let offset = |term: &Terminal| term.scrollbar().map(|s| s.offset).ok();
        let was = offset(&self.term);
        let s = self.size.get();
        let height = f64::from(s.rows) * f64::from(s.cell_height);
        let beyond = if y < 0.0 { -y } else { (y - height).max(0.0) };
        let rows = (1 + (beyond / (2.0 * f64::from(s.cell_height.max(1)))) as u32).min(AUTOSCROLL_MOST);
        let (at, geometry) = (self.cell(x, y), self.geometry());
        for _ in 0..rows {
            let Ok(tick) = self.tick.set_rectangle(rectangle).and_then(|t| t.set_position(x, y)) else { return };
            match tick.apply(&mut self.gesture, &self.term, at, geometry) {
                Ok(Some(sel)) => {
                    let _ = self.term.set_selection(Some(&sel));
                }
                // Nothing to scroll toward any more, or the row the drag began on has gone from
                // the screen (a program cleared it): the selection stays as it was.
                _ => {
                    self.dragged_at = None;
                    return;
                }
            }
        }
        if offset(&self.term) == was {
            self.dragged_at = None;
        }
    }

    /// Whether a Shift-click now takes the selection on rather than starting one: there's a click
    /// it began from, still on the screen shown (not one from before a program took the whole
    /// screen, nor before something was typed), and this isn't as soon after it as a double-click.
    fn extends(&self, since: Duration) -> bool {
        let soon = self.pressed_at.is_some_and(|at| since.saturating_sub(at) <= CLICK_INTERVAL);
        !soon && matches!(self.gesture.anchor(&self.term), Ok(Some(_)))
    }

    /// Takes the selection being made on to the pointer at `x`, `y`. Past the top or bottom (the
    /// window keeps sending moves there while the button's held), it scrolls on from there
    /// (`autoscroll`).
    fn drag_to(&mut self, x: f64, y: f64, rectangle: bool) {
        let geometry = self.geometry();
        self.dragged_at = Some((x, y, rectangle));
        if let Ok(grid) = self.term.grid_ref(self.point(x, y)) {
            if let Ok(drag) = self.drag.set_rectangle(rectangle).and_then(|d| d.set_position(x, y)) {
                if let Ok(sel) = drag.apply(&mut self.gesture, &self.term, grid, geometry) {
                    let _ = self.term.set_selection(sel.as_ref());
                }
            }
        }
    }

    fn mouse(&mut self, action: MouseAction, button: Option<MouseButton>, x: f64, y: f64, mods4: Mods4) {
        let mods = mods4.mods();
        let tracking = self.term.is_mouse_tracking().unwrap_or(false);
        // A program that asks for the mouse gets it; Shift selects anyway, as in Ghostty.
        let selects = !tracking || mods.contains(Mods::SHIFT);
        match action {
            MouseAction::Down => {
                if let Some(b) = button {
                    if !self.held.contains(&b) {
                        self.held.push(b);
                    }
                }
                if button == Some(MouseButton::Left) && selects {
                    self.selecting = true;
                    self.dragged_at = None;
                    let since = self.born.elapsed();
                    // Shift and a click takes the selection from where it began (a click, a
                    // drag, a double-click's word) to here, as in Ghostty, Terminal and iTerm2:
                    // a long one is a click at one end, a scroll, and a Shift-click at the other.
                    // Held and dragged, it goes on as a drag does.
                    if mods.contains(Mods::SHIFT) && self.extends(since) {
                        self.drag_to(x, y, mods.contains(Mods::ALT));
                        return;
                    }
                    self.pressed_at = Some(since);
                    let s = self.size.get();
                    let point = self.point(x, y);
                    if let Ok(grid) = self.term.grid_ref(point) {
                        if let Ok(press) = self.press.set_repeat_distance(f64::from(s.cell_width)).and_then(|p| p.set_time(since)).and_then(|p| p.set_position(x, y)) {
                            if let Ok(sel) = press.apply(&mut self.gesture, &self.term, grid) {
                                let _ = self.term.set_selection(sel.as_ref());
                            }
                        }
                    }
                    return;
                }
            }
            MouseAction::Up => {
                self.held.retain(|b| Some(*b) != button);
                if button == Some(MouseButton::Left) && self.selecting {
                    self.selecting = false;
                    self.dragged_at = None;
                    let grid = self.term.grid_ref(self.point(x, y)).ok();
                    let _ = self.release.apply(&mut self.gesture, &self.term, grid);
                    return;
                }
            }
            MouseAction::Move => {
                if self.selecting {
                    // Option drags a rectangle, as in Ghostty on macOS.
                    self.drag_to(x, y, mods.contains(Mods::ALT));
                    return;
                }
            }
        }
        if !tracking {
            return;
        }
        let _ = self
            .mouse_encoder
            .set_options_from_terminal(&self.term)
            .set_any_button_pressed(!self.held.is_empty())
            .set_track_last_cell(true)
            .set_size(encoder_size(self.size.get()));
        let button = match action {
            MouseAction::Move => self.held.first().copied(),
            _ => button,
        };
        self.mouse_event
            .set_mods(mods)
            .set_position(mouse::Position { x: x as f32, y: y as f32 })
            .set_action(match action {
                MouseAction::Down => mouse::Action::Press,
                MouseAction::Up => mouse::Action::Release,
                MouseAction::Move => mouse::Action::Motion,
            })
            .set_button(button.map(|b| match b {
                MouseButton::Left => mouse::Button::Left,
                MouseButton::Middle => mouse::Button::Middle,
                MouseButton::Right => mouse::Button::Right,
            }));
        let _ = self.mouse_encoder.encode_to_vec(&self.mouse_event, &mut self.out);
    }

    fn wheel(&mut self, rows: f64, x: f64, y: f64, mods4: Mods4) {
        let steps = rows.round() as isize;
        if steps == 0 {
            return;
        }
        if self.term.is_mouse_tracking().unwrap_or(false) && !mods4.shift {
            for _ in 0..steps.unsigned_abs().min(10) {
                for action in [MouseAction::Down, MouseAction::Up] {
                    self.held.clear();
                    let _ = self.mouse_encoder.set_options_from_terminal(&self.term).set_size(encoder_size(self.size.get()));
                    self.mouse_event
                        .set_mods(mods4.mods())
                        .set_position(mouse::Position { x: x as f32, y: y as f32 })
                        .set_action(if action == MouseAction::Down { mouse::Action::Press } else { mouse::Action::Release })
                        .set_button(Some(if steps < 0 { mouse::Button::Four } else { mouse::Button::Five }));
                    let _ = self.mouse_encoder.encode_to_vec(&self.mouse_event, &mut self.out);
                }
            }
            return;
        }
        // A full-screen program that didn't ask for the mouse gets arrow keys, as Ghostty sends
        // them on the alternate screen (alternate scroll, mode 1007).
        let alternate = self.term.mode(Mode::ALT_SCREEN_SAVE).unwrap_or(false) || self.term.mode(Mode::ALT_SCREEN).unwrap_or(false);
        if alternate && self.term.mode(Mode::ALT_SCROLL).unwrap_or(false) {
            let code = if steps < 0 { "ArrowUp" } else { "ArrowDown" };
            for _ in 0..steps.unsigned_abs().min(10) {
                self.key(code, None, None, Mods4::default(), KeyAction::Press);
            }
            return;
        }
        self.term.scroll_viewport(ScrollViewport::Delta(steps));
    }

    /// Whether the program holds the screen still at `now`, halfway through a redraw it marked as
    /// one (synchronized output, which Claude Code and Codex wrap their draws in): no frame is
    /// made then, so a window never shows a redraw half done. Past `HOLD_MAX` the hold is let go.
    pub fn held(&mut self, now: Instant) -> bool {
        match self.hold.get() {
            Some(since) if now.saturating_duration_since(since) >= HOLD_MAX => {
                let _ = self.term.set_mode(Mode::SYNC_OUTPUT, false);
                self.hold.set(None);
                false
            }
            since => since.is_some(),
        }
    }

    /// What to draw: every row with `full`, else the rows that changed since the last frame, and
    /// the cursor and the rest beside them (None when nothing did: a cursor that moved on its own,
    /// as ← at a prompt moves it, leaves the rows clean and still makes a frame). `id` is the
    /// terminal's.
    pub fn frame(&mut self, id: &str, full: bool) -> Option<Frame> {
        let Screen { term, render, rows, cells, .. } = self;
        let snap = render.update(term).ok()?;
        if full {
            snap.set_dirty(Dirty::Full).ok()?;
        }
        let dirty = snap.dirty().ok()?;
        let colors = snap.colors().ok()?;
        let cursor = snap.cursor().ok().and_then(|c| {
            let at = c.viewport?;
            c.visible.then_some(Caret {
                x: at.x,
                y: at.y,
                shape: match c.visual_style {
                    CursorVisualStyle::Bar => "bar",
                    CursorVisualStyle::Underline => "underline",
                    CursorVisualStyle::BlockHollow => "hollow",
                    _ => "block",
                },
                blink: c.blinking,
                color: colors.cursor.map(hex_of),
                password: c.password_input,
            })
        });
        let scroll = term.scrollbar().map(|s| Scroll { total: s.total, offset: s.offset, len: s.len }).unwrap_or_default();
        let around = Around { cursor, scroll, fg: hex_of(colors.foreground), bg: hex_of(colors.background), mouse: term.is_mouse_tracking().unwrap_or(false), cols: snap.cols().unwrap_or(0), rows: snap.rows().unwrap_or(0) };
        if dirty == Dirty::Clean && self.around == Some(around) {
            return None;
        }
        let mut looks = Looks::default();
        let mut lines = Vec::new();
        let mut text = String::new();
        let mut row_it = rows.update(&snap).ok()?;
        while let Some((y, row)) = row_it.next_dirty() {
            let Ok(mut cell_it) = cells.update(row) else { continue };
            let mut runs: Vec<Run> = Vec::new();
            let mut x: u16 = 0;
            while let Some(cell) = cell_it.next() {
                let wide = cell.raw_cell().and_then(|c| c.wide()).unwrap_or(CellWide::Narrow);
                let content = match wide {
                    CellWide::SpacerTail | CellWide::SpacerHead => String::new(),
                    _ if cell.graphemes_len().unwrap_or(0) == 0 => " ".to_string(),
                    _ => {
                        text.clear();
                        let _ = cell.graphemes_utf8(&mut text);
                        text.clone()
                    }
                };
                let look = look_of(cell, &colors);
                let blank = (content == " " || (wide == CellWide::SpacerHead)) && look == Look::default();
                if !blank {
                    let index = looks.index(look);
                    match runs.last_mut() {
                        Some(run) if run.look == index && run.x + run.cells.len() as u16 == x => run.cells.push(content),
                        _ => runs.push(Run { x, look: index, cells: vec![content] }),
                    }
                }
                x = x.saturating_add(1);
            }
            lines.push(Line { y, runs });
        }
        let _ = snap.clean();
        self.around = Some(around);
        self.seq += 1;
        Some(Frame {
            id: id.to_string(),
            seq: self.seq,
            cols: around.cols,
            rows: around.rows,
            full: dirty == Dirty::Full,
            lines,
            looks: looks.list,
            cursor: around.cursor,
            fg: around.fg,
            bg: around.bg,
            scroll: around.scroll,
            mouse: around.mouse,
        })
    }
}

/// What a frame says beside its rows, which changes without them (a cursor moved, the program
/// taking the mouse): a frame goes out when it does.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Around {
    cursor: Option<Caret>,
    scroll: Scroll,
    fg: u32,
    bg: u32,
    mouse: bool,
    cols: u16,
    rows: u16,
}

/// A frame's looks, each once.
#[derive(Default)]
struct Looks {
    list: Vec<Look>,
}

impl Looks {
    fn index(&mut self, look: Look) -> u16 {
        match self.list.iter().position(|l| *l == look) {
            Some(i) => i as u16,
            None => {
                self.list.push(look);
                (self.list.len() - 1) as u16
            }
        }
    }
}

/// The screen's size in pixels, as the mouse encoder places a click in a cell by it. The encoder
/// refuses a cell with no width or height and keeps the size it had, but a terminal's size never
/// has one (terminals.rs `sane`), so the refusal is left unsaid where it's set.
fn encoder_size(s: Size) -> mouse::EncoderSize {
    mouse::EncoderSize {
        screen_width: u32::from(s.cols) * u32::from(s.cell_width),
        screen_height: u32::from(s.rows) * u32::from(s.cell_height),
        cell_width: u32::from(s.cell_width),
        cell_height: u32::from(s.cell_height),
        padding_top: 0,
        padding_bottom: 0,
        padding_left: 0,
        padding_right: 0,
    }
}

/// How a cell is drawn, its colors resolved: inverse swaps them, invisible draws the text in the
/// background's.
fn look_of(cell: &libghostty_vt::render::CellIteration<'_, '_>, colors: &libghostty_vt::render::Colors) -> Look {
    let mut fg = cell.fg_color().ok().flatten().map(hex_of);
    let mut bg = cell.bg_color().ok().flatten().map(hex_of);
    let mut look = Look::default();
    if cell.has_styling().unwrap_or(false) {
        if let Ok(style) = cell.style() {
            if style.inverse {
                let (f, b) = (fg.unwrap_or(hex_of(colors.foreground)), bg.unwrap_or(hex_of(colors.background)));
                fg = Some(b);
                bg = Some(f);
            }
            if style.invisible {
                fg = Some(bg.unwrap_or(hex_of(colors.background)));
            }
            look.ul = match style.underline_color {
                StyleColor::Rgb(c) => Some(hex_of(c)),
                StyleColor::Palette(i) => colors.palette.get(usize::from(i.0)).copied().map(hex_of),
                StyleColor::None => None,
            };
            let mut flags = 0;
            for (on, flag) in [(style.bold, BOLD), (style.italic, ITALIC), (style.faint, FAINT), (style.blink, BLINK), (style.strikethrough, STRIKE), (style.overline, OVERLINE)] {
                if on {
                    flags |= flag;
                }
            }
            let underline: u16 = match style.underline {
                Underline::None => 0,
                Underline::Single => 1,
                Underline::Double => 2,
                Underline::Curly => 3,
                Underline::Dotted => 4,
                Underline::Dashed => 5,
                // A kind newer than this build knows: drawn as a plain underline.
                _ => 1,
            };
            look.flags = flags | (underline << UNDERLINE_SHIFT);
        }
    }
    if cell.is_selected().unwrap_or(false) {
        look.flags |= SELECTED;
    }
    if cell.raw_cell().and_then(|c| c.has_hyperlink()).unwrap_or(false) {
        look.flags |= LINK;
    }
    look.fg = fg;
    look.bg = bg;
    look
}

fn hex_of(c: RgbColor) -> u32 {
    (u32::from(c.r) << 16) | (u32::from(c.g) << 8) | u32::from(c.b)
}

fn rgb_of(hex: u32) -> RgbColor {
    RgbColor { r: (hex >> 16) as u8, g: (hex >> 8) as u8, b: hex as u8 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    const SIZE: Size = Size { cols: 20, rows: 4, cell_width: 8, cell_height: 16 };

    fn screen() -> (Screen, Arc<Mutex<Vec<u8>>>) {
        let written = Arc::new(Mutex::new(Vec::new()));
        let w = written.clone();
        (Screen::new(SIZE, move |b| w.lock().unwrap().extend_from_slice(b)).unwrap(), written)
    }

    fn row_text(frame: &Frame, y: u16) -> String {
        let Some(line) = frame.lines.iter().find(|l| l.y == y) else { return String::new() };
        let mut s = String::new();
        for run in &line.runs {
            while s.chars().count() < usize::from(run.x) {
                s.push(' ');
            }
            for c in &run.cells {
                s.push_str(if c.is_empty() { "" } else { c });
            }
        }
        s.trim_end().to_string()
    }

    #[test]
    fn a_frame_has_the_rows_printed_and_then_only_the_ones_that_change() {
        let (mut s, _) = screen();
        s.feed(b"hello\r\nworld");
        let first = s.frame("t", true).expect("a frame");
        assert!(first.full);
        assert_eq!(first.lines.len(), 4, "every row");
        assert_eq!(row_text(&first, 0), "hello");
        assert_eq!(row_text(&first, 1), "world");
        assert_eq!(first.cursor.map(|c| (c.x, c.y)), Some((5, 1)));
        assert!(s.frame("t", false).is_none(), "nothing changed");
        s.feed(b"!");
        let next = s.frame("t", false).expect("a change");
        assert!(!next.full);
        assert_eq!(next.lines.iter().map(|l| l.y).collect::<Vec<_>>(), vec![1], "only the row typed on");
        assert_eq!(row_text(&next, 1), "world!");
        assert_eq!(next.seq, first.seq + 1);
    }

    /// ← at a prompt moves the cursor and changes no row: the window still hears where it went.
    #[test]
    fn a_cursor_that_moves_alone_makes_a_frame_without_rows() {
        let (mut s, _) = screen();
        s.feed(b"abc");
        s.frame("t", true).unwrap();
        s.feed(b"\x1b[D");
        let moved = s.frame("t", false).expect("a frame for the cursor");
        assert!(moved.lines.is_empty(), "no row changed");
        assert_eq!(moved.cursor.map(|c| (c.x, c.y)), Some((2, 0)));
        assert!(s.frame("t", false).is_none(), "and then nothing until something does");
        s.feed(b"\x1b[?25l");
        assert_eq!(s.frame("t", false).expect("hidden").cursor, None);
    }

    #[test]
    fn colors_come_resolved_and_blank_cells_are_left_out() {
        let (mut s, _) = screen();
        s.theme(&Theme { fg: 0x111111, bg: 0xeeeeee, cursor: None, palette: vec![0x000000, 0xcc0000], dark: false });
        // Red (palette 1) and bold, then a true color background, then reverse video.
        s.feed(b"\x1b[1;31mred\x1b[0m  \x1b[48;2;1;2;3mbg\x1b[0m \x1b[7minv\x1b[0m");
        let f = s.frame("t", true).unwrap();
        assert_eq!((f.fg, f.bg), (0x111111, 0xeeeeee), "the theme's defaults");
        let runs = &f.lines.iter().find(|l| l.y == 0).unwrap().runs;
        let red = &runs[0];
        assert_eq!(red.cells.join(""), "red");
        assert_eq!(f.looks[usize::from(red.look)], Look { fg: Some(0xcc0000), bg: None, ul: None, flags: BOLD });
        let bg = runs.iter().find(|r| r.cells.join("") == "bg").expect("the background run");
        assert_eq!(bg.x, 5, "the blank cells between are left out");
        assert_eq!(f.looks[usize::from(bg.look)].bg, Some(0x010203));
        let inv = runs.iter().find(|r| r.cells.join("") == "inv").expect("the reverse run");
        assert_eq!(f.looks[usize::from(inv.look)], Look { fg: Some(0xeeeeee), bg: Some(0x111111), ul: None, flags: 0 });
    }

    /// Claude Code and Codex wrap each redraw in synchronized output: while one is under way no
    /// frame is made, and a program that never lets go is drawn after `HOLD_MAX` anyway.
    #[test]
    fn a_redraw_held_still_is_drawn_once_its_done() {
        let (mut s, _) = screen();
        s.feed(b"old");
        s.frame("t", true).unwrap();
        let now = Instant::now();
        assert!(!s.held(now));
        s.feed(b"\x1b[?2026h\r\x1b[2Knew, half");
        assert!(s.held(now), "a redraw under way");
        s.feed(b" drawn\x1b[?2026l");
        assert!(!s.held(now));
        assert_eq!(row_text(&s.frame("t", false).unwrap(), 0), "new, half drawn", "drawn whole");
        s.feed(b"\x1b[?2026h\r\x1b[2Kstuck");
        assert!(s.held(now + Duration::from_millis(500)));
        assert!(!s.held(now + HOLD_MAX + Duration::from_millis(10)), "let go after the limit");
        assert!(!s.held(Instant::now()), "and stays let go");
        assert_eq!(row_text(&s.frame("t", false).unwrap(), 0), "stuck");
    }

    /// Claude Code turns the Kitty keyboard protocol on only once the terminal answers its
    /// question about it (it asks every terminal it doesn't know), and Shift+Return then reaches
    /// it as a key of its own, a new line rather than sending the message.
    #[test]
    fn claude_codes_keyboard_question_is_answered_and_shift_return_is_its_own_key() {
        let (mut s, written) = screen();
        s.feed(b"\x1b[?u\x1b[c");
        let answer = String::from_utf8_lossy(&written.lock().unwrap()).into_owned();
        assert!(answer.starts_with("\x1b[?0u"), "the keyboard question answered before the attributes: {answer:?}");
        let shift_return = Input::Key { code: "Enter".into(), text: None, unshifted: None, mods: Mods4 { shift: true, ..Default::default() }, action: KeyAction::Press };
        s.input(&shift_return);
        assert_eq!(s.take_out(), b"\x1b[27;2;13~", "never a plain Return, even before the program asks: xterm's form, as Ghostty sends it");
        s.feed(b"\x1b[>5u");
        s.input(&shift_return);
        assert_eq!(s.take_out(), b"\x1b[13;2u");
    }

    /// A screen handed to another server comes back as it was: what it shows, its scrollback, the
    /// modes the program set (the Kitty keyboard protocol, bracketed pastes), and a sequence cut
    /// partway, which the next bytes finish.
    #[test]
    fn a_snapshot_restores_the_screen_its_modes_and_a_sequence_cut_partway() {
        let (mut s, _) = screen();
        for n in 0..30 {
            s.feed(format!("line {n}\r\n").as_bytes());
        }
        s.feed(b"\x1b[?2004h\x1b[>5u\x1b[1;32mgreen\x1b[0m \x1b[3");
        let snap = s.snapshot().expect("a snapshot partway through a sequence");
        let written = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let w = written.clone();
        let mut back = Screen::restore(&snap, SIZE, move |b| w.lock().unwrap().extend_from_slice(b)).expect("restored");
        back.feed(b"1mred\x1b[0m");
        let f = back.frame("t", true).unwrap();
        let row = f.lines.iter().find(|l| l.runs.iter().any(|r| r.cells.join("") == "red")).expect("the cut sequence finished");
        let red = row.runs.iter().find(|r| r.cells.join("") == "red").unwrap();
        assert!(f.looks[usize::from(red.look)].fg.is_some(), "drawn in its color");
        assert!(row_text(&f, row.y).starts_with("green red"), "{:?}", row_text(&f, row.y));
        assert!(back.text().contains("line 0"), "the scrollback came too");
        assert!(back.paste("two\nlines", false), "a paste with a line break goes, bracketed");
        assert!(String::from_utf8_lossy(&written.lock().unwrap()).contains("\x1b[200~two"), "{:?}", written.lock().unwrap());
        let shift_return = Input::Key { code: "Enter".into(), text: None, unshifted: None, mods: Mods4 { shift: true, ..Default::default() }, action: KeyAction::Press };
        back.input(&shift_return);
        assert_eq!(back.take_out(), b"\x1b[13;2u", "the keyboard protocol came too");
    }

    /// What an attached terminal is first sent draws the same screen in another: its text, the
    /// Kitty keyboard protocol the program turned on, and the cursor where it was.
    #[test]
    fn the_screen_as_escape_sequences_draws_the_same_screen_with_its_modes() {
        let (mut s, _) = screen();
        s.feed(b"\x1b[>5u\x1b[?2004hone\r\ntwo \x1b[1;31mred");
        let (mut other, _) = screen();
        other.feed(&s.vt());
        let f = other.frame("t", true).unwrap();
        assert_eq!(row_text(&f, 0), "one");
        assert_eq!(row_text(&f, 1), "two red");
        assert_eq!(f.cursor.map(|c| (c.x, c.y)), Some((7, 1)), "the cursor where it was");
        other.feed(b"!");
        let g = other.frame("t", false).unwrap();
        let bang = g.lines.iter().flat_map(|l| &l.runs).find(|r| r.cells.join("").contains('!')).expect("drawn");
        assert!(g.looks[usize::from(bang.look)].fg.is_some(), "drawn with the pen the program had");
        let shift_return = Input::Key { code: "Enter".into(), text: None, unshifted: None, mods: Mods4 { shift: true, ..Default::default() }, action: KeyAction::Press };
        other.input(&shift_return);
        assert_eq!(other.take_out(), b"\x1b[13;2u", "the Kitty keyboard protocol on");
    }

    #[test]
    fn a_wide_character_takes_two_cells_the_second_left_empty() {
        let (mut s, _) = screen();
        s.feed("a字b".as_bytes());
        let f = s.frame("t", true).unwrap();
        let run = &f.lines[0].runs[0];
        assert_eq!(run.cells, vec!["a", "字", "", "b"]);
    }

    #[test]
    fn keys_are_encoded_for_the_modes_the_program_set() {
        let (mut s, _) = screen();
        let press = |code: &str, text: Option<&str>| Input::Key { code: code.into(), text: text.map(String::from), unshifted: None, mods: Mods4::default(), action: KeyAction::Press };
        s.input(&press("KeyA", Some("a")));
        s.input(&press("ArrowUp", None));
        s.input(&Input::Key { code: "KeyC".into(), text: None, unshifted: None, mods: Mods4 { ctrl: true, ..Default::default() }, action: KeyAction::Press });
        assert_eq!(s.take_out(), b"a\x1b[A\x03");
        // Application cursor keys, as an editor sets them.
        s.feed(b"\x1b[?1h");
        s.input(&press("ArrowUp", None));
        assert_eq!(s.take_out(), b"\x1bOA");
    }

    /// Ctrl and a letter go by the letter on your layout, not the key's place on a US keyboard:
    /// on AZERTY, Ctrl+A is the key a US keyboard has Q on.
    #[test]
    fn ctrl_and_a_letter_go_by_the_layouts_letter() {
        let (mut s, _) = screen();
        let ctrl = Mods4 { ctrl: true, ..Default::default() };
        s.input(&Input::Key { code: "KeyQ".into(), text: None, unshifted: Some("a".into()), mods: ctrl, action: KeyAction::Press });
        assert_eq!(s.take_out(), b"\x01", "Ctrl+A");
        s.input(&Input::Key { code: "KeyQ".into(), text: None, unshifted: None, mods: ctrl, action: KeyAction::Press });
        assert_eq!(s.take_out(), b"\x11", "Ctrl+Q where the window gave no letter");
        // The keypad's 1 stays the keypad's, which a program that asked for every key told apart
        // (the Kitty protocol's KP_1) from the 1 above the letters.
        s.feed(b"\x1b[>8u");
        s.input(&Input::Key { code: "Numpad1".into(), text: Some("1".into()), unshifted: Some("1".into()), mods: Mods4::default(), action: KeyAction::Press });
        let keypad = String::from_utf8_lossy(&s.take_out()).into_owned();
        assert!(keypad.contains("57400"), "{keypad:?}");
        s.feed(b"\x1b[<u");
        // A press and its release name the same key, for a program that asked for releases.
        s.feed(b"\x1b[>3u");
        s.input(&Input::Key { code: "KeyQ".into(), text: None, unshifted: Some("a".into()), mods: ctrl, action: KeyAction::Press });
        s.input(&Input::Key { code: "KeyQ".into(), text: None, unshifted: Some("a".into()), mods: ctrl, action: KeyAction::Release });
        let out = String::from_utf8_lossy(&s.take_out()).into_owned();
        assert!(out.contains("\x1b[97;5u") && out.contains("\x1b[97;5:3u"), "{out:?}");
    }

    #[test]
    fn a_paste_is_bracketed_when_the_program_asks_and_refused_when_it_could_run_commands() {
        let (mut s, written) = screen();
        assert!(!s.paste("ls\nrm -rf x\n", false), "a line break could run it");
        assert!(written.lock().unwrap().is_empty());
        assert!(s.paste("ls\n", true), "unless allowed");
        s.feed(b"\x1b[?2004h");
        assert!(s.paste("two\nlines", false));
        let out = String::from_utf8_lossy(&written.lock().unwrap()).into_owned();
        assert!(out.contains("\x1b[200~two"), "bracketed: {out:?}");
    }

    #[test]
    fn programs_asking_for_the_colors_are_told_the_theme() {
        let (mut s, written) = screen();
        s.theme(&Theme { fg: 0x102030, bg: 0xf0e0d0, cursor: None, palette: vec![], dark: true });
        s.feed(b"\x1b]11;?\x1b\\");
        s.feed(b"\x1b[?996n");
        let out = String::from_utf8_lossy(&written.lock().unwrap()).into_owned();
        assert!(out.contains("rgb:f0f0/e0e0/d0d0"), "the background: {out:?}");
        assert!(out.contains("\x1b[?997;1n"), "dark: {out:?}");
    }

    #[test]
    fn the_title_a_program_sets_is_noted() {
        let (mut s, _) = screen();
        s.feed(b"\x1b]2;building\x07");
        assert!(s.take_noted().title);
        assert_eq!(s.title(), "building");
        s.feed(b"\x07");
        assert!(s.take_noted().bell);
    }

    #[test]
    fn a_drag_selects_and_copying_gives_its_text() {
        let (mut s, _) = screen();
        s.feed(b"one two three");
        s.input(&Input::Mouse { action: MouseAction::Down, button: Some(MouseButton::Left), x: 33.0, y: 4.0, mods: Mods4::default() });
        s.input(&Input::Mouse { action: MouseAction::Move, button: Some(MouseButton::Left), x: 60.0, y: 4.0, mods: Mods4::default() });
        s.input(&Input::Mouse { action: MouseAction::Up, button: Some(MouseButton::Left), x: 60.0, y: 4.0, mods: Mods4::default() });
        assert_eq!(s.selection_text().as_deref(), Some("two"));
        let f = s.frame("t", true).unwrap();
        let selected: String = f.lines[0].runs.iter().filter(|r| f.looks[usize::from(r.look)].flags & SELECTED != 0).flat_map(|r| r.cells.clone()).collect();
        assert_eq!(selected, "two");
        assert!(s.take_out().is_empty(), "nothing went to the program");
    }

    /// A selection dragged past the top scrolls back through the scrollback as the terminal's
    /// thread ticks, a row and more the farther past it is, and stops once there's no more.
    #[test]
    fn a_drag_past_the_top_scrolls_and_selects_what_comes_into_view() {
        let (mut s, _) = screen();
        for i in 0..20 {
            s.feed(format!("line {i}\r\n").as_bytes());
        }
        // 4 rows of 16 px: the view shows lines 17 to 19 and the prompt's empty row.
        let mouse = |action, y: f64| Input::Mouse { action, button: Some(MouseButton::Left), x: 1.0, y, mods: Mods4::default() };
        s.input(&mouse(MouseAction::Down, 40.0));
        s.input(&mouse(MouseAction::Move, 20.0));
        assert!(!s.autoscrolling(), "inside the screen, nothing scrolls");
        s.input(&mouse(MouseAction::Move, -2.0));
        assert!(s.autoscrolling());
        s.autoscroll();
        assert_eq!(s.selection_text().as_deref(), Some("line 16\nline 17\nline 18"), "a row just past the top scrolls one");
        // Four rows' height past it scrolls three a tick.
        s.input(&mouse(MouseAction::Move, -64.0));
        s.autoscroll();
        assert!(s.selection_text().unwrap().starts_with("line 13\n"), "{:?}", s.selection_text());
        let mut ticks = 0;
        while s.autoscrolling() && ticks < 50 {
            s.autoscroll();
            ticks += 1;
        }
        assert!(!s.autoscrolling(), "it stops at the top of the scrollback");
        s.input(&mouse(MouseAction::Up, -64.0));
        let text = s.selection_text().unwrap();
        assert!(text.starts_with("line 0\n") && text.ends_with("line 18"), "{text:?}");
        // Let go while it still has somewhere to scroll (a window that lost the drag says so):
        // it stops there.
        s.input(&mouse(MouseAction::Down, 20.0));
        s.input(&mouse(MouseAction::Move, 80.0));
        assert!(s.autoscrolling());
        s.input(&mouse(MouseAction::Up, 80.0));
        assert!(!s.autoscrolling(), "the button let go stops it");
        assert!(s.take_out().is_empty(), "nothing went to the program");
    }

    /// As if `ms` went by: a click's time is counted from when the screen was made.
    fn wait(s: &mut Screen, ms: u64) {
        s.born -= Duration::from_millis(ms);
    }

    fn left(action: MouseAction, x: f64, y: f64, shift: bool) -> Input {
        Input::Mouse { action, button: Some(MouseButton::Left), x, y, mods: Mods4 { shift, ..Default::default() } }
    }

    fn click(s: &mut Screen, x: f64, y: f64, shift: bool) {
        s.input(&left(MouseAction::Down, x, y, shift));
        s.input(&left(MouseAction::Up, x, y, shift));
    }

    /// A click at one end, a scroll back through the scrollback and a Shift-click at the other
    /// selects everything between, as in Ghostty, Terminal and iTerm2; another Shift-click moves
    /// the far end again from the same place.
    #[test]
    fn a_shift_click_selects_from_a_click_across_a_scroll() {
        let (mut s, _) = screen();
        for i in 0..20 {
            s.feed(format!("line {i}\r\n").as_bytes());
        }
        // 4 rows of 16 px: the view shows lines 17 to 19 and the prompt's empty row. A click just
        // past the end of line 18 selects nothing yet.
        click(&mut s, 60.0, 20.0, false);
        assert_eq!(s.selection_text(), None);
        s.input(&Input::Wheel { rows: -10.0, x: 1.0, y: 1.0, mods: Mods4::default() });
        wait(&mut s, 1000);
        // The view now starts at line 7.
        click(&mut s, 1.0, 4.0, true);
        let text = s.selection_text().unwrap();
        assert!(text.starts_with("line 7\n") && text.ends_with("line 18"), "{text:?}");
        assert_eq!(text.lines().count(), 12);
        wait(&mut s, 1000);
        click(&mut s, 1.0, 36.0, true);
        let text = s.selection_text().unwrap();
        assert!(text.starts_with("line 9\n") && text.ends_with("line 18"), "{text:?}");
        assert!(s.take_out().is_empty(), "nothing went to the program");
    }

    /// A Shift-click after a double-click goes on by words, and one after a triple-click by lines.
    #[test]
    fn a_shift_click_goes_on_by_what_the_selection_began_with() {
        let (mut s, _) = screen();
        s.feed(b"one two three four\r\nfive six");
        click(&mut s, 33.0, 4.0, false);
        click(&mut s, 33.0, 4.0, false);
        assert_eq!(s.selection_text().as_deref(), Some("two"));
        wait(&mut s, 1000);
        click(&mut s, 115.0, 4.0, true);
        assert_eq!(s.selection_text().as_deref(), Some("two three four"));
        for _ in 0..3 {
            click(&mut s, 1.0, 4.0, false);
        }
        assert_eq!(s.selection_text().as_deref(), Some("one two three four"));
        wait(&mut s, 1000);
        click(&mut s, 1.0, 20.0, true);
        assert_eq!(s.selection_text().as_deref(), Some("one two three four\nfive six"));
    }

    /// A Shift-click as quick after a click as a double-click's second counts as that, and one
    /// after something was typed, or with nothing clicked before, starts afresh.
    #[test]
    fn a_shift_click_starts_afresh_when_its_a_double_click_or_after_typing() {
        let (mut s, _) = screen();
        s.feed(b"one two three");
        click(&mut s, 33.0, 4.0, true);
        assert_eq!(s.selection_text(), None, "nothing clicked before: a plain click");
        click(&mut s, 33.0, 4.0, true);
        assert_eq!(s.selection_text().as_deref(), Some("two"), "as quick as a double-click");
        s.input(&Input::Text { text: "x".into() });
        assert_eq!(s.selection_text(), None);
        wait(&mut s, 1000);
        click(&mut s, 100.0, 4.0, true);
        assert_eq!(s.selection_text(), None, "typing forgot where it began");
    }

    /// A program that asks for the mouse (htop, vim with `mouse=a`) is sent clicks and the wheel
    /// at the cell they happened in, in the SGR encoding it asked for, and nothing is selected.
    #[test]
    fn a_program_that_asks_for_the_mouse_is_sent_clicks_and_the_wheel_where_they_were() {
        let (mut s, _) = screen();
        s.feed(b"one two three\x1b[?1000h\x1b[?1006h");
        // 8×16 cells: x 33, y 20 is the fifth column of the second row.
        s.input(&Input::Mouse { action: MouseAction::Down, button: Some(MouseButton::Left), x: 33.0, y: 20.0, mods: Mods4::default() });
        s.input(&Input::Mouse { action: MouseAction::Up, button: Some(MouseButton::Left), x: 33.0, y: 20.0, mods: Mods4::default() });
        assert_eq!(String::from_utf8_lossy(&s.take_out()), "\x1b[<0;5;2M\x1b[<0;5;2m");
        assert_eq!(s.selection_text(), None);
        s.input(&Input::Wheel { rows: 1.0, x: 33.0, y: 20.0, mods: Mods4::default() });
        let wheel = String::from_utf8_lossy(&s.take_out()).into_owned();
        assert!(wheel.starts_with("\x1b[<65;5;2M"), "the wheel down, as button 5: {wheel:?}");
    }

    #[test]
    fn the_text_has_the_scrollback() {
        let (mut s, _) = screen();
        for i in 0..10 {
            s.feed(format!("line {i}\r\n").as_bytes());
        }
        let text = s.text();
        assert!(text.contains("line 0") && text.contains("line 9"), "{text:?}");
    }
}
