# Plan: your devices, peer to peer

Status: built 2026-09-27 (fa29006, 92e20d6, 1d2147d, and the badges and grouping after), and
checked between two real devices over Tailscale and iroh; what's next is at the end. README's
*Your other devices* and CLAUDE.md's Tailscale and iroh notes have the result; this is the
reasoning.

This replaced a plan with a hosted service in the middle: a gateway every device kept a socket
to, a database of every device's sessions, search fanned out through it (this file's history
has it, as `cloud.md`). Decided against:

- **The gateway.** It would only have relayed. Every fact on a board has one owner, the device
  it describes, so keeping another device's copy is "take the owner's latest", with nothing to
  merge; conversations are read only on request, from the device that has them; and search
  already runs where the text is (`search.rs`). Devices talking to each other directly lose
  nothing, gain the local network's speed, and nothing we run can read or forge what they say.
- **Vercel for the gateway.** Its WebSockets (a beta since 2026-06) are pinned to one function
  instance and close at the function's time limit, five minutes by default, so a relay between
  two devices would need Redis between instances and reconnects every few minutes.
- **Trusting the tailnet alone.** Being on your tailnet links nothing: a tailnet can hold
  shared machines, and linking is the moment you say which devices see and act on which.

## The idea

Codealong on each of your devices keeps a connection open to Codealong on the others and trades
feeds with the ones you've linked, so each device's windows show the others' worktrees and
sessions beside its own. It asks the others things: a page of a session's conversation, what a
session waits on you for, a search of what was said, and, when remote control is on at that
device, to send a prompt, answer an approval or start a session there. The device asked runs
the function its own window's command runs, as `server.rs` does for the `codealong` command.
Off until `[devices] enabled = true` (Settings › Devices).

## Two ways to reach a device

