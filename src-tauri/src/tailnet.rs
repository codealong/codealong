//! Finding your devices on your tailnet (peers.rs). Tailscale already knows your machines, reaches
//! them through NATs, encrypts what goes between them and knows whose each one is, so Codealong
//! asks its command: `tailscale status --json` for this device's address and the other machines,
//! and `tailscale whois --json <ip>` for whose a connection is. Codealong listens on a TCP port
//! bound to this device's tailnet address alone, so nothing on the local network or the internet
//! can reach it.

use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use serde::Serialize;
use serde_json::Value;

const TIMEOUT: Duration = Duration::from_secs(6);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(4);
/// The systems, as Tailscale names them, a device running Codealong can be on.
const DESKTOPS: [&str; 3] = ["macOS", "linux", "windows"];

/// The `tailscale` command: Homebrew's, the standalone app's, or the one inside the app.
pub fn cli() -> Option<PathBuf> {
    ["/usr/local/bin/tailscale", "/opt/homebrew/bin/tailscale", "/Applications/Tailscale.app/Contents/MacOS/Tailscale"].into_iter().map(PathBuf::from).find(|p| p.exists())
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    /// Tailscale's id for the machine.
    pub id: String,
    /// The machine's own name for itself.
    pub name: String,
    /// Its MagicDNS name, without the trailing dot.
    pub dns: String,
    pub ips: Vec<IpAddr>,
    pub os: String,
    pub online: bool,
    /// Who it belongs to: the tailnet user's login.
    pub user: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// Tailscale is connected; when it isn't, the rest is whatever it last knew.
    pub running: bool,
    pub tailnet: String,
    pub me: Node,
    pub peers: Vec<Node>,
}

impl Status {
    /// This device's IPv4 address on the tailnet, which Codealong listens on and dials from.
    pub fn my_ipv4(&self) -> Option<IpAddr> {
        self.me.ips.iter().copied().find(IpAddr::is_ipv4)
    }

    /// The other devices on the tailnet that are yours, online now and on a system Codealong runs
    /// on, with the address to dial. Phones and tablets are left out: nothing there would answer.
    pub fn my_devices(&self) -> Vec<(&Node, IpAddr)> {
        self.peers
            .iter()
            .filter(|n| n.online && DESKTOPS.iter().any(|os| n.os.eq_ignore_ascii_case(os)) && n.user == self.me.user && !self.me.user.is_empty())
            .filter_map(|n| n.ips.iter().copied().find(IpAddr::is_ipv4).map(|ip| (n, ip)))
            .collect()
    }
}

pub fn status() -> Result<Status> {
    let cli = cli().ok_or_else(|| anyhow!("Tailscale isn't installed."))?;
    let out = crate::procs::run(&cli.to_string_lossy(), &["status", "--json"], None, TIMEOUT).context("tailscale status")?;
    parse_status(&out)
}

pub fn parse_status(json: &str) -> Result<Status> {
    let v: Value = serde_json::from_str(json).context("tailscale status didn't read as JSON")?;
    let users = v.get("User").and_then(Value::as_object);
    let login = |n: &Value| -> String {
        let id = match n.get("UserID") {
            Some(Value::Number(x)) => x.to_string(),
            Some(Value::String(s)) => s.clone(),
            _ => return String::new(),
        };
        users.and_then(|u| u.get(&id)).and_then(|u| u.get("LoginName")).and_then(Value::as_str).unwrap_or_default().to_string()
    };
    let node = |n: &Value| Node {
        id: str_of(n, "ID"),
        name: str_of(n, "HostName"),
        dns: str_of(n, "DNSName").trim_end_matches('.').to_string(),
        ips: n.get("TailscaleIPs").and_then(Value::as_array).map(|a| a.iter().filter_map(|x| x.as_str()?.parse().ok()).collect()).unwrap_or_default(),
        os: str_of(n, "OS"),
        online: n.get("Online").and_then(Value::as_bool).unwrap_or(false),
        user: login(n),
    };
    let Some(me) = v.get("Self").filter(|s| s.is_object()) else { bail!("Tailscale isn't signed in.") };
    let mut peers: Vec<Node> = v.get("Peer").and_then(Value::as_object).map(|m| m.values().map(node).collect()).unwrap_or_default();
    peers.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(Status {
        running: v.get("BackendState").and_then(Value::as_str) == Some("Running"),
        tailnet: v.get("CurrentTailnet").and_then(|t| t.get("Name")).and_then(Value::as_str).unwrap_or_default().to_string(),
        me: node(me),
        peers,
    })
}

fn str_of(v: &Value, k: &str) -> String {
    v.get(k).and_then(Value::as_str).unwrap_or_default().to_string()
}

/// Whose machine an address on the tailnet is.
#[derive(Debug, Clone, PartialEq)]
pub struct WhoIs {
    pub user: String,
    pub machine: String,
}

pub fn whois(ip: IpAddr) -> Result<WhoIs> {
    let cli = cli().ok_or_else(|| anyhow!("Tailscale isn't installed."))?;
    let out = crate::procs::run(&cli.to_string_lossy(), &["whois", "--json", &ip.to_string()], None, TIMEOUT).context("tailscale whois")?;
    parse_whois(&out)
}

