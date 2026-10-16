//! The process table on Linux, read from `/proc` as `ps` reads it: a process's `stat` line (its
//! parent, terminal, foreground group and start), its `cwd`, `environ` and `fd` links, and the
//! kernel's tables of TCP sockets (`/proc/net/tcp`, `tcp6`). A process's `stat`, `cwd` and
//! `fd` are only yours to read for your own processes, as macOS answers.

use std::collections::HashMap;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use super::{Listed, Listener};

/// The fields of `/proc/<pid>/stat` read here.
#[derive(Debug, PartialEq)]
struct Stat {
    name: String,
    ppid: u32,
    /// The terminal's device number as the kernel encodes it, 0 for none.
    tty_nr: u32,
    tpgid: i32,
    /// When it started, in clock ticks since boot.
    start_ticks: u64,
}

/// `pid (name) state ppid pgrp session tty_nr tpgid … starttime …`. The name is in parentheses
/// and may hold spaces and parentheses of its own, so the fields after it are counted from the
/// last `)`.
fn parse_stat(line: &str) -> Option<Stat> {
    let open = line.find('(')?;
    let close = line.rfind(')')?;
    let name = line.get(open + 1..close)?.to_string();
    // The fields from the third (state) on.
    let f: Vec<&str> = line.get(close + 1..)?.split_whitespace().collect();
    Some(Stat { name, ppid: f.get(1)?.parse().ok()?, tty_nr: f.get(4)?.parse::<i64>().ok()? as u32, tpgid: f.get(5)?.parse().ok()?, start_ticks: f.get(19)?.parse().ok()? })
}

fn stat(pid: u32) -> Option<Stat> {
    parse_stat(&std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?)
}

/// When this machine booted, in epoch seconds (`/proc/stat`'s `btime`), and the clock's ticks a
/// second, which a process's start is counted in.
fn boot() -> Option<(i64, i64)> {
    static BOOT: OnceLock<Option<(i64, i64)>> = OnceLock::new();
    *BOOT.get_or_init(|| {
        let stat = std::fs::read_to_string("/proc/stat").ok()?;
        let btime = stat.lines().find_map(|l| l.strip_prefix("btime ")?.trim().parse().ok())?;
        // SAFETY: sysconf only reads a setting.
        let hz = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
        (hz > 0).then_some((btime, hz as i64))
    })
}

pub fn proc_start_ms(pid: u32) -> Option<i64> {
    let (btime, hz) = boot()?;
    Some(btime * 1000 + stat(pid)?.start_ticks as i64 * 1000 / hz)
}

pub fn parent_pid(pid: u32) -> Option<u32> {
    stat(pid).map(|s| s.ppid).filter(|&p| p > 0)
}

/// As macOS's process table has it, a process without a terminal on NODEV (all ones).
pub(super) fn listed(pid: u32) -> Option<Listed> {
    let s = stat(pid)?;
    Some(Listed { ppid: s.ppid, name: s.name, tdev: if s.tty_nr == 0 { u32::MAX } else { s.tty_nr }, tpgid: s.tpgid })
}

/// The device a terminal's number names: a pseudo-terminal's `/dev/pts/<n>` (majors 136 to
/// 143), or a console's `/dev/tty<n>`.
fn tty_path(tty_nr: u32) -> Option<PathBuf> {
    let major = (tty_nr >> 8) & 0xfff;
    let minor = (tty_nr & 0xff) | ((tty_nr >> 12) & 0xfff00);
    match major {
        136..=143 => Some(PathBuf::from(format!("/dev/pts/{}", (major - 136) * 256 + minor))),
        4 if minor < 64 => Some(PathBuf::from(format!("/dev/tty{minor}"))),
        _ => None,
    }
}

pub fn tty(pid: u32) -> Option<PathBuf> {
    let s = stat(pid)?;
    (s.tty_nr != 0).then(|| tty_path(s.tty_nr)).flatten()
}

