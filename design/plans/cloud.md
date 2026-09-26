# Plan: Codealong across Macs and teams (the cloud)

Status: proposal, 2026-09-26; nothing is built. Decided against: keeping conversations or a
search index in the cloud (search goes out to the Macs instead), a connection per window or
per command (one per Mac, held by the app), and merging other Macs into the engine's board
(they arrive as a feed of their own). The decisions still open are at the end.

## The idea

Codealong sees the agents on one Mac. The cloud lets it see every Mac you run it on, and your
team's:

1. Every Codealong keeps one connection open to Codealong's own service and tells it what its
   board says: the Mac, its accounts, its worktrees and sessions and where each stands. That's
   the metadata, and the service keeps it.
2. Your other Macs show those sessions beside their own and can do to them what they do to
   their own: read the conversation, send a message, answer an approval, start a session
   there. The requests travel over the same connections; the service passes them along and
   keeps nothing of them.
3. Teammates see each other's metadata: whose agents are on which repo, branch and PR, and in
   what state. They can't act on each other's sessions.
4. Searching what was said goes out to every Mac in scope; each answers from the index it
   already keeps (`search.rs`), and the Mac that asked merges the answers. Conversations never
   rest in the cloud.
5. Sign-in, teams, devices and the metadata store are ours: no outside auth service, and
   nothing kept with a harness's vendor.

## Ground rules

- **Local first.** A Mac works as it does today with the cloud off, unreachable or gone. The
  cloud adds to what a Mac shows; it never stands between a click and a local action, and the
  engine stays the one writer of its own files.
- **Metadata is stored, content is relayed.** The service keeps what the board shows about a
  session: status, title, a short excerpt, branch, PR, times. Conversations, search hits and
  prompts cross it only in answer to a request, and it never writes them to disk or logs.
  Between your own Macs they're encrypted to the Mac that asked, so the service can't read
  them in passing either.
- **Anything that reads content is a question to the Mac that has it**, never a lookup in the
  service: search, a conversation, and later a worktree's diff or a file an agent wrote.
- **Each Mac decides what leaves it.** Sharing is set on the Mac (`[cloud]` in config.toml)
  and applied there before anything is sent; the service applies the same rules again, it
  doesn't make them.
- **A remote request is a Tauri command.** Another Mac's request runs the function a window's
  command runs, through the same engine handle, as `server.rs` already does for the
  `codealong` command. There's no second way of reading, sending, answering or starting.
- **Only your own Macs act.** Sending, answering and starting take a request signed by one of
  your Macs, and a Mac takes them only after you've turned remote control on at that Mac.

## What's there today

- The engine composes `model::Board` and hands it whole to every window (`engine::publish`,
  then the emit closure in `lib.rs`, which also feeds the bar): about 500 KB for 96 lanes, up
  to every 300 ms, skipped when `board_hash` hasn't changed. `barfeed.rs` cuts a small feed
  from each board and sends it only when it changes; the cloud feed works the same way.
- `server.rs` serves the `codealong` command JSON lines over a Unix socket. Each `Request`
  (status, board, todo-apply, todo-started, start, search) is a call on `engine::Handle`.
- Everything a window does to a session is a Tauri command keyed by the session's key:
  `session_page`, `session_newer`, `session_pending`, `session_answer`, `send_to_session`,
  `archive_session`, `mark_seen`, and `start_session` with a `StartSpec` (lane, account,
  place, prompt, model). The UI reaches them only through `src/api.ts`. The session panel
  follows a session by calling `session_newer` whenever the session's `lastActivity` or
  `status` changes (`SessionThread.tsx`).
- `search.rs` keeps an in-memory index of prompts and replies for `[search] days`, from Claude
  transcripts, Codex rollouts and OpenCode's database. `find(words, limit)` returns `Found`
  (hits of `key`, `snippet`, `you` and `at`, plus `building` and `off`): a small answer, made
  where the text is, which is the shape a fan-out needs.
- Keys are this Mac's. A session is `harness:account:id`, and the account is this Mac's name
  for it (`claude2`); a lane is the worktree's absolute path; a todo is `t-<base36>`. Paths
  collide across Macs laid out alike, and account names mean different things on different
  Macs. Only the harnesses' own session ids (UUIDs, `ses_…`) are unique everywhere.
- Local state sits next to config.toml: `state.json` (seen, pinned, archived), `todos.json`,
  `canvas.json`, `insights.json`, `jev-todos.json`, and attachments by content in `images/`
  and `files/<hash>/`.
