//! The machine as a whole: its load, memory and swap, what a process holds of memory (for
//! `codealong hygiene`), and how the OS schedules Codealong's threads and the app.

use serde::Serialize;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::*;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::*;

/// This machine's load, memory and swap.
#[derive(Serialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct System {
    /// The load averages over 1, 5 and 15 minutes, and how many CPUs share it.
    pub load: [f64; 3],
    pub cpus: u32,
    /// Memory in bytes, and how much of it is free as the OS counts it (macOS for memory pressure, Linux
    /// what it could give a new program without swapping).
    pub memory: u64,
    pub free_percent: Option<u32>,
    /// What macOS's compressor holds, in bytes.
    pub compressed: Option<u64>,
    pub swap_total: u64,
    pub swap_used: u64,
}
