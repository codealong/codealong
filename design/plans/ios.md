# Plan: Codealong for iPhone

Status: planned 2026-09-27. The spike linked the simulator with a Mac and listed its sessions;
codealong-peer landed 2026-09-28 (f44c553), then phones as a kind of device, QR link codes and the
typed requests (items 1 and 2 below). The app is its own repo, github.com/codealong/codealong-ios,
at `../codealong-ios`.

## The idea

A native app for managing sessions across all your Macs from your phone: every session each Mac
tracks, in one list, most urgent first, with the approvals, replies, archiving and new sessions
that remote control allows. It doesn't need a backend of its own. Devices (devices.md) already
does what it needs: Macs trade feeds of worktrees and sessions, prove their device keys to each
other, link with a one-time code over iroh, and answer `session-page`, `session-pending`,
`session-send`, `session-answer`, `start` and `search`. The phone is a device that only watches:
it links, keeps feeds and asks, and has no board to share and answers nothing. The Mac already
works out each session's status, headline and detail, so the phone shows what it's sent.

## Sharing code with the phone: codealong-peer

All of that is Rust, and iroh has no Swift implementation (its Swift package is Rust underneath).
A Swift client would copy the protocol, the handshake, link codes and the model, and drift from
them. So what goes between devices is a crate of its own, `codealong-peer`, in this repo at
`src-tauri/peer/` (a second package in the engine's Cargo workspace, so one `cargo test` runs
both):

- `proto`: the messages and each side's proof of its key (was `peerproto.rs`).
- `net`: link codes and iroh (was `irohnet.rs`); `wire`: a connection, whatever carries it.
- `feed`: `FeedMsg`, `Replica` and `DeviceInfo` (the sending side stays in `peerfeed.rs`).
- `model`: the worktrees, sessions, PRs and plan usage a feed carries (the board and what only
  the Mac app shows stay in the engine's `model.rs`, which re-exports the rest).
- `client`: a device that only watches, which the phone runs and the engine's tests can run
  against `peers.rs`.

The engine keeps its module names (`crate::peerproto`, `crate::model::Session`), so nothing
else changed. The phone's `core/` is a thin UniFFI layer over the client, built into an
XCFramework; SwiftUI gets typed records, never JSON. It depends on `codealong-peer` by git,
pinned to a commit in `origin/main`: never a path into the main checkout.

A separate repo, as asked; a monorepo would only add landing a protocol change and the phone's
side in one commit, and the client's test living here covers most of that.

## Changes here, in order

1. The crate (above), after the ACP branch lands, since it edits `model.rs`. With it: peers skip
   messages they don't know (today an unknown one ends the connection), and the hello lists
   what a device understands, so new messages go only to peers that do.
2. Phones as a kind of device: Macs don't dial phones (they redial every linked device today)
   or ask them to search; Settings › Devices lists them apart and shows the link code as a QR
   code for the phone's camera.
3. Resumable feeds: every connection starts with the whole feed (the Mac mini's is 112 KB; this
   Mac's board is 705 KB). The phone reconnects every time it's opened, so it sends back what it
   holds and gets only what changed.
4. ACP for other devices: acp.rs's streaming reply to a device watching the session,
   `session-cancel`, and ACP's permission options (Always allow) in `Pending`. A session
   Codealong hosts over ACP takes prompts, approvals and stop through the engine, so it works
   with the Mac's screen locked; terminal sessions go through Ghostty, which opens nothing then.
   The phone starts its sessions in the `codealong` place when the Mac offers it.
5. Maybe: the device info says when the screen is locked (no terminal starts), and the Mac stays
   awake while remote control is on and agents work (devices.md, Next).

## The app

iOS 26, SwiftUI with Observation, Swift 6, no third-party Swift packages, an XcodeGen spec.

- Inbox: one list across Macs: Needs you (Allow and Deny in the row where the Mac can answer),
  Your turn, Working (live timers), Recent. Each row names the Mac, the project and branch, and
  shows the Mac's headline. Swipe to mark seen or archive; filter by Mac or project; search
  titles, or what was said on every Mac.
- Session: the conversation a page at a time, refreshed when the feed says the session moved
  (streamed for ACP sessions); the approval card; a reply box that says why when the Mac is
  offline or has remote control off; archive and stop.
- New session: Mac, worktree, account and place (`start-options`), prompt.
- Macs: linked Macs, whether they're connected, remote control, plan usage; link by QR or code.

Fast by construction: the core holds the connections and state and tells Swift at most every
150 ms; the app opens on the feeds it saved and connects meanwhile; it disconnects in the
background; rows redraw only when they change. Security: the key stays in this phone's Keychain,
links take one-time codes, and each Mac's remote control switch and devices.log still apply.

## Phases

1. Spike: link the simulator with this Mac through `codealong devices code` and list its
   sessions: the key, linking, iroh on iOS and the handshake with the real app.
2. Items 1–2 here, landed.
3. iOS v1: the four screens, saved feeds, background handling, tests; screenshots light and dark.
4. Items 3–4 and the phone's side of them; measure reconnect bytes and time.
5. Later: notifications, widgets, the iPad layout, TestFlight.

## Notifications

Without a server the phone only knows while it's open, so v1 has none. Later, either each Mac
sends pushes with your own APNs key (fine for your own builds, not for other people's), or a
small relay holds Codealong's key and passes along end-to-end encrypted messages (works for
everyone, but it's the one server devices.md chose not to have).