- The app opens no network connections of its own: Claude usage and TypeSafe go through curl
  with secrets on stdin, GitHub through `gh`. Secrets live in the login keychain (`jev.rs`).
  The engine and the socket are threads and channels; none of the app's own code is async.

## How it fits together

```
  Your MacBook                      Codealong service                 Your Mac mini
 ┌─────────────────────┐      ┌───────────────────────────┐      ┌─────────────────────┐
 │ engine ─► board     │      │ gateway, a room per team: │      │     board ◄─ engine │
 │              │      │      │ presence, feeds, relay,   │      │      │              │
 │              ▼      │ wss  │ search fan-out            │ wss  │      ▼              │
 │ windows ◄─► cloud.rs│◄════►│                           │◄════►│cloud.rs ◄─► windows │
 │                     │      │ Postgres: people, teams,  │      │                     │
 │ a remote key's      │      │ Macs, feeds, audit        │      │ requests run what   │
 │ command goes out    │      │                           │      │ lib.rs's commands do│
 │ through cloud.rs    │      │                           │      │                     │
 └─────────────────────┘      └─────────────▲─────────────┘      └─────────────────────┘
                                            ║ wss
                            teammates' Macs; later a browser
```

Three new pieces:

1. **The service**, one Rust binary (axum, tokio, sqlx) and a Postgres database: sign-in,
   teams and devices, and a gateway that holds every Mac's WebSocket, knows which are online,
   keeps each Mac's latest feed, relays requests and fans searches out.
2. **The link on each Mac**, `cloud.rs`: a thread that holds the connection, sends this Mac's
   feed (`cloudfeed.rs`), answers requests by calling what `lib.rs`'s commands call, and
   passes other Macs' feeds to the windows.
3. **`protocol/`**, a crate of the wire types both sides compile against, so the app and the
   service can't disagree about a message.

A WebSocket over TLS, because it's the one two-way connection that gets through every proxy
and firewall on port 443, and a browser can speak the same protocol when the web client comes.
On the Mac, `tungstenite` with rustls: it blocks, so the link is a thread like the socket's,
and the app's own code stays free of async.

## Identity: people, teams and Macs

**People** have an account in our service. They sign in with GitHub, since every Codealong
user has an account there (`gh` is a requirement) and knowing their login lets team views put
a name to a PR's author, or with a one-time link sent to their email; passkeys later. People,
their sign-in identities and their web sessions are rows in our database, and every token is
ours.

**Teams** have owners and members, who join by invitation (to an email or a GitHub login). A
team names its shared repos as patterns, like `github.com/acme/*`, the way config.toml's
projects do, and what's in those repos is what the team sees. Someone working alone is a team
of one, so there's one way through the code. In the first version a person is in one team.

