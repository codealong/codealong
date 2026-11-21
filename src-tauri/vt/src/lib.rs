//! Codealong's terminal emulator wrapper over `libghostty-vt`, kept in a workspace crate of its
//! own so edits to `codealong_lib` don't recompile this ~2,000-line wrapper or re-run its tests,
//! and `cargo test -p codealong-vt` iterates on terminal emulation in under a second.

// The engine's checkout: reading it has cargo rebuild this crate whenever another checkout built
// it into the shared build directory last (the engine's `CHECKOUT` says why).
const _CHECKOUT: Option<&str> = option_env!("CODEALONG_CHECKOUT");

/// A terminal's size: in cells, and the cell's size in pixels, which programs that draw pictures
/// ask for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Size {
    pub cols: u16,
    pub rows: u16,
    pub cell_width: u16,
    pub cell_height: u16,
}

pub mod keys;
pub mod queries;
pub mod screen;