pub fn parse_whois(json: &str) -> Result<WhoIs> {
    let v: Value = serde_json::from_str(json).context("tailscale whois didn't read as JSON")?;
    let user = v.pointer("/UserProfile/LoginName").and_then(Value::as_str).unwrap_or_default().to_string();
    if user.is_empty() {
        bail!("Tailscale doesn't know whose machine that is.");
    }
    let machine = v.pointer("/Node/ComputedName").and_then(Value::as_str).or_else(|| v.pointer("/Node/Name").and_then(Value::as_str)).unwrap_or_default().trim_end_matches('.').to_string();
    Ok(WhoIs { user, machine })
}

/// Listens on this device's tailnet address. Accepting doesn't block, so the loop that accepts can
/// notice when it should stop.
pub fn listen(ip: IpAddr, port: u16) -> Result<TcpListener> {
    let l = TcpListener::bind(SocketAddr::new(ip, port)).with_context(|| format!("Couldn't listen on {ip}:{port}"))?;
    l.set_nonblocking(true)?;
    Ok(l)
}

pub fn dial(ip: IpAddr, port: u16) -> Result<TcpStream> {
    let s = TcpStream::connect_timeout(&SocketAddr::new(ip, port), CONNECT_TIMEOUT)?;
    s.set_nodelay(true)?;
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    const STATUS: &str = r#"{
        "Version": "1.102.3", "BackendState": "Running",
        "CurrentTailnet": {"Name": "sam@example.com", "MagicDNSSuffix": "tail1234.ts.net"},
        "Self": {"ID": "nSelf", "HostName": "Sam's MacBook Pro", "DNSName": "sams-macbook-pro.tail1234.ts.net.", "OS": "macOS",
                 "Online": true, "UserID": 11, "TailscaleIPs": ["100.101.1.1", "fd7a:115c:a1e0::1"]},
        "Peer": {
            "nodekey:a": {"ID": "nMini", "HostName": "sams-mac-mini", "DNSName": "sams-mac-mini.tail1234.ts.net.", "OS": "macOS",
                          "Online": true, "UserID": 11, "TailscaleIPs": ["100.101.1.2", "fd7a:115c:a1e0::2"]},
            "nodekey:b": {"ID": "nPhone", "HostName": "iPhone", "DNSName": "iphone.tail1234.ts.net.", "OS": "iOS",
                          "Online": true, "UserID": 11, "TailscaleIPs": ["100.101.1.3"]},
            "nodekey:c": {"ID": "nAlex", "HostName": "alexs-mbp", "DNSName": "alexs-mbp.tail1234.ts.net.", "OS": "macOS",
                          "Online": true, "UserID": 22, "TailscaleIPs": ["100.101.1.4"]},
            "nodekey:d": {"ID": "nOld", "HostName": "old-imac", "DNSName": "old-imac.tail1234.ts.net.", "OS": "macOS",
                          "Online": false, "UserID": 11, "TailscaleIPs": ["100.101.1.5"]}
        },
        "User": {"11": {"LoginName": "sam@example.com", "DisplayName": "Sam"}, "22": {"LoginName": "alex@example.com"}}
    }"#;

    #[test]
    fn reads_status() {
        let s = parse_status(STATUS).unwrap();
        assert!(s.running);
        assert_eq!(s.tailnet, "sam@example.com");
        assert_eq!(s.me.user, "sam@example.com");
        assert_eq!(s.my_ipv4(), Some("100.101.1.1".parse().unwrap()));
        assert_eq!(s.me.dns, "sams-macbook-pro.tail1234.ts.net");
        assert_eq!(s.peers.len(), 4);
        // Only your own devices that are online: not the phone, not Alex's, not the one that's off.
        let mine: Vec<(&str, String)> = s.my_devices().into_iter().map(|(n, ip)| (n.name.as_str(), ip.to_string())).collect();
        assert_eq!(mine, [("sams-mac-mini", "100.101.1.2".to_string())]);
    }

    #[test]
    fn signed_out_is_an_error_and_stopped_isnt_running() {
        assert!(parse_status(r#"{"BackendState": "NeedsLogin", "Self": null}"#).is_err());
        let stopped = STATUS.replace(r#""BackendState": "Running""#, r#""BackendState": "Stopped""#);
        assert!(!parse_status(&stopped).unwrap().running);
    }

    /// This device's real tailnet: its status reads, `whois` knows this device, and Codealong can
    /// listen on its tailnet address. Read-only; run by hand: `cargo test tailnet_here -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn tailnet_here() {
        let s = status().expect("tailscale status");
        let ip = s.my_ipv4().expect("an IPv4 address on the tailnet");
        println!("me: {} ({}) on {}, {} peers, your devices online: {:?}", s.me.name, ip, s.tailnet, s.peers.len(), s.my_devices().iter().map(|(n, _)| &n.name).collect::<Vec<_>>());
        let who = whois(ip).expect("whois");
        assert_eq!(who.user, s.me.user);
        let l = listen(ip, 0).expect("listen on the tailnet address");
        println!("listening on {}", l.local_addr().unwrap());
    }

    #[test]
    fn reads_whois() {
        let w = parse_whois(r#"{"Node": {"ComputedName": "sams-mac-mini", "Name": "sams-mac-mini.tail1234.ts.net."},
                                "UserProfile": {"LoginName": "sam@example.com", "DisplayName": "Sam"}}"#)
        .unwrap();
        assert_eq!(w, WhoIs { user: "sam@example.com".into(), machine: "sams-mac-mini".into() });
        assert!(parse_whois(r#"{"Node": {}}"#).is_err());
    }
}