**Devices** are Codealong installs: a Mac, as one macOS user. Linking one gives it an id
(`dev_…`) and a name (the computer's name, which you can change), and makes two key pairs on
the Mac, Ed25519 to sign and X25519 to encrypt. The private halves stay in the login keychain,
like Jev's key; the service gets the public halves. A linked Mac proves itself by signing the
service's challenge each time it connects, so no bearer token sits on it to be stolen, and
unlinking it from anywhere ends its access.

A Mac is *signed in* once you confirm it in the browser: it sends its feed and sees metadata.
It's *trusted* once a Mac you already trust approves it: the new Mac shows a six-digit code,
the trusted one shows the same code and asks, and Approve signs the newcomer's public keys.
Each Mac keeps its own list of the keys it trusts, so the service can't slip in a device of
its own. Only a trusted Mac can act on your sessions, read their conversations, or search
more of them than your team may. Your first Mac is trusted when it links; if you lose every
Mac, the next one you link starts a new trusted list and the old ones are unlinked.

**Keys across Macs.** In the service a session is its Mac's id and its local key, and so is a
lane. On a Mac, a session from another Mac has the key `@<device id>:<local key>`, as in
`@dev_7q2m:claude:claude2:9f3c…` or `@dev_7q2m:/Users/sam/projects/app`. No local key starts
with `@`, so a command can tell at a glance that a key belongs elsewhere, and the owning Mac
gets back exactly the key it knows.

## What each Mac sends: its feed

`cloudfeed.rs` cuts a feed from each board the engine publishes, as `barfeed.rs` does for the
bar, and hands it to the link, which keeps a hash of each thing it last sent and sends only
what changed:

- **The Mac**: name, model, macOS and app versions, its harnesses, load average (for choosing
  where to start something), and whether remote control and team search are on.
- **Accounts**: name, harness, signed in, plan, usage meters; the email only to your own Macs.
  One Claude account signed in on two Macs is one account in the service, which shows its
  meters once, the latest reading winning.
- **Lanes**: project, repo, repo slug, branch, callsign, origin, bay, glyph, headline, the
  PR's number, state, checks and merge state, git ahead, behind and dirty, last activity,
  looks finished. The path only to your own Macs; teammates get repo, branch and callsign.
- **Sessions**: harness, account, surface, status, live, title, name, headline, excerpt,
  detail, branch, lane, started, status since, last activity, last prompt, archived, stalled
  since, and Jev's insight. Not `pid`, `resume_command`, `attach_command` or `open_target`,
  which mean nothing on another Mac.

Each thing says who may see it: `own` (your Macs) or `team` (it's in one of the team's repos
and `[cloud] share` isn't off). For team things the Mac cuts the fields down to what `share`
allows before sending: `status` (repo, branch, PR, status and times), `titles` (and the title
and headline) or `excerpts` (and the excerpt and detail). Excerpts quote commands and replies,
so every excerpt, your own Macs' too, goes through a redaction pass for token-shaped strings
(`sk-…`, `ghp_…`, `Bearer …`, AWS keys) before it leaves.

Pacing: the whole feed on each connect, then changes at most once a second. A change of status
goes at once; a session whose only change is its excerpt goes at most every five seconds. A
whole feed is a few hundred KB of JSON and is sent compressed; changes are a few KB. The
engine's own publishing doesn't change.

The service keeps each Mac's latest feed in the team's room and as rows in Postgres, so a Mac
that's asleep still shows, as of when it was last seen, and a Mac that links later has
everything at once. It passes changes on to the Macs that may see them: yours at once, your
teammates' batched every few seconds. What leaves a feed leaves the service; a Mac's rows go
when it's unlinked, or 30 days after it was last seen.

## What each Mac answers: requests

A request names a Mac, a method and its parameters, and the service passes it to that Mac's
connection:

| Method | Runs | Who may ask |
| --- | --- | --- |
| `search` | `search::find` | your trusted Macs; teammates' when team search is on |
| `session-page`, `session-newer` | `transcript::page`, `transcript::newer` | your trusted Macs |
| `session-pending` | `Adapter::pending` | your trusted Macs |
| `session-answer` | `Adapter::answer` | your trusted Macs, signed, remote control on |
| `session-send` | `send::send` | your trusted Macs, signed, remote control on |
| `start-options` | `start::options_for` | your trusted Macs |
| `start` | `start_and_link` | your trusted Macs, signed, remote control on |
| `session-archive`, `mark-seen` | `Handle::archive`, `Handle::mark_seen` | your trusted Macs, signed |

The Mac that owns the session checks everything itself: the key is on its board, the asker
may ask this, the signature is from a Mac it trusts, the request is fresh (an id it hasn't
seen, and a time within five minutes of its own clock), and, for anything that changes a
session, that remote control is on here. Then it runs the function the Tauri command runs and
replies. Replies to your own Macs are sealed to the asker's key (libsodium's sealed boxes,
through the `crypto_box` crate; a browser opens them with libsodium.js), so the service
passes on bytes it can't read.

`mark-seen` going to the owning Mac is how Your turn clears everywhere at once: the owner
updates its `state.json`, the session's status changes in its feed, and every Mac sees the
change. There's no seen state to sync. Pins stay each Mac's own: pinning another Mac's session
keeps its `@` key in this Mac's `state.json`.

Not remote: opening a session where it lives, a folder in the editor, or a URL, which would
happen on a screen you aren't at; Settings, accounts and installing. On another Mac's session
the default action is its panel.

On the Mac that asks, each command in `lib.rs` that takes a session key checks for `@`: a
local key goes where it goes today, a remote one to `cloud::ask(device, method, params)`,
which waits for the reply up to a deadline (3 s for a search, 10 s for a page, 20 s to send or
start). When the Mac isn't connected, the service answers at once: "The Mac mini is asleep;
last seen 2 h ago." The UI hardly changes: the session panel calls `sessionPage` and
`sessionNewer` with the key it has, and follows another Mac's session as it follows its own,
when its `lastActivity` or `status` changes, which now arrives in the feed.

## Search that fans out

1. You type in the quick panel or the Sessions view. The Mac searches its own index at once,
   as today.
2. It sends `search { words }` to the service, which passes it to every connected Mac in
   scope but the asker: your other Macs, and teammates' Macs that answer team searches.
3. Each runs `search::find` on its own index and replies with its `Found`. A teammate's Mac
   answers only for sessions in the team's repos, with at most 20 hits, snippets only, and
   redacted like excerpts.
4. The service passes each answer back as it arrives, then says which Macs answered, which
   were offline and which ran out of time, so the list can say "3 of 4 Macs answered; Sam's
   MacBook is asleep."
5. Hits join the list with their Mac and person. One of yours opens its panel; a teammate's
   shows its metadata and the snippet.

Titles, branches, repos and PRs don't need a fan-out: each Mac already has every feed it may
see, so the quick panel's matching on those fields covers other Macs' sessions as it stands.
Only what was said goes out. The quick panel already waits for a pause in typing; a newer
search cancels the one before it at the service too, and the service caps searches per person
per minute. Every Mac lists, in Settings › Cloud, the searches it answered for teammates: the
words and who asked.

The trade-off: a Mac that's asleep can't answer, so what was said on it can't be found until
it wakes. An index in the cloud would fix that, and is what this design exists to avoid.

## Acting on your other Macs

Sending a prompt to an agent is running code on that Mac: agents run commands, some without
asking. So the service can pass control along but can't forge it. A Mac acts only on a
request signed by a Mac on its own trusted list, and only when remote control is on, which is
turned on at that Mac, in Settings, and nowhere else. Every remote action is logged on both
Macs and, without its content, in the service: who, from which Mac, to which session, the
method, when, and what happened. Unlinking a Mac takes it off every trusted list.

Remote control is wanted most when you're away from the Mac, so its screen is locked or its
display asleep, and some of today's ways in need the screen:

- Ghostty doesn't open terminals while the screen is locked (CLAUDE.md, Ghostty), so a remote
  start in a new terminal fails. Remote starts should use Claude Code's `--bg`, a background
  job with no terminal that `attach` opens later, which Codealong knows of and doesn't use
  yet. The Claude and Codex apps' new-session pages wait for you to press send there, so
  `start-options` offers another Mac only the places that start on their own.
- Whether Ghostty's `input text` and `send key` reach existing tabs while the screen is locked
  is unchecked. It decides whether `send::Route::Type` and Claude Code approvals by keypress
  in Ghostty work from another Mac. tmux, the daemon route (background jobs), the
  cross-session socket, OpenCode's service and the Codex app's IPC don't need the screen.
- Routes that end with an app opening with the prompt filled in (`Sent.opened`) send nothing
  when nobody's at that Mac, so a remote send skips them and says so.
- A sleeping Mac has no connection. With remote control on, Codealong can hold a power
  assertion (what `caffeinate -i` holds) while any of its sessions is working or waiting on
  you, and let the Mac sleep otherwise. A MacBook with its lid closed sleeps anyway.

A message sent from another Mac is text in the first version: a pasted screenshot is a file on
the Mac you typed on. Later the files travel with the request into the owning Mac's attachment
store (`attachments::import`) and the text points at them there.

## In the app

- **Settings › Cloud**: link this Mac (the browser opens to confirm it, then another of your
  Macs approves it), its name, what the team sees, team search, remote control, and the
  searches answered for teammates. It writes `[cloud]` in config.toml like every other pane:

  ```toml
  [cloud]
  enabled = false
  server = ""              # the service's address; empty is Codealong's own
  share = "titles"         # what teammates see: "off", "status", "titles" or "excerpts"
  team_search = true       # answer teammates' searches in the team's repos, with snippets
  remote_control = false   # let your other Macs send, answer approvals and start sessions here
  ```

- **Devices** in the sidebar: your Macs, each with its online dot and how many sessions need
  you there, then the team's people and theirs.
- **Sessions and Worktrees**: other Macs' sessions and worktrees beside this Mac's, marked
  with the Mac's name, and a filter for This Mac, My Macs or Team. A worktree belongs to its
  Mac, since its path is that Mac's; the same branch checked out on two Macs shows on each,
  linked by repo and branch.
- **PRs**: On this Mac becomes On your Macs, and the team's sessions that worked on a PR show
  under it.
- **The quick panel, the floating bar and the mark** count what needs you on every one of
  your Macs. The new-session row can start on another Mac, with its worktrees and accounts
  from its `start-options`.
- The engine's board stays this Mac's. `cloud.rs` keeps the other Macs' feeds and sends them
  to the windows as a `devices` event (as the bar gets `bar-feed`), and `src/lanes.ts` merges
  them into the views. A change on another Mac doesn't republish the 500 KB board, and
  `board.rs` never has to know about other Macs.

## The command and the skill

A new `codealong cloud status|link|unlink|devices`, and the existing commands learn about
other Macs through the app: `sessions --devices mine|team`, `sessions find` searching every
Mac in scope and saying where each hit is, `session show @dev_…:<key>` printing another Mac's
conversation. The skill then tells an agent to look on your other Macs before redoing what an
agent there did in the same repo. With the app closed the command has no cloud, as it has no
git.

## The service

- **One binary**, in `cloud/` in this repo, beside `protocol/`, in one Cargo workspace with
  `src-tauri`. One process and one managed Postgres to start, on whatever the team already
  runs; the same binary is what someone hosting their own runs.
- **Rooms.** The gateway keeps a room per team in memory: its connected Macs, their latest
  feeds, and requests in flight. Presence, relaying and fan-out all happen inside a room, and
  nothing crosses teams, so outgrowing one process means sharding by team (each team's Macs
  connect to one process), with no messaging between processes.
- **Postgres**: `people`, `identities`, `teams`, `memberships`, `devices` (public keys, who
  approved each, last seen, unlinked), `lanes` and `sessions` (a row per Mac and key: the
  plain fields as columns, for team views and the web, the rest as JSON), `accounts`, `audit`.
  Feeds are written behind, coalesced, never per message.
- **HTTP** for everything but the Macs' connections: signing in, linking a Mac (OAuth's device
  authorization grant, RFC 8628: the Mac shows a code and opens the browser, you confirm it,
  the Mac asks until it's confirmed), and pages for your account, your Macs and your team.
  The Mac makes those few calls with curl, as `jev.rs` calls TypeSafe.
- **An operator view** for us: how many people, teams and Macs, and which app and protocol
  versions connect, so we know when an old version can be dropped. Counts and versions, never
  anyone's sessions.
- **Never** logs or stores a request's or reply's body; metadata is encrypted at rest. The
  service talks to no harness, GitHub or TypeSafe: every Mac still does its own.

## The protocol

JSON, one message per WebSocket frame. Every field but ids is optional and unknown ones are
skipped, so an older app and a newer service get along; `v` in the hello says which version
each speaks, and the service keeps speaking the one before.

```
Mac → service   {"t":"hello","v":1,"device":"dev_7q2m","app":"0.1.0"}
service → Mac   {"t":"challenge","nonce":"…"}
Mac → service   {"t":"auth","sig":"…"}
service → Mac   {"t":"welcome","person":"p_…","team":"tm_…","teamRepos":["github.com/acme/*"],"trusted":[…]}
Mac → service   {"t":"feed","rev":1,"full":true,"device":{…},"accounts":[…],"lanes":[…],"sessions":[…]}
Mac → service   {"t":"feed","rev":2,"sessions":[{"key":"claude:claude2:9f3c…","status":"needs"}],"gone":{"lanes":[…]}}
service → Mac   {"t":"feeds","device":"dev_k4x1","rev":88,"sessions":[…]}          another Mac's changes
service → Mac   {"t":"presence","device":"dev_k4x1","online":false,"lastSeen":1790300000000}
service → Mac   {"t":"req","id":"r_…","from":"dev_k4x1","method":"session-page","params":{…},"at":…,"sig":"…"}
Mac → service   {"t":"res","id":"r_…","ok":true,"sealed":"…"}                        sealed to dev_k4x1
Mac → service   {"t":"res","id":"r_…","ok":false,"error":"Remote control is off on this Mac."}
```

## Todos, later

`todos.json` is one Mac's list; with several Macs you'd want one. The service can keep each
person's list, with every Mac keeping `todos.json` as its copy, for the command and for when
it's offline. `TodoOp` is already a list of changes (create, update with a patch, link,
delete, restore, order, labels), so a change goes to the service, which puts every change in
one order and sends it to every Mac; one made offline waits and goes when the Mac is back.
Patches merge field by field, the later write winning; order and labels are whole lists, the
later one winning. Links name sessions and worktrees, which become `@` keys for other Macs.
Attachments are already kept by content, so they'd go to a blob store by hash when a todo that
uses them syncs. After that, a team's shared list, with todos handed to a teammate, and
canvases the same way. Nothing before this depends on it.