- **Tailscale** (`tailnet.rs`), when it runs: it already knows your machines, gets through
  NATs, encrypts, and says whose each machine is. Codealong reads `tailscale status --json` for
  this device's tailnet address and your other devices, listens on a TCP port bound to that
  address only (47471 by default), dials your devices that are online (the one with the lower
  address first, so two devices don't both dial), and asks `tailscale whois` whose each
  incoming connection is, keeping only your own machines'.
- **iroh** (codealong-peer's `net.rs`), for linked devices anywhere: a device's key is its iroh address, so
  a device dials the key; iroh connects directly when it can (about 90% of pairs) and otherwise
  through number 0's relays, which pass encrypted packets along. Linked devices not connected
  are dialed through iroh after the tailnet has had its turn, less often each time one doesn't
  answer.

Either way the connection is JSON lines (codealong-peer's `proto.rs`), and both sides first prove they hold
the device key they claim: each signs the other's nonce and both keys. So trust goes by device
key whatever carried the connection, and one device is one key. The key is made when Devices is
first turned on and kept in the login keychain (`devicekey.rs`).

## Linking

Each device keeps its own list of the devices it's linked with (`devices.json`); a device sends
its feed only to devices on its list and answers only them. Two ways to link:

- **On your tailnet:** Settings › Devices lists your devices running Codealong that aren't
  linked. Link asks the other device, which shows the ask across the top of its board with a
  six-digit code both devices work out from the pair of keys; saying yes there links both.
- **Anywhere else:** a link code made on one device (its key and a one-time secret, good for
  ten minutes) and entered on the other, which dials the first through iroh and presents the
  secret.

Unlinking on one device unlinks both when they're connected.

## What goes between devices

- **The feed** (`peerfeed.rs`), cut from each board the engine publishes, as `barfeed.rs` cuts
  the bar's: the device's name, whether it takes remote control, plan usage, and its worktrees
  with their sessions, less what only works on that device (open targets, pids, resume and
  attach commands). A whole feed when a connection opens, then only the worktrees that changed,
  at most once a second. The receiving device keeps each linked device's last feed on disk
  (`device-feeds.json`), so a device that's asleep still shows as it was when last seen.
- **Keys:** another device's lane ids and session keys reach the windows as `@<short>:<its
  key>`, the short id being the device key's first five bytes. A command given such a key
  (`lib.rs`) asks that device instead of looking here: `session_page`, `session_newer`,
  `session_pending`, `session_answer`, `send_to_session`, `archive_session`, `mark_seen`,
  `start_session`. Seen goes to the device a session is on, so Your turn clears on every device
  at once.
- **Requests** (`actions.rs`, one table of what each does on the device asked; the names, and who may make each, are codealong-peer's `api::REQUESTS`): `session-page`,
  `session-newer`, `session-pending`, `session-stream`, `search`, `session-archive`, `mark-seen`,
  `start-options`, `session-watch` (peers.rs keeps it: the hosted sessions whose replies go to
  that connection as they're written, as `stream` messages), and, only with remote control on at
  the device asked, `session-send`, `session-answer`, `start` and `session-cancel`. Each of those
  is written to that device's `~/Library/Logs/Codealong/devices.log`, without the prompt.
- **Picking up where a feed left off:** a feed carries each lane's hash, and a device that
  reconnects sends back the ones it still has (`have`, just before its `standing`, which the
  first feed waits for), so only what changed since comes again. Hellos list what a device knows
  (`have`, `watch`), and new messages go only to devices that know them. A send
  whose only way in opens the app with the prompt ready (`Sent.opened`) is refused: nobody's
  there to send it.

## In the windows

The main window adds linked devices' lanes to its own (`src/devices.ts`), so every view shows
them. Each such lane carries a tag of its device (`device`: its name, short id, color and
whether it's connected), and everything from another device wears that device's badge: a
colored square and its name in a neutral chip, drawn as labels are, since the color names a
device and not a status. This device's own work wears none. The color is yours to pick in
Settings › Devices, kept with the link in `devices.json`; until you do, the device's key picks
one, the same on every device. Work on a device that's away stays, faded, as it was when last
seen. The sidebar lists your devices, each narrowing the views to itself, and Worktrees can
group by device (By device), each device's worktrees in bays of their own under its name. A
session on another device says where it is in its panel instead of offering to open it here,
and its reply box works only when that device takes remote control. Settings › Devices has the
switches, what each transport is doing, the linked devices, the ones to link, and link codes.

Projects line up by repo, not by name. A repo is the same on every device, while what a device
calls its project, and where its folders are, are that device's own (the other device may even
run as another user). So each device puts another device's worktree where its own worktrees of
that repo are, or else by its own repo patterns (`project_here` in peers.rs). A repo goes by the
name its remote has and by the one GitHub gives it now: each device asks GitHub once a launch
what its repos are called (`current_slug` in github.rs), and a worktree whose remote has an old
name carries the new one, so a checkout from before a repo or its owner was renamed still lines
up. A worktree whose repo matches nothing here goes into a project here of the name its own
device gave it, or keeps that name and shows in the project switcher as a project of its own.
Each device is consistent on its own terms, and nothing about projects is synced: config.toml
stays each device's, and a work device may rightly want projects of its own. Copying one
device's projects to another, to make the names match too, would be a button in Settings ›
Projects.

## Next

1. The quick panel and the floating bar: other devices' sessions in their lists (the bar's feed
   is the engine's, so `barfeed.rs` would take the peers' lanes too), and the quick panel's
   search asking `search_devices` beside `search_text`.
2. The `codealong` command: `codealong devices` lists, links and asks devices already;
   `sessions` and `session show` should take other devices' sessions too, through the app.
3. Starting a session on another device from the quick panel, with that device's
   `start-options`. Ghostty opens nothing while the screen is locked, so those starts should
   run in the background (`claude --bg`).
4. Check, with the screen locked, whether Ghostty's `input text` and `send key` reach existing
   tabs: that decides whether prompts and approvals typed into a terminal work from another
   device.
5. With remote control on, keep the device awake while its agents work (what `caffeinate -i`
   holds).
6. Teams: another tailnet user's devices, sharing only what each device allows (status, titles
   or excerpts) and answering team searches only in shared repos.
7. A phone view (ios.md: the iPhone app, a device that only watches), a web view, and todos
   synced between your devices.
