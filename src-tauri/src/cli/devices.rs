//! `codealong devices`: this device, the ones it's linked with, the ones it could link with and
//! the ones asking to. All of it is asked of the server (`ask_app`), which holds the links.
//! `which_device` is how a command names a device (its name, its short id or the start of its
//! key); `start --device` and a schedule's `--prefer` name one the same way.

use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};

use crate::server::{self, Request};
use crate::util::now_ms;

use super::*;

// ---- your other devices ----

pub(super) fn ask_app(req: &Request) -> Result<Value> {
    match server::ask(req) {
        Some(Ok(v)) => Ok(v),
        Some(Err(e)) => Err(anyhow!("{e}")),
        None => bail!("Devices need Codealong's server, which couldn't be started here (`codealong server start` says why)."),
    }
}

/// A list in the devices view.
#[derive(Clone, Copy)]
pub(super) enum Listed {
    /// Linked Macs.
    Linked,
    Phones,
    /// Your devices on your tailnet that aren't linked.
    Found,
    /// Devices asking to link.
    Asks,
}

/// A device named on the command line, by name, short id or the start of its key, among the ones
/// in the view's `lists`. Its key and name.
pub(super) fn which_device(view: &server::DevicesSummary, named: &str, lists: &[Listed]) -> Result<(String, String)> {
    let named = named.trim();
    let lower = named.to_lowercase();
    let one = |key: &str, short: &str, name: &str| (key.to_string(), short.to_string(), name.to_string());
    let all: Vec<(String, String, String)> = lists
        .iter()
        .flat_map(|list| -> Vec<(String, String, String)> {
            match list {
                Listed::Linked => view.devices.iter().map(|d| one(&d.device.key, &d.device.short, &d.device.name)).collect(),
                Listed::Phones => view.phones.iter().map(|d| one(&d.key, &d.short, &d.name)).collect(),
                Listed::Found => view.found.iter().map(|d| one(&d.key, &d.short, &d.name)).collect(),
                Listed::Asks => view.asks.iter().map(|d| one(&d.key, &d.short, &d.name)).collect(),
            }
        })
        .collect();
    let hits: Vec<&(String, String, String)> = all.iter().filter(|(k, short, name)| name.to_lowercase() == lower || short == named || (named.len() >= 4 && k.starts_with(named))).collect();
    match hits.as_slice() {
        [one] => Ok((one.0.clone(), one.2.clone())),
        [] => bail!("No device here matches {named:?}; `codealong devices` lists them."),
        _ => bail!("More than one device matches {named:?}; give its short id."),
    }
}

pub(super) fn devices_cmd(c: &mut Client, cmd: Option<DevicesCmd>, json: bool) -> Result<()> {
    if c.app.is_none() {
        bail!("Devices need Codealong's server, which couldn't be started here (`codealong server start` says why).");
    }
    let view: server::DevicesSummary = serde_json::from_value(ask_app(&Request::Devices)?).context("reading the app's devices")?;
    let said = |text: String, v: Value| -> Result<()> {
        if json {
            print_json(&v)
        } else {
            println!("{text}");
            Ok(())
        }
    };
    match cmd {
        None => devices_list(&view, json),
        Some(DevicesCmd::Link { device }) => {
            let (key, name) = which_device(&view, &device, &[Listed::Found])?;
            let code = ask_app(&Request::DeviceLink { key })?;
            let code = code.as_str().unwrap_or_default().to_string();
            said(format!("Asked {name}. Say yes there when it shows {code}."), json!({ "device": name, "code": code }))
        }
        Some(DevicesCmd::Answer { device, no }) => {
            let (key, name) = which_device(&view, &device, &[Listed::Asks])?;
            ask_app(&Request::DeviceAnswer { key, accept: !no })?;
            said(if no { format!("Said no to {name}.") } else { format!("Linked with {name}.") }, json!({ "device": name, "linked": !no }))
        }
        Some(DevicesCmd::Unlink { device }) => {
            let (key, name) = which_device(&view, &device, &[Listed::Linked, Listed::Phones])?;
            ask_app(&Request::DeviceUnlink { key })?;
            said(format!("Unlinked {name}."), json!({ "device": name, "linked": false }))
        }
        Some(DevicesCmd::Code) => {
            let code = ask_app(&Request::DeviceCode)?;
            let code = code.as_str().unwrap_or_default().to_string();
            said(format!("{code}\nEnter it on the other device within ten minutes: codealong devices use-code <code>, or Settings › Devices."), json!({ "code": code }))
        }
        Some(DevicesCmd::UseCode { code }) => {
            let name = ask_app(&Request::DeviceUseCode { code })?;
            let name = name.as_str().unwrap_or_default().to_string();
            said(format!("Linked with {name}."), json!({ "device": name, "linked": true }))
        }
        Some(DevicesCmd::Ask { device, method, params }) => {
            let (key, _) = which_device(&view, &device, &[Listed::Linked])?;
            let params: Value = match params {
                Some(p) => serde_json::from_str(&p).context("the parameters aren't JSON")?,
                None => json!({}),
            };
            let got = ask_app(&Request::DeviceAsk { device: key.chars().take(10).collect(), request: method, params })?;
            print_json(&got)
        }
    }
}