/// A listening socket in `/proc/net/tcp` or `tcp6`: its inode, port and address.
fn parse_net_tcp(table: &str) -> Vec<(u64, u16, std::net::IpAddr)> {
    const LISTEN: &str = "0A";
    table
        .lines()
        .skip(1)
        .filter_map(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            if f.get(3)? != &LISTEN {
                return None;
            }
            let (addr, port) = f.get(1)?.split_once(':')?;
            let port = u16::from_str_radix(port, 16).ok()?;
            let inode = f.get(9)?.parse().ok()?;
            Some((inode, port, kernel_addr(addr)?))
        })
        .collect()
}

/// An address as the kernel writes it there: each 32-bit word in hex, in the machine's order.
/// One bound to every address is reached through loopback.
fn kernel_addr(hex: &str) -> Option<std::net::IpAddr> {
    let words: Vec<u32> = (0..hex.len() / 8).map(|i| u32::from_str_radix(hex.get(i * 8..i * 8 + 8)?, 16).ok()).collect::<Option<_>>()?;
    let bytes: Vec<u8> = words.iter().flat_map(|w| w.to_ne_bytes()).collect();
    match bytes.len() {
        4 => {
            let a = std::net::Ipv4Addr::new(bytes[0], bytes[1], bytes[2], bytes[3]);
            Some(if a.is_unspecified() { std::net::Ipv4Addr::LOCALHOST.into() } else { a.into() })
        }
        16 => {
            let a = std::net::Ipv6Addr::from(<[u8; 16]>::try_from(bytes).ok()?);
            Some(if a.is_unspecified() { std::net::Ipv6Addr::LOCALHOST.into() } else { a.into() })
        }
        _ => None,
    }
}

/// The inode of a descriptor that's a socket, from its link (`socket:[1234]`).
fn socket_inode(link: &Path) -> Option<u64> {
    link.to_str()?.strip_prefix("socket:[")?.strip_suffix(']')?.parse().ok()
}

pub fn listeners() -> Vec<Listener> {
    let mut sockets: HashMap<u64, (u16, std::net::IpAddr)> = HashMap::new();
    for table in ["/proc/net/tcp", "/proc/net/tcp6"] {
        for (inode, port, addr) in parse_net_tcp(&std::fs::read_to_string(table).unwrap_or_default()) {
            sockets.insert(inode, (port, addr));
        }
    }
    if sockets.is_empty() {
        return Vec::new();
    }
    // SAFETY: getuid can't fail.
    let me = unsafe { libc::getuid() };
    let Ok(procs) = std::fs::read_dir("/proc") else { return Vec::new() };
    let mut out = Vec::new();
    for entry in procs.flatten() {
        let Some(pid) = entry.file_name().to_str().and_then(|n| n.parse::<u32>().ok()) else { continue };
        if entry.metadata().ok().map(|m| m.uid()) != Some(me) {
            continue;
        }
        let Ok(fds) = std::fs::read_dir(entry.path().join("fd")) else { continue };
        let mut ports: Vec<(u16, std::net::IpAddr)> = Vec::new();
        for fd in fds.flatten() {
            let Some((port, addr)) = std::fs::read_link(fd.path()).ok().as_deref().and_then(socket_inode).and_then(|i| sockets.get(&i)) else { continue };
            match ports.iter_mut().find(|(p, _)| p == port) {
                // Listening on both families: IPv4 is reached the same way everywhere.
                Some(have) if have.1.is_ipv6() && addr.is_ipv4() => have.1 = *addr,
                Some(_) => {}
                None => ports.push((*port, *addr)),
            }
        }
        if ports.is_empty() {
            continue;
        }
        let name = stat(pid).map(|s| s.name).unwrap_or_default();
        let cwd = cwd_of(pid).unwrap_or_default();
        let started_ms = proc_start_ms(pid).unwrap_or(0);
        out.extend(ports.into_iter().map(|(port, addr)| Listener { pid, port, addr, name: name.clone(), cwd: cwd.clone(), started_ms }));
    }
    out
}