## Phases

1. **See**: the service with sign-in, teams, invitations and linking; the gateway with
   presence and feeds; `protocol/`; `cloud.rs` and `cloudfeed.rs`; Settings › Cloud, Devices
   in the sidebar, and other Macs' sessions and worktrees in the views, read-only; `codealong
   cloud`. `codealong --snapshot --cloud-feed [--share <level>]` prints what this Mac would
   send, so the feed is checked without a service, and
   `PREVIEW_DEVICES=scripts/preview/devices-sample.json` puts other Macs in the preview. Done
   means two Macs linked to one account each show the other's sessions within a second of a
   change, a teammate's Macs show at the chosen level, and a Mac with the cloud off, or the
   service down, works as today.
2. **Ask**: trusting Macs (the approval code), requests and sealed replies, search fanned out
   to your Macs and then to teammates', other Macs' conversations in the session panel, and
   what they wait on you for. Done means a search on one Mac finds what was said on another,
   and another Mac's session reads in its panel as it would at home.
3. **Act**: signed requests to send, answer, archive, mark seen and start (`--bg` for Claude
   Code); the remote control switch; the power assertion; audit logs. First, check Ghostty
   with the screen locked. Done means answering an approval on the Mac mini from the MacBook
   while the mini's screen is locked, and a Mac with remote control off refusing to.
4. **Anywhere**: the web client, the same React app with `api.ts` speaking the protocol
   instead of Tauri's IPC (the preview already runs the UI in a browser with the IPC stubbed),
   for any browser, a phone's included. A browser links as a Mac does, keeping its keys in its
   own storage, and is approved from a Mac. Notifications when something needs you.
5. **Share**: todos through the service, then team todos and canvases.

Each phase updates README (what it does, and "Permissions and safety") and CLAUDE.md (the
service's commands, the protocol, running it locally), as the agent-cli plan's phases did.

## Decisions to make

- **Where the service runs.** The plan assumes a container and a managed Postgres on
  infrastructure the team already has. Cloudflare's Durable Objects are the strongest
  alternative: an object per team is the room, holds its Macs' sockets and sleeps when idle,
  at the cost of TypeScript and their storage instead of Postgres.
- **Sign-in.** GitHub and email links are proposed. GitHub alone is less to build, since every
  user has it.
- **What teammates see by default.** Proposed: `titles` for the team's repos, nothing outside
  them, and team search on, so joining a team is the opt-in and each person can turn it down
  on their Macs. The alternative is all of it off until each person turns it on, which is
  safer and leaves the team view empty until they do.
- **Sealing from phase 2.** Proposed, since the keys come with linking. Waiting until phase 3
  means conversations cross the service readable for a phase, on its promise not to keep them.
- **Sealing metadata too.** Titles and excerpts could be sealed to your own Macs, so the
  service holds only structure: status, repo, branch, times. It's stronger, but the web client
  needs keys to show anything, notifications can't carry text the service can't read, and team
  views need a key the whole team shares. Proposed as a later option; redaction now.
- **One team per person** in the first version. Several would mean a Mac's feed cut per team.
- **Where the code lives**: here, sharing `protocol/` with the app (proposed), or a repo of its
  own with the protocol as a dependency.

## Risks

- Remote control is remote code execution by design. Signing, each Mac's own trusted list, the
  switch on each Mac, and the audit are what keep a breach of the service from reaching every
  agent. Test them as CLAUDE.md says to test actions that reach outside the Mac: never on real
  sessions.
- Team search trusts the service to say who's on the team, so a breach could search your
  sessions in the team's repos as a teammate. That's why it answers only there, with snippets,
  redacted, and lists every search it answered.
- Titles and excerpts say what agents are doing, in their words. They're stored, redacted or
  not; Settings and README's "Permissions and safety", which today say transcripts and
  excerpts stay on the Mac, have to say so plainly.
- Clocks: times are each Mac's. The service notes when it heard each one, and tells a Mac
  whose clock is off by more than a minute, since fresh requests depend on it.
- Cost on the Mac: the feed is cut from boards the engine publishes anyway, hashed per thing,
  and `perf.log` gets a line per feed sent, so `[debug] perf_log` shows what it costs.