fn devices_list(view: &server::DevicesSummary, json: bool) -> Result<()> {
    if json {
        // As a JSON value, whose keys come out in order, as the app's reply's did.
        return print_json(&serde_json::to_value(view)?);
    }
    let me = &view.me;
    if !view.enabled {
        let load = me.load.as_ref().map(|l| format!(" This device's load: {}.", crate::load::words(l))).unwrap_or_default();
        println!("Devices is off on this device: turn it on in Settings › Devices, or [devices] enabled = true in config.toml.{load}");
        return Ok(());
    }
    let tailscale = match me.tailscale.as_str() {
        "running" => format!("on {}{}", me.tailnet.as_deref().unwrap_or_default(), me.address.as_deref().map(|a| format!(", listening at {a}")).unwrap_or_default()),
        "missing" => "not installed".into(),
        "stopped" => "not connected".into(),
        _ => "off".into(),
    };
    let load = me.load.as_ref().map(|l| format!(" Load: {}.", crate::load::words(l))).unwrap_or_default();
    println!("This device: {} ({}). Tailscale: {tailscale}. iroh: {}. Remote control: {}.{load}", me.name, me.short, me.iroh, if me.remote_control { "on" } else { "off" });
    if let Some(e) = &me.error {
        println!("{e}");
    }
    let mut rows = Vec::new();
    for d in &view.devices {
        let via = d.device.via.and_then(|v| serde_json::to_value(v).ok()).and_then(|v| v.as_str().map(String::from)).unwrap_or_default();
        let state = if d.device.online { format!("connected, {via}") } else { d.device.last_seen.map(|t| format!("last seen {} ago", ago(now_ms() - t))).unwrap_or_else(|| "not connected".into()) };
        let load = d.device.load.as_ref().map(crate::load::words).unwrap_or_default();
        rows.push(vec![d.device.name.clone(), d.device.short.clone(), state, if d.device.remote_control { "on".into() } else { "off".into() }, d.worktrees.to_string(), d.sessions.to_string(), d.needs.to_string(), load]);
    }
    // Phones only watch: no worktrees, and remote control is this device's to give, not theirs.
    for p in &view.phones {
        let kind = if p.kind == "tablet" { "tablet" } else { "phone" };
        let state = if p.online { format!("{kind}, connected") } else { p.last_seen.map(|t| format!("{kind}, last seen {} ago", ago(now_ms() - t))).unwrap_or_else(|| format!("{kind}, not connected")) };
        rows.push(vec![p.name.clone(), p.short.clone(), state, String::new(), String::new(), String::new(), String::new(), String::new()]);
    }
    for f in &view.found {
        rows.push(vec![f.name.clone(), f.short.clone(), if f.asking { format!("asked to link, code {}", f.code) } else { "on your tailnet, not linked".into() }, String::new(), String::new(), String::new(), String::new(), String::new()]);
    }
    for a in &view.asks {
        rows.push(vec![a.name.clone(), a.short.clone(), format!("asks to link, code {}", a.code), String::new(), String::new(), String::new(), String::new(), String::new()]);
    }
    if rows.is_empty() {
        println!("No other devices yet.");
    } else {
        print!("{}", table_text(&["DEVICE", "ID", "STATE", "REMOTE CONTROL", "WORKTREES", "SESSIONS", "NEED YOU", "LOAD"], rows));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn devices_are_named_by_name_short_id_or_key() {
        // As the app sends the view, and what an app of another build leaves out reads as empty.
        let view: server::DevicesSummary = serde_json::from_value(json!({
            "devices": [{ "key": "9f8e7d6c5b4a3928", "short": "9f8e7d6c5b", "name": "Mac mini", "worktrees": 3 }],
            "phones": [{ "key": "b7c6d5e4f3a29180", "short": "b7c6d5e4f3", "name": "Sam's iPhone", "kind": "phone" }],
            "found": [{ "key": "7c6b5a4938271605", "short": "7c6b5a4938", "name": "iMac" }, { "key": "7c6b00000000", "short": "7c6b000000", "name": "iMac Pro" }]
        }))
        .unwrap();
        assert_eq!((view.devices[0].worktrees, view.devices[0].sessions), (3, 0));
        assert_eq!(which_device(&view, "mac MINI", &[Listed::Linked]).unwrap().1, "Mac mini");
        assert_eq!(which_device(&view, "9f8e7d6c5b", &[Listed::Linked]).unwrap().0, "9f8e7d6c5b4a3928");
        assert_eq!(which_device(&view, "7c6b5a", &[Listed::Found]).unwrap().1, "iMac");
        assert!(which_device(&view, "7c6b", &[Listed::Found]).is_err(), "two keys start that way");
        assert!(which_device(&view, "iMac", &[Listed::Linked]).is_err(), "only the lists asked for");
        // Unlinking takes a phone too.
        assert_eq!(which_device(&view, "sam's iphone", &[Listed::Linked, Listed::Phones]).unwrap().0, "b7c6d5e4f3a29180");
        // The summary keeps a device's own fields beside its counts, as the app wrote them by hand.
        let back = serde_json::to_value(&view).unwrap();
        assert_eq!((back["devices"][0]["name"].as_str(), back["devices"][0]["worktrees"].as_u64()), (Some("Mac mini"), Some(3)));
        assert!(back.get("lanes").is_none());
    }
}
