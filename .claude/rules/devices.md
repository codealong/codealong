---
paths:
  - "src-tauri/src/tailnet.rs"
  - "src-tauri/src/peers.rs"
  - "src-tauri/src/peerfeed.rs"
  - "src-tauri/src/devicekey.rs"
  - "src-tauri/peer/**"
---

# Your other devices

What Tailscale 1.102's command and iroh 1.2 answer and do, as tailnet.rs and codealong-peer's
net.rs use them, and how devices are tested, moved from CLAUDE.md. Paths are under
`src-tauri/`.

- Where the `tailscale` command is, and what `status --json` and `whois --json <ip>` answer:
  `src/tailnet.rs` (`cli`, `parse_status`, `parse_whois`). `whois` answers for this device's
  own address too.
- A TCP listener binds to this device's 100.x address without asking anything. A stream
  accepted from a listener that doesn't block doesn't block either: `peer/src/wire.rs`
  (`Wire::tcp`).
- iroh's endpoint on number 0's relays and address lookup, dialing by key, the connection
  proving both keys, and its own tokio runtime under blocking readers and writers:
  `peer/src/net.rs` (header, `Iroh::start`, `finish`). The public relays are rate-limited:
  real use wants their Pro plan or a self-hosted `iroh-relay`. Address lookup is pkarr and DNS.
  The `EndpointId` is the Ed25519 public key; `PublicKey` prints as hex and parses hex or
  base32, and `fmt_short` is its first five bytes (`peer/src/proto.rs`, `short`).
- A stream from `open_bi` reaches the other side's `accept_bi` only once something is written
  on it.
- Two devices in one process over loopback: `cargo test peers` (linking, feeds, requests and
  remote control's refusal). By hand, `cargo test iroh_connects -- --ignored` connects two
  iroh endpoints on this device through the relays (it needs the internet), and `cargo test
  tailnet_here -- --ignored --nocapture` reads this device's tailnet and listens on it.
- Sends from another device to real sessions are fine to test, with remote control on.
  `CODEALONG_DEVICE_KEY=<hex>` gives a second copy of the app a device key of its own
  (`src/devicekey.rs`).