/// The files it has open: its descriptors that link to a path, not a socket, pipe or the like.
pub fn open_files(pid: u32) -> Option<Vec<PathBuf>> {
    let fds = std::fs::read_dir(format!("/proc/{pid}/fd")).ok()?;
    Some(fds.flatten().filter_map(|fd| std::fs::read_link(fd.path()).ok()).filter(|p| p.is_absolute()).collect())
}

pub fn cwd_of(pid: u32) -> Option<PathBuf> {
    std::fs::read_link(format!("/proc/{pid}/cwd")).ok().filter(|p| !p.as_os_str().is_empty())
}

pub fn environment(pid: u32) -> Option<HashMap<String, String>> {
    Some(environ(&std::fs::read(format!("/proc/{pid}/environ")).ok()?))
}

/// `/proc/<pid>/environ`: `NAME=value` strings, each ending in a NUL.
fn environ(bytes: &[u8]) -> HashMap<String, String> {
    bytes.split(|&b| b == 0).filter_map(|v| std::str::from_utf8(v).ok()?.split_once('=')).map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_stat_line_whatever_the_name_holds() {
        let line = "4102 (tmux: server) S 1 4102 4102 0 -1 4194624 2310 0 0 0 31 18 0 0 20 0 1 0 8817 9416704 891 18446744073709551615 1 1 0 0 0 0 0 4096 1 0 0 0 17 3 0 0 0 0 0";
        assert_eq!(parse_stat(line), Some(Stat { name: "tmux: server".into(), ppid: 1, tty_nr: 0, tpgid: -1, start_ticks: 8817 }));
        let shell = "4110 (we (ird) name) S 4102 4110 4110 34819 4290 4194304 917 0 0 0 1 0 0 0 20 0 1 0 8840 8650752 1280 18446744073709551615";
        let s = parse_stat(shell).unwrap();
        assert_eq!((s.name.as_str(), s.ppid, s.tty_nr, s.tpgid, s.start_ticks), ("we (ird) name", 4102, 34819, 4290, 8840));
        assert_eq!(parse_stat("4110 (sh"), None);
    }

    #[test]
    fn a_terminal_number_names_its_device() {
        // 34819 is major 136, minor 3.
        assert_eq!(tty_path(34819), Some(PathBuf::from("/dev/pts/3")));
        // Minors past 255 spill into the high bits, and majors past 136 count on.
        assert_eq!(tty_path((136 << 8) | (1 << 20)), Some(PathBuf::from("/dev/pts/256")));
        assert_eq!(tty_path(137 << 8), Some(PathBuf::from("/dev/pts/256")));
        assert_eq!(tty_path((4 << 8) | 2), Some(PathBuf::from("/dev/tty2")));
        assert_eq!(tty_path(5 << 8), None);
    }

    #[test]
    fn listening_sockets_read_from_the_kernels_tables() {
        let v4 = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n   0: 0100007F:1F90 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 31337 1 0000000000000000 100 0 0 10 0\n   1: 0100007F:A2C4 0100007F:1F90 01 00000000:00000000 00:00000000 00000000  1000        0 31338 1\n   2: 00000000:1538 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 31339 1\n";
        assert_eq!(parse_net_tcp(v4), vec![(31337, 8080, std::net::IpAddr::from([127, 0, 0, 1])), (31339, 5432, std::net::IpAddr::from([127, 0, 0, 1]))]);
        let v6 = "  sl  local_address                         remote_address                        st\n   0: 00000000000000000000000001000000:0BB8 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 41000 1\n";
        assert_eq!(parse_net_tcp(v6), vec![(41000, 3000, std::net::IpAddr::from(std::net::Ipv6Addr::LOCALHOST))]);
        assert_eq!(socket_inode(Path::new("socket:[31337]")), Some(31337));
        assert_eq!(socket_inode(Path::new("pipe:[31337]")), None);
    }

    #[test]
    fn reads_an_environment_of_nul_ended_strings() {
        let env = environ(b"HOME=/home/me\0EMPTY=\0EQ=a=b\0\0");
        assert_eq!(env.len(), 3);
        assert_eq!(env["HOME"], "/home/me");
        assert_eq!(env["EMPTY"], "");
        assert_eq!(env["EQ"], "a=b");
    }
}
