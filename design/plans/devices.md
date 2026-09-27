# Plan: your other Macs, peer to peer

Status: the engine and the main window's views built 2026-09-27 (fa29006, 92e20d6); what's
next is at the end. README's *Your other Macs* and CLAUDE.md's Tailscale and iroh notes have the
result; this is the reasoning.

This replaced a plan with a hosted service in the middle: a gateway every Mac kept a socket to,
a database of every Mac's sessions, search fanned out through it (this file's history has it,
as `cloud.md`). Decided against:

- **The gateway.** It would only have relayed. Every fact on a board has one owner, the Mac it
  describes, so keeping another Mac's copy is "take the owner's latest", with nothing to merge;
  conversations are read only on request, from the Mac that has them; and search already runs
  where the text is (`search.rs`). Macs talking to each other directly lose nothing, gain the
  local network's speed, and nothing we run can read or forge what they say.
- **Vercel for the gateway.** Its WebSockets (a beta since 2026-06) are pinned to one function
  instance and close at the function's time limit, five minutes by default, so a relay between
  two Macs would need Redis between instances and reconnects every few minutes.
- **Trusting the tailnet alone.** Being on your tailnet links nothing: a tailnet can hold shared
  machines, and linking is the moment you say which Macs see and act on which.

## The idea

Codealong on each of your Macs keeps a connection open to Codealong on the others and trades
feeds with the ones you've linked, so each Mac's windows show the others' worktrees and
sessions beside its own. It asks the others things: a page of a session's conversation, what a
session waits on you for, a search of what was said, and, when remote control is on at that Mac,
to send a prompt, answer an approval or start a session there. The Mac asked runs the function
its own window's command runs, as `server.rs` does for the `codealong` command. Off until
`[devices] enabled = true` (Settings › Devices).

## Two ways to reach a Mac

- **Tailscale** (`tailnet.rs`), when it runs: it already knows your machines, gets through NATs,
  encrypts, and says whose each machine is. Codealong reads `tailscale status --json` for this
  Mac's tailnet address and your other Macs, listens on a TCP port bound to that address only
  (47471 by default), dials your Macs that are online (the one with the lower address first, so
  two Macs don't both dial), and asks `tailscale whois` whose each incoming connection is,
  keeping only your own machines'.
- **iroh** (`irohnet.rs`), for linked Macs anywhere: a Mac's device key is its iroh address, so
  a Mac dials the key; iroh connects directly when it can (about 90% of pairs) and otherwise
  through number 0's relays, which pass encrypted packets along. Linked Macs not connected are
  dialed through iroh after the tailnet has had its turn, less often each time one doesn't
  answer.

Either way the connection is JSON lines (`peerproto.rs`), and both sides first prove they hold
the device key they claim: each signs the other's nonce and both keys. So trust goes by device
key whatever carried the connection, and one Mac is one key. The key is made when Devices is
first turned on and kept in the login keychain (`devicekey.rs`).

## Linking

Each Mac keeps its own list of the Macs it's linked with (`devices.json`); a Mac sends its feed
only to Macs on its list and answers only them. Two ways to link:

- **On your tailnet:** Settings › Devices lists your Macs running Codealong that aren't linked.
  Link asks the other Mac, which shows the ask across the top of its board with a six-digit code
  both Macs work out from the pair of keys; saying yes there links both.
- **Anywhere else:** a link code made on one Mac (its key and a one-time secret, good for ten
  minutes) and entered on the other, which dials the first through iroh and presents the
  secret.

Unlinking on one Mac unlinks both when they're connected.

## What goes between Macs

- **The feed** (`peerfeed.rs`), cut from each board the engine publishes, as `barfeed.rs` cuts
  the bar's: the Mac's name, whether it takes remote control, plan usage, and its worktrees with
  their sessions, less what only works on that Mac (open targets, pids, resume and attach
  commands). A whole feed when a connection opens, then only the worktrees that changed, at most
  once a second. The receiving Mac keeps each linked Mac's last feed on disk
  (`device-feeds.json`), so a Mac that's asleep still shows as it was when last seen.
- **Keys:** another Mac's lane ids and session keys reach the windows as `@<short>:<its key>`,
  the short id being the device key's first five bytes. A command given such a key (`lib.rs`)
  asks that Mac instead of looking here: `session_page`, `session_newer`, `session_pending`,
  `session_answer`, `send_to_session`, `archive_session`, `mark_seen`, `start_session`. Seen
  goes to the Mac a session is on, so Your turn clears on every Mac at once.
- **Requests** (`peer_answer` in `lib.rs`): `session-page`, `session-newer`, `session-pending`,
  `search`, `session-archive`, `mark-seen`, `start-options`, and, only with remote control on at
  the Mac asked, `session-send`, `session-answer` and `start`. Each of the last three is written
  to that Mac's `~/Library/Logs/Codealong/devices.log`, without the prompt. A send whose only
  way in opens the app with the prompt ready (`Sent.opened`) is refused: nobody's there to send
  it.

## In the windows

The main window adds linked Macs' lanes to its own (`src/devices.ts`), so every view shows
them; each lane carries its Mac's name (`mac`), shown before its repo. The sidebar lists your
Macs, each narrowing the views to itself. A session on another Mac says where it is in its
panel instead of offering to open it here, and its reply box works only when that Mac takes
remote control. Settings › Devices has the switches, what each transport is doing, the linked
Macs, the ones to link, and link codes.

## Next

1. The quick panel and the floating bar: other Macs' sessions in their lists (the bar's feed is
   the engine's, so `barfeed.rs` would take the peers' lanes too), and the quick panel's search
   asking `search_devices` beside `search_text`.
2. The `codealong` command: `codealong devices`, and `sessions` and `session show` for other
   Macs' sessions, through the app.
3. Starting a session on another Mac from the quick panel, with that Mac's `start-options`.
   Ghostty opens nothing while the screen is locked, so those starts should run in the
   background (`claude --bg`).
4. Check, with the screen locked, whether Ghostty's `input text` and `send key` reach existing
   tabs: that decides whether prompts and approvals typed into a terminal work from another Mac.
5. With remote control on, keep the Mac awake while its agents work (what `caffeinate -i`
   holds).
6. Teams: another tailnet user's Macs, sharing only what each Mac allows (status, titles or
   excerpts) and answering team searches only in shared repos.
7. A web or phone view, and todos synced between your Macs.
