//! Session emblems (`[emblems]`): something you can name beside each session's title, an emoji
//! or a small picture, so the sessions beside each other read apart at a glance and one is
//! remembered as "the dinosaur".
//!
//! - `kind = "emoji"`: Jev picks the emoji that stands for what the session works on, from
//!   `EMOJI`, by its title and first prompt (or its latest, where its harness keeps only that).
//!   Its answer ranks them, and the session gets the likeliest that no other current session of
//!   its project shows, among those nearly as likely as the first (`pick_emoji`).
//! - `kind = "drawn"`: an account that draws (`Adapter::draw`, Codex's image tool) draws one
//!   thing it picks for what the session works on, told what the sessions beside it show so it
//!   picks something else, and drawn as `[emblems] prompt` says (`draw_prompt`). The picture is
//!   cut to the thing, a plain white background made clear, and kept small (`shrink`) as a kept
//!   image, which the windows load and other devices ask for. One at a time: each takes a minute
//!   or so and an image turn of the account's usage.
//!
//! What a session is given is kept for its life (emblems.json), whatever its title becomes, and
//! the newest sessions are given theirs first. A helper shows the emblem of the session it works
//! for and gets none of its own. The engine asks (`engine.rs` `look_emblems`, `draw_emblem`);
//! `Store::emblem_of` is what each session shows.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::config::{AccountCfg, EmblemKind};
use crate::jev;
use crate::model::{Emblem, Session};
use crate::util::DAY_MS;

/// The emoji Jev picks from: a name, the emoji, and what it stands for, which is what Jev reads.
/// Each is a thing you can name at a glance; what it stands for leans on what coding sessions
/// work on, and on a pun where one is handy (a spool for threads, a dinosaur for Rex). Jev takes
/// at most 255 options.
pub const EMOJI: &[(&str, &str, &str)] = &[
    ("bug", "🐛", "A bug: something broken, a crash, an error, a regression."),
    ("ladybug", "🐞", "A small bug: a glitch, a cosmetic problem, a nit, a flaky test."),
    ("fire", "🔥", "Something urgent: an outage, a hotfix, an incident in production."),
    ("ambulance", "🚑", "Recovering from a disaster: a rollback, restoring lost data, a post-mortem."),
    ("test-tube", "🧪", "Tests: writing or running them, coverage, a test harness, checks."),
    ("microscope", "🔬", "Looking closely: debugging one path, tracing why something happens."),
    ("magnifier", "🔍", "Search: a search feature, finding things, indexing, lookups."),
    ("detective", "🕵️", "An investigation: hunting down a cause, gathering evidence, auditing."),
    ("compass", "🧭", "Exploring: finding your way around a codebase, research, navigation."),
    ("map", "🗺️", "Maps: locations, geography, routes between places, a site map."),
    ("palette", "🎨", "Visual design: colors, styles, CSS, themes, how something looks."),
    ("paintbrush", "🖌️", "Painting the screen: rendering, repainting, drawing, fills."),
    ("crayon", "🖍️", "Sketching: a canvas, drawing tools, doodles, mockups."),
    ("frame", "🖼️", "Images: photos, icons, image processing, thumbnails."),
    ("camera", "📸", "Screenshots: capturing the screen, snapshots, photos taken."),
    ("movie", "🎬", "Video and motion: animation, recordings, transitions."),
    ("music", "🎵", "Music and audio playback."),
    ("microphone", "🎤", "Voice: speech input, dictation, recording audio, a talk."),
    ("speaker", "🔊", "Sounds: sound effects, alerts that play, volume."),
    ("bell", "🔔", "Notifications: alerts, badges, reminders."),
    ("envelope", "✉️", "Email: sending mail, newsletters, mail templates."),
    ("mailbox", "📬", "An inbox: messages arriving, incoming mail, a queue of things to read."),
    ("speech", "💬", "Chat: conversations, messages, replies, transcripts."),
    ("thought", "💭", "Thinking: an agent's reasoning, ideas being weighed."),
    ("megaphone", "📣", "Announcing: marketing, a launch post, broadcasting to many."),
    ("newspaper", "📰", "News: a changelog, release notes, a blog post."),
    ("book", "📖", "Documentation: a guide, a README, explaining how something works."),
    ("books", "📚", "A library of documents: a knowledge base, references, a wiki."),
    ("memo", "📝", "Notes and drafts: writing prose, a letter, a report."),
    ("pencil", "✏️", "Editing words: wording, copy changes, renaming."),
    ("clipboard", "📋", "Copy and paste, the clipboard, forms to fill in."),
    ("checkmark", "✅", "Tasks: todos, checklists, marking things done, triage."),
    ("calendar", "📅", "Dates: calendars, events, schedules, planning."),
    ("alarm-clock", "⏰", "Alarms: timers, snoozing, reminders at a time, waking up later."),
    ("hourglass", "⏳", "Waiting: slowness, long-running work, progress bars, delays."),
    ("stopwatch", "⏱️", "Timing: benchmarks, measuring how long something takes."),
    ("zap", "⚡", "Speed: performance, latency, making something faster."),
    ("rocket", "🚀", "Shipping: a release, a deploy, launching, publishing a version."),
    ("package", "📦", "Packaging: dependencies, upgrades, npm, cargo, bundles, installers."),
    ("gift", "🎁", "A new feature as a present: a surprise, a welcome, onboarding."),
    ("toolbox", "🧰", "A kit of tools: utilities, an SDK, developer tooling."),
    ("hammer-wrench", "🛠️", "The dev setup: build scripts, CI, linting, configuration."),
    ("wrench", "🔧", "A small fix or adjustment: tuning, maintenance, tweaks."),
    ("hammer", "🔨", "Building: compiling, builds that break, construction."),
    ("nut-bolt", "🔩", "Nuts and bolts: low-level internals, fastening parts together."),
    ("gear", "⚙️", "Settings: preferences, options, configuration files."),
    ("robot", "🤖", "Agents and bots: automation, agents working on their own."),
    ("sparkles", "✨", "AI and polish: models, prompts, making something shine."),
    ("brain", "🧠", "Memory and reasoning: what an agent remembers, learning, intelligence."),
    ("crystal-ball", "🔮", "Predictions: suggestions, forecasts, guessing what's next."),
    ("dice", "🎲", "Chance: randomness, probabilities, sampling, shuffling."),
    ("puzzle", "🧩", "Add-ons: plugins, skills, extensions, pieces that fit together."),
    ("plug", "🔌", "Integrations: connecting another service, connectors, MCP servers."),
    ("link", "🔗", "Links: URLs, deep links, linking things together."),
    ("chain", "⛓️", "Chains: pipelines, steps that depend on each other."),
    ("hook", "🪝", "Hooks: git hooks, webhooks, React hooks, lifecycle hooks."),
    ("key", "🔑", "Sign-in: authentication, logins, tokens, API keys, the keychain."),
    ("old-key", "🗝️", "Secrets: old credentials, unlocking something long locked."),
    ("lock", "🔒", "Privacy and encryption: locking things down, permissions."),
    ("shield", "🛡️", "Security: vulnerabilities, sandboxing, protection, security reviews."),
    ("passport", "🛂", "Approvals: permission prompts, checks before something is allowed."),
    ("siren", "🚨", "Alarms: monitoring alerts, something flagged as wrong."),
    ("stop-sign", "🛑", "Stopping: cancelling, aborting, interrupting a turn, stop logs."),
    ("no-entry", "⛔", "Blocked: access denied, rejected calls, forbidden actions."),
    ("construction", "🚧", "Work in progress: a migration under way, something half-built."),
    ("traffic-light", "🚦", "Flow control: rate limiting, statuses, go and wait."),
    ("thermometer", "🌡️", "Health checks: metrics running hot, load, temperature."),
    ("chart-up", "📈", "Growth: analytics, usage going up, dashboards."),
    ("chart-down", "📉", "Falling numbers: costs or metrics going down, a drop."),
    ("bar-chart", "📊", "Data: statistics, reports, evaluations, numbers in charts."),
    ("abacus", "🧮", "Counting: calculations, math, totals, tokens counted."),
    ("money-bag", "💰", "Money: pricing, costs, billing, budgets."),
    ("credit-card", "💳", "Payments: checkout, cards, subscriptions."),
    ("receipt", "🧾", "Receipts: invoices, accounting, a record of what was spent."),
    ("cart", "🛒", "Shopping: a store, products, orders, a marketplace."),
    ("bank", "🏦", "Finance: banking, accounts and balances."),
    ("scales", "⚖️", "Weighing: comparing options, trade-offs, legal matters, licenses."),
    ("briefcase", "💼", "Business: a client, a job, professional matters, hiring."),
    ("handshake", "🤝", "Agreements: collaboration, partnerships, handing work over."),
    ("people", "👥", "People: users, teams, profiles, sharing, roles."),
    ("family", "👪", "Parents and children: lineage, child sessions, inheritance."),
    ("baby", "👶", "Something brand new: a first version, bootstrapping a project."),
    ("hatching", "🐣", "Hatching: something just born, a first run, an MVP."),
    ("egg", "🥚", "Beginnings: chicken-and-egg problems, setup before setup."),
    ("tree", "🌳", "Trees: hierarchies, file trees, a tree view."),
    ("seedling", "🌱", "Starting small: scaffolding, seeding data, growing something."),
    ("cactus", "🌵", "Something prickly: touchy code, a thorny problem."),
    ("mushroom", "🍄", "Spreading: things that multiply, growth in the dark, power-ups."),
    ("leaf", "🍂", "Falling away: deprecations, sunsetting, removing what's old."),
    ("sunflower", "🌻", "Cheer: brightness, light themes, a friendly touch."),
    ("rose", "🌹", "Something pretty: a polished design, a gift for someone."),
    ("bouquet", "💐", "A bunch of things: many changes gathered, a collection."),
    ("broom", "🧹", "Cleanup: refactoring, tidying, removing clutter, simplifying."),
    ("recycle", "♻️", "Reuse: caching, reusing worktrees, retries, recycling."),
    ("wastebasket", "🗑️", "Deleting: removal, garbage collection, archiving old things."),
    ("scissors", "✂️", "Cutting: trimming, splitting, cropping, removing parts."),
    ("sponge", "🧽", "Scrubbing: sanitizing data, soaking up input, cleaning."),
    ("spool", "🧵", "Threads: conversation threads, threading, concurrency, weaving things in."),
    ("yarn", "🧶", "Tangles: untangling messy code, knitting pieces together."),
    ("knot", "🪢", "A knot: deadlocks, tied-up dependencies, a hard problem."),
    ("t-rex", "🦖", "Something old or big and fierce: legacy code; anything called Rex."),
    ("sauropod", "🦕", "Something huge and slow-moving: big files, monoliths, big migrations."),
    ("dragon", "🐉", "A dragon to slay: a big hairy problem, a legendary bug."),
    ("unicorn", "🦄", "Something rare: an edge case, magic, an unlikely idea."),
    ("snail", "🐌", "Slowness: a slow build, a slow page, latency that crawls."),
    ("turtle", "🐢", "Steady pacing: backoff, throttling, patience."),
    ("rabbit", "🐇", "Quick hops: speedups, quick fixes, going down a rabbit hole."),
    ("octopus", "🐙", "GitHub: issues, pull requests, Actions, the gh command."),
    ("snake", "🐍", "Python: Python scripts and packages."),
    ("crab", "🦀", "Rust: Rust code, cargo, crates."),
    ("elephant", "🐘", "Big memory: Postgres, large databases, remembering a lot, PHP."),
    ("whale", "🐳", "Containers: Docker, images, Kubernetes."),
    ("penguin", "🐧", "Linux: running on Linux, distributions, its desktops."),
    ("apple", "🍎", "Apple's platforms: macOS, iOS, Swift, Xcode."),
    ("phone", "📱", "A phone app: iPhone, Android, mobile layouts."),
    ("laptop", "💻", "A laptop app: native windows, the Mac, its notch, the menu bar."),
    ("desktop", "🖥️", "Big screens: displays, monitors, where windows go."),
    ("keyboard", "⌨️", "The keyboard: shortcuts, key handling, typing, focus."),
    ("mouse", "🖱️", "The pointer: clicks, hover, drag and drop."),
    ("joystick", "🕹️", "Controls: input devices, steering, game controls."),
    ("game", "🎮", "Games: game logic, gamepads, playful features."),
    ("globe", "🌍", "The web: websites, the browser, the world, languages."),
    ("satellite", "📡", "Remote connections: syncing between devices, signals, polling."),
    ("antenna", "📶", "Connectivity: being online or offline, Wi-Fi, signal strength."),
    ("cloud", "☁️", "The cloud: hosting, workers, buckets, DNS, servers somewhere else."),
    ("storm", "⛈️", "A storm: many failures at once, flaky infrastructure, chaos."),
    ("sun", "☀️", "Light: light mode, mornings, a fresh start, daily jobs."),
    ("moon", "🌙", "Night: dark mode, sleeping, idle states, overnight jobs."),
    ("star", "⭐", "Favorites: stars, ratings, pins, highlights."),
    ("rainbow", "🌈", "Many colors: color palettes, gradients, variety."),
    ("snowflake", "❄️", "Freezing: frozen windows, hangs, cold starts, a code freeze."),
    ("ice", "🧊", "Cold storage: caches, frozen data, cooling down."),
    ("droplet", "💧", "Leaks: memory leaks, draining, something dripping away."),
    ("wave", "🌊", "Streams: streaming data, floods of events, waves of requests."),
    ("volcano", "🌋", "Eruptions: crashes that blow up, panics, overflows."),
    ("comet", "☄️", "Something fleeting: one-off runs, spikes, transient errors."),
    ("mountain", "⛰️", "A big climb: a large project, a hard milestone, peak load."),
    ("island", "🏝️", "Isolation: sandboxes, isolated worktrees, offline mode, time off."),
    ("house", "🏠", "Home: the home screen, home life, the household."),
    ("office", "🏢", "Organizations: companies, workspaces, enterprise, teams."),
    ("factory", "🏭", "Production: generators, making things at scale."),
    ("bridge", "🌉", "Bridges: adapters, interop, gateways between systems."),
    ("door", "🚪", "Doors: entry points, sign-in screens, openings and exits."),
    ("window", "🪟", "Windows: app windows, panels, Microsoft Windows."),
    ("mirror", "🪞", "Mirrors: reflections, replicas, previews, mirrored state."),
    ("ladder", "🪜", "Step by step: escalation, levels, incremental work."),
    ("school", "🏫", "Learning: tutorials, teaching, explaining a concept."),
    ("graduation", "🎓", "Mastery: onboarding done, skills learned, certification."),
    ("trophy", "🏆", "Winning: achievements, scores, goals reached, leaderboards."),
    ("medal", "🥇", "Ranking: sorting, order, priorities, first place."),
    ("target", "🎯", "Aiming: goals, accuracy, precision, targeting."),
    ("red-flag", "🚩", "Flags: feature flags, red flags, markers to look at."),
    ("finish-flag", "🏁", "Finishing: wrapping up, landing work, the end of a race."),
    ("pushpin", "📌", "Pinning: pinned items, keeping something in place."),
    ("bookmark", "🔖", "Bookmarks: saved views, saved places, marks to come back to."),
    ("label", "🏷️", "Labels: tags, names, attribution, versions, prices."),
    ("paperclip", "📎", "Attachments: attached files, uploads, clipping things together."),
    ("folder", "📁", "Folders: files, paths, directories, organizing."),
    ("cabinet", "🗄️", "Storage: archives, records kept, file storage."),
    ("floppy", "💾", "Saving: persistence, saved state, backups."),
    ("card-index", "🗃️", "Databases: tables, schemas, migrations, queries, SQL."),
    ("scroll", "📜", "Logs and history: log files, long records, scripts."),
    ("page", "📄", "Documents and formats: PDFs, parsing, importing, exporting files."),
    ("inbox-tray", "📥", "Importing: downloads, ingest, incoming data."),
    ("outbox-tray", "📤", "Exporting: uploads, sending out, publishing data."),
    ("refresh", "🔄", "Refreshing: reloads, syncing, loops that come back around."),
    ("repeat", "🔁", "Repeating: recurring schedules, cron jobs, loops, retries."),
    ("shuffle", "🔀", "Branching and merging: git branches, rebases, reordering."),
    ("anchor", "⚓", "Anchors: fixed positions, docking, anchors in a page, stability."),
    ("ship", "🚢", "A big crossing: a major release, a large move, a fleet."),
    ("airplane", "✈️", "Travel and moving: moving between machines, migrations, trips."),
    ("car", "🚗", "Driving: drivers, getting from place to place, a quick trip."),
    ("train", "🚆", "Trains: release trains, queues in order, things on a timetable."),
    ("bicycle", "🚲", "Something light: a minimal version, a lightweight approach."),
    ("fuel", "⛽", "Limits: usage limits, quotas, running out, credits."),
    ("battery", "🔋", "Power: battery life, energy use, resources left."),
    ("light-bulb", "💡", "Ideas: brainstorming, a proposal, a spec, product thinking."),
    ("flashlight", "🔦", "Finding hidden things: highlighting, spotlighting, discovery."),
    ("telescope", "🔭", "Looking far: long-term plans, roadmaps, distant systems."),
    ("magnet", "🧲", "Pulling in: fetching, snapping into place, sticky things."),
    ("dna", "🧬", "Structure: schemas, types, what makes something itself."),
    ("atom", "⚛️", "React: components, state, atoms of a UI."),
    ("pill", "💊", "Remedies: patches, small cures, a quick fix that works."),
    ("syringe", "💉", "Injection: dependency injection, injected scripts, inserted content."),
    ("stethoscope", "🩺", "Diagnosis: a doctor command, figuring out what's wrong."),
    ("bandage", "🩹", "A patch: a workaround, healing a small break."),
    ("microbe", "🦠", "Contagion: malware, viruses, problems that spread."),
    ("bomb", "💣", "Something explosive: a risky change, a crash that takes everything down."),
    ("boom", "💥", "Collisions: conflicts, merge conflicts, crashes."),
    ("ghost", "👻", "Something missing: invisible UI, things not showing, hidden state."),
    ("alien", "👽", "Something foreign: third-party code, an unfamiliar codebase, odd behavior."),
    ("skull", "💀", "Dead things: dead code, killed processes."),
    ("zombie", "🧟", "Zombies: orphaned processes, stale sessions that won't die."),
    ("mask", "🎭", "Roles: modes, personas, switching between modes."),
    ("eyes", "👀", "Watching: monitoring, observing, reviewing, looking over work."),
    ("ear", "👂", "Listening: event listeners, feedback, hearing back."),
    ("raised-hand", "✋", "Holding on: pausing, waiting for permission, asking to wait."),
    ("waving-hand", "👋", "Greetings: welcome screens, hellos and goodbyes, onboarding."),
    ("thumbs-up", "👍", "Approval: approving, sign-off, reactions, likes."),
    ("muscle", "💪", "Strength: robustness, hardening, resilience."),
    ("party", "🎉", "Celebrating: a launch, completion, delight."),
    ("balloon", "🎈", "Something playful: an easter egg, a light touch."),
    ("smile", "😊", "Something friendly: a small delight, a casual chat."),
    ("thinking", "🤔", "Open questions: figuring out what to do, uncertainty."),
    ("question", "❓", "Questions: asking you something, help, FAQs."),
    ("warning", "⚠️", "Warnings: lint warnings, deprecations, caution."),
    ("hundred", "💯", "Completeness: full coverage, perfect scores, 100%."),
    ("new", "🆕", "Something new: new features, new versions, fresh starts."),
    ("up", "🆙", "Upgrades: version bumps, updates, raising levels."),
    ("zzz", "💤", "Sleep: snoozing, idle, sleeping processes, delays until later."),
    ("dizzy", "💫", "Spinning: spinners, loading states, confusion, rotation."),
    ("dash", "💨", "Rushing: running fast, quitting, exits."),
    ("hole", "🕳️", "Holes: gaps, missing data, rabbit holes."),
    ("infinity", "♾️", "Endless: infinite loops, infinite scroll, things that run forever."),
    ("crown", "👑", "The main one: the primary, root, admins, leaders."),
    ("gem", "💎", "Quality and Ruby: Ruby gems, polish, something precious."),
    ("coffee", "☕", "Coffee: Java, long sessions, a break while waiting."),
    ("pizza", "🍕", "Slices: splitting work into pieces, sharing out."),
    ("cookie", "🍪", "Cookies: browser cookies, sessions on the web, consent."),
    ("cake", "🎂", "Layers: layered architecture, birthdays, anniversaries."),
    ("pepper", "🌶️", "Spicy: hard or risky work, heat, intensity."),
    ("salt", "🧂", "Salt: hashing, cryptography, a pinch of something."),
    ("honey", "🍯", "Honey: honeypots, sweet spots, sticky state."),
    ("bee", "🐝", "Busy workers: a hive of jobs, many small tasks."),
    ("ant", "🐜", "Tiny workers: background workers, many small processes."),
    ("spider", "🕷️", "Crawling: web scraping, crawlers, spidering links."),
    ("web", "🕸️", "Webs: graphs, networks of dependencies, tangled links."),
    ("owl", "🦉", "Night owls and wisdom: late-night jobs, insight."),
    ("parrot", "🦜", "Repeating: echoes, mimicking, copied output, duplicates."),
    ("fox", "🦊", "Cleverness: a clever trick, browsers, Firefox."),
    ("cat", "🐱", "Cats: concatenation, curiosity, small pets."),
    ("dog", "🐶", "Watchdogs: guarding, fetching, loyal background jobs."),
    ("bird", "🐦", "Small messages: posts, tweets, notes that fly out."),
    ("butterfly", "🦋", "Transformation: rewrites, migrations, the butterfly effect."),
    ("frog", "🐸", "Leaps: jumping between screens, hopping from step to step."),
    ("shark", "🦈", "Lurking danger: threats, attackers, aggressive bots."),
    ("fish", "🐟", "Fishing: phishing, catching things, the fish shell."),
    ("shell", "🐚", "The shell: shell scripts, the command line, terminals."),
    ("paws", "🐾", "Tracks: tracing footsteps, telemetry, following a trail."),
    ("horse", "🐎", "Workhorses: reliable jobs, horsepower, racing."),
    ("lion", "🦁", "Bold moves: big decisions, brave changes, leadership."),
    ("ufo", "🛸", "The unexplained: mysterious behavior, unknown causes."),
    ("telephone", "☎️", "Calls: RPC, calling functions, phoning someone."),
    ("pager", "📟", "Paging: on-call, pagination, pages of results."),
    ("printer", "🖨️", "Printing: print layouts, output devices, printed reports."),
    ("tv", "📺", "Screens to watch: streaming video, dashboards on a wall."),
    ("radio", "📻", "Broadcasts: pub/sub, signals sent to many listeners."),
    ("watch", "⌚", "Wearables: watches, watchOS, keeping time."),
    ("ruler", "📏", "Measuring: sizes, spacing, lengths, measurements."),
    ("set-square", "📐", "Layout and geometry: positions, alignment, rectangles, grids."),
    ("abc", "🔤", "Text and type: fonts, typography, characters, Unicode, spelling."),
    ("numbers", "🔢", "Numbers: numeric formats, IDs, versions, counting up."),
    ("speaking-head", "🗣️", "Languages: translation, localization, speech."),
];

/// How a picture is drawn when `[emblems] prompt` says nothing. Settings shows it in the prompt's
/// box until you write your own (SettingsDialog.tsx `DRAWN_STYLE`).
pub const DRAWN_STYLE: &str = "Flat colors and a bold, simple shape with a dark outline, centered on a plain white background.";

/// How many pixels a side a kept picture is: twice the 20 points of a session header's emblem,
/// for a Retina screen, with room to spare.
const PICTURE_PX: usize = 64;

/// How likely, next to the likeliest, an emoji must be for a session to get it in place of one
/// a session beside it shows: below this, the two share one rather than one getting an emoji
/// that doesn't fit.
const NEARLY: f64 = 0.25;

/// Sessions quiet for longer than this aren't given one or counted among a project's current
/// ones: turning emblems on shouldn't ask about every session of the past two weeks.
const RECENT_MS: i64 = 2 * DAY_MS;
/// Emblems of sessions not seen for this long are dropped from emblems.json.
const KEEP_MS: i64 = 30 * DAY_MS;
/// How long a session whose emblem failed waits before it's tried again, and how long no
/// drawing starts after one failed: what failed it (an account signed out, out of usage)
/// fails the next one too.
const RETRY_MS: i64 = 5 * 60_000;

/// What the session is about, for Jev or the account that draws: the first prompt you gave it,
/// else the latest where its harness keeps only that (OpenCode). None while it has been sent
/// nothing: its title then is its harness's word for an empty one ("Codex thread"), which an
/// emblem kept for its life mustn't be picked from.
fn about(s: &Session) -> Option<&str> {
    [s.first_prompt.as_deref(), s.last_prompt.as_deref()].into_iter().flatten().map(str::trim).find(|p| !p.is_empty())
}

/// Whether the session counts among those it's set apart from: not archived and active lately,
/// or running.
fn current(s: &Session, now: i64) -> bool {
    !s.archived && (s.live || now - s.last_activity < RECENT_MS)
}

/// Whether the session gets an emblem of its own: not a run nobody is at, nor a helper, which
/// shows the emblem of the session it works for.
fn has_own(s: &Session) -> bool {
    !s.headless && s.helper.as_ref().and_then(|h| h.parent.as_ref()).is_none()
}

/// A session's color as its harness names it (`Session::color`): one of the colors Claude Code's
/// `/color` takes, which a picture drawn for it is mostly in. None for its default or one it
/// doesn't take.
pub fn harness_color(name: &str) -> Option<String> {
    let name = name.trim().to_lowercase();
    ["red", "blue", "green", "yellow", "purple", "orange", "pink", "cyan"].contains(&name.as_str()).then_some(name)
}

/// The emoji by its name in `EMOJI`.
fn emoji_named(name: &str) -> Option<&'static (&'static str, &'static str, &'static str)> {
    EMOJI.iter().find(|e| e.0 == name)
}

/// Jev's question: which emoji stands for what the session works on.
pub fn emoji_request(s: &Session, model: &str) -> Value {
    let mut state = Map::new();
    state.insert("session title".into(), json!(jev::clip(&s.title, 200)));
    if let Some(p) = about(s) {
        let said = if s.first_prompt.as_deref().is_some_and(|f| f.trim() == p) { "first prompt" } else { "latest prompt" };
        state.insert(said.into(), json!(jev::clip(p, 600)));
    }
    jev::body(
        model,
        Value::Object(state),
        json!({
            "emoji": {
                "type": "choice",
                "instructions": "Which emoji best stands for what this coding session works on? Go by its subject, not by the kind of work every session does (writing code, reviewing it, committing). A playful match is good: a thing its subject brings to mind.",
                "criteria": Value::Object(EMOJI.iter().map(|(name, _, about)| (name.to_string(), json!(about))).collect()),
            },
        }),
    )
}

/// The emoji a session gets from Jev's ranking (`jev::choice`, the likeliest first): the
/// likeliest that no session beside it shows (`taken`, by name), among those nearly as likely as
/// the first; else the first. None when Jev named none of `EMOJI`.
pub fn pick_emoji(ranking: &[(String, f64)], taken: &[String]) -> Option<&'static str> {
    let known: Vec<(&'static str, f64)> = ranking.iter().filter_map(|(name, p)| emoji_named(name).map(|e| (e.0, *p))).collect();
    let top = known.first()?.1;
    known.iter().find(|(name, p)| *p >= top * NEARLY && !taken.iter().any(|t| t == name)).or(known.first()).map(|(name, _)| *name)
}

/// What the account that draws is asked: one thing that stands for what the session works on,
/// not what the sessions beside it show (`taken`), mostly in the color its harness gives it,
/// drawn as `style` says (`DRAWN_STYLE` when it says nothing), and its name after.
pub fn draw_prompt(s: &Session, taken: &[String], style: &str) -> String {
    let mut p = String::from("Draw an emblem for a coding session with your image generation tool: one picture. Use the tool directly; don't read any files or skills, and don't write code.\n\n");
    p.push_str(&format!("The session is called “{}”.", jev::clip(&s.title, 200)));
    if let Some(asked) = about(s).filter(|a| a.trim() != s.title.trim()) {
        p.push_str(&format!(" It was asked: “{}”", jev::clip(asked, 600)));
    }
    p.push_str("\n\nPick one thing you can name in a word or two (an object, an animal, a plant, a place) that stands for what it works on; a pun is welcome. Draw only that thing, big, filling most of a square picture. It's shown at 16 to 20 pixels, so: one bold shape, no text or letters, no fine detail.");
    if !taken.is_empty() {
        p.push_str(&format!(" The sessions beside it show {}, so pick something else.", crate::util::listed(taken, "and")));
    }
    if let Some(color) = s.color.as_deref() {
        p.push_str(&format!(" Make it mostly {color}."));
    }
    let style = if style.trim().is_empty() { DRAWN_STYLE } else { style.trim() };
    p.push_str(&format!("\n\nHow to draw it: {style}\n\nOnce it's drawn, answer with only the thing's name, in a word or two."));
    p
}

/// Has `account` draw what `prompt` asks, in `cwd`: the kept picture's name (`<hash>.png`) and
/// what it shows, as the account said.
pub fn draw(account: &AccountCfg, cwd: &Path, prompt: &str) -> Result<(String, String)> {
    let adapter = crate::harness::of(account).ok_or_else(|| anyhow!("{} isn't an account Codealong knows how to run.", account.name))?;
    std::fs::create_dir_all(cwd).with_context(|| format!("making {}", cwd.display()))?;
    let drawn = adapter.draw(&crate::harness::Ctx { account }, cwd, prompt)?;
    let small = shrink(&drawn.image)?;
    let kept = crate::attachments::keep_image_in(&crate::attachments::images_dir(), &small).ok_or_else(|| anyhow!("Codealong couldn't keep the picture {} drew.", account.name))?;
    let picture = kept.strip_prefix(crate::attachments::IMAGES).unwrap_or(&kept).to_string();
    Ok((picture, name_of(&drawn.said)))
}

/// Where the account that draws runs: a folder of Codealong's own, with nothing in it to read.
pub fn drawing_dir() -> PathBuf {
    crate::config::support_dir().join("emblem-drawing")
}

/// The thing's name from what the account answered after drawing: its last line, without
/// quotes, marks or a full stop.
fn name_of(said: &str) -> String {
    let line = said.lines().map(str::trim).rfind(|l| !l.is_empty()).unwrap_or("");
    let name = line.trim_matches(|c: char| "\"'`*_.“”‘’".contains(c)).trim();
    if name.is_empty() {
        "picture".to_string()
    } else {
        jev::clip(name, 40)
    }
}

/* ---- Pictures ---- */

/// A drawn picture made an emblem: a plain white background around the thing made clear, cut
/// to a square around what's left with a little room, scaled down to `PICTURE_PX` by averaging,
/// and written as a PNG.
pub fn shrink(png_bytes: &[u8]) -> Result<Vec<u8>> {
    let (w, h, mut px) = decode(png_bytes)?;
    clear_white(&mut px, w, h);
    let (x0, y0, side) = square_around(&px, w, h);
    encode(&scale_down(&px, w, h, (x0, y0, side), PICTURE_PX), PICTURE_PX)
}

/// A PNG's pixels as RGBA, 8 bits a channel.
fn decode(bytes: &[u8]) -> Result<(usize, usize, Vec<u8>)> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().context("The drawing isn't a PNG Codealong can read")?;
    let mut buf = vec![0; reader.output_buffer_size().ok_or_else(|| anyhow!("The drawing is too big to read."))?];
    let info = reader.next_frame(&mut buf).context("The drawing isn't a PNG Codealong can read")?;
    let (w, h) = (info.width as usize, info.height as usize);
    let data = &buf[..info.buffer_size()];
    let rgba: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => data.to_vec(),
        png::ColorType::Rgb => data.chunks_exact(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => data.chunks_exact(2).flat_map(|p| [p[0], p[0], p[0], p[1]]).collect(),
        png::ColorType::Grayscale => data.iter().flat_map(|g| [*g, *g, *g, 255]).collect(),
        png::ColorType::Indexed => bail!("The drawing's colors didn't read."),
    };
    if w == 0 || h == 0 || rgba.len() != w * h * 4 {
        bail!("The drawing is empty.");
    }
    Ok((w, h, rgba))
}

/// Whether a pixel is the plain white a picture is drawn on, or already clear.
fn background(p: &[u8]) -> bool {
    p[3] < 16 || (p[0] >= 230 && p[1] >= 230 && p[2] >= 230)
}

/// Makes the white a picture is drawn on clear, so it sits on any window: every pixel of it
/// that reaches the edge, when most of the edge is white (a picture that fills its square keeps
/// it). The thing's own light edge, where it was blended into the white, is made partly clear
/// and unblended, so no white rim shows on a dark window.
fn clear_white(px: &mut [u8], w: usize, h: usize) {
    let at = |x: usize, y: usize| (y * w + x) * 4;
    let edge: Vec<(usize, usize)> = (0..w).flat_map(|x| [(x, 0), (x, h - 1)]).chain((0..h).flat_map(|y| [(0, y), (w - 1, y)])).collect();
    if edge.iter().filter(|(x, y)| background(&px[at(*x, *y)..at(*x, *y) + 4])).count() * 2 < edge.len() {
        return;
    }
    let mut clear = vec![false; w * h];
    let mut stack: Vec<(usize, usize)> = edge.into_iter().filter(|(x, y)| background(&px[at(*x, *y)..at(*x, *y) + 4])).collect();
    while let Some((x, y)) = stack.pop() {
        if clear[y * w + x] {
            continue;
        }
        clear[y * w + x] = true;
        let next = [(x.wrapping_sub(1), y), (x + 1, y), (x, y.wrapping_sub(1)), (x, y + 1)];
        for (nx, ny) in next {
            if nx < w && ny < h && !clear[ny * w + nx] && background(&px[at(nx, ny)..at(nx, ny) + 4]) {
                stack.push((nx, ny));
            }
        }
    }
    // Lighter than this, a pixel beside the background is taken as partly white.
    const SOFT: f64 = 170.0;
    for y in 0..h {
        for x in 0..w {
            let i = at(x, y);
            if clear[y * w + x] {
                px[i..i + 4].copy_from_slice(&[0, 0, 0, 0]);
                continue;
            }
            let beside = [(x.wrapping_sub(1), y), (x + 1, y), (x, y.wrapping_sub(1)), (x, y + 1)].iter().any(|(nx, ny)| *nx < w && *ny < h && clear[ny * w + nx]);
            let light = f64::from(px[i].min(px[i + 1]).min(px[i + 2]));
            if beside && light > SOFT {
                let a = ((255.0 - light) / (255.0 - SOFT)).clamp(0.05, 1.0);
                for c in 0..3 {
                    px[i + c] = ((f64::from(px[i + c]) - 255.0 * (1.0 - a)) / a).clamp(0.0, 255.0) as u8;
                }
                px[i + 3] = (f64::from(px[i + 3]) * a) as u8;
            }
        }
    }
}

/// The square a picture is cut to: around what isn't clear, with a twentieth of room on each
/// side, centered on it; the whole picture's when none of it, or all of it, is clear. It can
/// reach past the picture's edges, which are clear there.
fn square_around(px: &[u8], w: usize, h: usize) -> (i64, i64, i64) {
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
    for y in 0..h {
        for x in 0..w {
            if px[(y * w + x) * 4 + 3] > 24 {
                (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x + 1), y1.max(y + 1));
            }
        }
    }
    if x1 <= x0 || y1 <= y0 || (x0, y0, x1, y1) == (0, 0, w, h) {
        let side = w.max(h) as i64;
        return ((w as i64 - side) / 2, (h as i64 - side) / 2, side);
    }
    let (bw, bh) = ((x1 - x0) as i64, (y1 - y0) as i64);
    let side = (bw.max(bh) as f64 * 1.1).ceil() as i64;
    let (cx, cy) = (x0 as i64 + bw / 2, y0 as i64 + bh / 2);
    (cx - side / 2, cy - side / 2, side.max(1))
}

/// The square `at` (left, top, side) scaled to `n` pixels a side, each the average of the
/// pixels it covers, weighted by how clear they are so a clear pixel's color doesn't darken the
/// edge. Past the picture's edges counts as clear.
fn scale_down(px: &[u8], w: usize, h: usize, at: (i64, i64, i64), n: usize) -> Vec<u8> {
    let (left, top, side) = at;
    let span = |o: usize, from: i64| (from + (o as i64 * side) / n as i64, from + ((o as i64 + 1) * side) / n as i64);
    let mut out = vec![0u8; n * n * 4];
    for oy in 0..n {
        let (sy0, sy1) = span(oy, top);
        for ox in 0..n {
            let (sx0, sx1) = span(ox, left);
            let (mut rgb, mut alpha, mut count) = ([0f64; 3], 0f64, 0f64);
            for sy in sy0..sy1.max(sy0 + 1) {
                for sx in sx0..sx1.max(sx0 + 1) {
                    count += 1.0;
                    if sx < 0 || sy < 0 || sx as usize >= w || sy as usize >= h {
                        continue;
                    }
                    let i = (sy as usize * w + sx as usize) * 4;
                    let a = f64::from(px[i + 3]) / 255.0;
                    for c in 0..3 {
                        rgb[c] += f64::from(px[i + c]) * a;
                    }
                    alpha += a;
                }
            }
            let o = (oy * n + ox) * 4;
            if alpha > 0.0 {
                for c in 0..3 {
                    out[o + c] = (rgb[c] / alpha).round().clamp(0.0, 255.0) as u8;
                }
            }
            out[o + 3] = (alpha / count * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }
    out
}

/// RGBA pixels, `n` a side, as a PNG.
fn encode(px: &[u8], n: usize) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, n as u32, n as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header()?;
        writer.write_image_data(px)?;
        writer.finish()?;
    }
    Ok(out)
}

/* ---- Kept emblems ---- */

/// What a session was given. Emblems kept before 2026-10-27, icons on colors, read as neither
/// and are given again.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Kept {
    /// The emoji Jev picked, by its name in `EMOJI`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emoji: Option<String>,
    /// The picture drawn for it: a kept image's name, `<hash>.png`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub picture: Option<String>,
    /// What the picture shows, as the account that drew it said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drawn: Option<String>,
    /// When the session was last seen, to drop old ones.
    pub seen_at: i64,
}

impl Kept {
    /// What it shows when emblems are `kind`: its emoji, or its picture, with its emoji where it
    /// has one for the windows to show when the picture doesn't load, else its emoji until it has
    /// a picture.
    fn shown(&self, kind: EmblemKind) -> Option<Emblem> {
        let emoji = || self.emoji.as_deref().and_then(emoji_named).map(|(name, emoji, _)| Emblem { emoji: Some(emoji.to_string()), picture: None, name: name.replace('-', " ") });
        match kind {
            EmblemKind::Off => None,
            EmblemKind::Emoji => emoji(),
            EmblemKind::Drawn => match &self.picture {
                Some(p) => Some(Emblem { emoji: emoji().and_then(|e| e.emoji), picture: Some(p.clone()), name: self.drawn.clone().unwrap_or_default() }),
                None => emoji(),
            },
        }
    }

    /// Whether it has what emblems of `kind` show.
    fn has(&self, kind: EmblemKind) -> bool {
        match kind {
            EmblemKind::Off => true,
            EmblemKind::Emoji => self.emoji.is_some(),
            EmblemKind::Drawn => self.picture.is_some(),
        }
    }
}

/// Emblems by session key, as emblems.json keeps them.
#[derive(Default)]
pub struct Store {
    pub kept: HashMap<String, Kept>,
    /// Each current session's project, as the last look found them: what the sessions beside
    /// one show is asked of these.
    projects: HashMap<String, String>,
    /// Sessions whose emblem failed, and when: tried again only after a while.
    failed: HashMap<String, i64>,
    /// The session whose picture is being drawn.
    pub drawing: Option<String>,
    /// When a drawing last failed.
    draw_failed: Option<i64>,
    /// emblems.json was there and didn't read, so what's kept here isn't all it holds.
    unread: bool,
    dirty: bool,
}

fn path() -> PathBuf {
    crate::config::support_dir().join("emblems.json")
}

impl Store {
    pub fn load() -> Store {
        // A file that doesn't read is left as it is, and a notice says so: sessions are
        // remembered by how they look, so their emblems aren't given out again over it.
        let kept = crate::store::load_whole(&path(), "the sessions' emblems");
        Store { kept, unread: crate::store::unread(&path()), ..Default::default() }
    }

    /// What the session shows when emblems are `kind`: its own, or a helper's, the emblem of the
    /// session it works for. None until it has one.
    pub fn emblem_of(&self, s: &Session, kind: EmblemKind) -> Option<Emblem> {
        let parent = s.helper.as_ref().and_then(|h| h.parent.as_deref());
        self.kept.get(parent.unwrap_or(&s.key))?.shown(kind)
    }

    /// What the other current sessions of the session's project show, by name.
    pub fn taken(&self, key: &str, kind: EmblemKind) -> Vec<String> {
        let Some(project) = self.projects.get(key) else { return Vec::new() };
        let mut names: Vec<String> = self
            .projects
            .iter()
            .filter(|(k, p)| *p == project && k.as_str() != key)
            .filter_map(|(k, _)| self.kept.get(k))
            .filter_map(|m| match kind {
                EmblemKind::Emoji => m.emoji.clone(),
                EmblemKind::Drawn => m.drawn.clone(),
                EmblemKind::Off => None,
            })
            .collect();
        names.sort();
        names.dedup();
        names
    }

    /// Looks over the board's sessions, each with its project, and says which want an emblem of
    /// `kind`, by key, the newest first: current ones with a prompt and an emblem of their own,
    /// not given one yet, and not failed lately. Each session once, however many lanes list it.
    pub fn look<'a>(&mut self, sessions: impl IntoIterator<Item = (&'a str, &'a Session)>, kind: EmblemKind, now: i64) -> Vec<String> {
        let mut by_key: HashMap<&str, (&str, &Session)> = HashMap::new();
        for (project, s) in sessions {
            by_key.entry(s.key.as_str()).or_insert((project, s));
        }
        self.projects = by_key.iter().filter(|(_, (_, s))| current(s, now)).map(|(k, (p, _))| (k.to_string(), p.to_string())).collect();
        for k in by_key.keys() {
            self.seen(k, now);
        }
        let mut keys: Vec<&str> = by_key
            .iter()
            .filter(|(k, (_, s))| {
                current(s, now) && has_own(s) && about(s).is_some() && !self.kept.get(**k).is_some_and(|m| m.has(kind)) && self.failed.get(**k).is_none_or(|at| now - at > RETRY_MS)
            })
            .map(|(k, _)| *k)
            .collect();
        keys.sort_by_key(|k| (std::cmp::Reverse(by_key[k].1.last_prompt_at.max(by_key[k].1.started_at.unwrap_or(0))), *k));
        keys.into_iter().map(String::from).collect()
    }

    /// Jev's ranking for a session's emoji: it gets the one `pick_emoji` picks. False when Jev
    /// named none it knows.
    pub fn answered_emoji(&mut self, key: &str, ranking: &[(String, f64)], now: i64) -> bool {
        let Some(name) = pick_emoji(ranking, &self.taken(key, EmblemKind::Emoji)) else {
            self.failed(key, now);
            return false;
        };
        self.failed.remove(key);
        let m = self.kept.entry(key.to_string()).or_default();
        m.emoji = Some(name.to_string());
        m.seen_at = now;
        self.dirty = true;
        true
    }

    /// A session's picture is drawn: the kept image's name, and what it shows.
    pub fn drawn(&mut self, key: &str, picture: String, name: String, now: i64) {
        self.drawing = None;
        self.draw_failed = None;
        self.failed.remove(key);
        let m = self.kept.entry(key.to_string()).or_default();
        m.picture = Some(picture);
        m.drawn = Some(name);
        m.seen_at = now;
        self.dirty = true;
    }

    pub fn failed(&mut self, key: &str, now: i64) {
        self.failed.insert(key.to_string(), now);
    }

    /// A session's picture couldn't be drawn: neither it nor another is drawn for a while.
    pub fn draw_failed(&mut self, key: &str, now: i64) {
        self.drawing = None;
        self.draw_failed = Some(now);
        self.failed(key, now);
    }

    /// Whether a drawing can start: none is under way, and none failed lately.
    pub fn can_draw(&self, now: i64) -> bool {
        self.drawing.is_none() && self.draw_failed.is_none_or(|at| now - at > RETRY_MS)
    }

    /// The kept pictures, as text refers to kept images (`images/<name>`), so they aren't
    /// cleared as unused (`attachments::collect_garbage`). None while emblems.json is one that
    /// didn't read, whose pictures can't be told from unused ones.
    pub fn pictures(&self) -> Option<Vec<String>> {
        (!self.unread).then(|| self.kept.values().filter_map(|m| m.picture.as_ref()).map(|p| format!("{}{p}", crate::attachments::IMAGES)).collect())
    }

    /// Notes that a session with an emblem is still around.
    fn seen(&mut self, key: &str, now: i64) {
        if let Some(m) = self.kept.get_mut(key) {
            // Written at most once a day per session: this runs on every publish.
            if now - m.seen_at > DAY_MS {
                m.seen_at = now;
                self.dirty = true;
            }
        }
    }

    /// Writes emblems.json when something changed, dropping emblems of sessions not seen for a
    /// month and those from before that show nothing.
    pub fn save(&mut self, now: i64) {
        if !self.dirty {
            return;
        }
        self.kept.retain(|_, m| now - m.seen_at < KEEP_MS && (m.emoji.is_some() || m.picture.is_some()));
        self.dirty = false;
        // Not over a file that didn't read: save_whole leaves it and its notice says why.
        let _ = crate::store::save_whole(&path(), &self.kept);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Harness, Helper};

    const NOW: i64 = 40 * DAY_MS;

    fn session(id: &str, prompt: &str) -> Session {
        Session {
            title: prompt.into(),
            first_prompt: Some(prompt.into()),
            started_at: Some(NOW - 3_600_000),
            last_activity: NOW - 60_000,
            last_prompt_at: NOW - 600_000,
            ..Session::new(Harness::Claude, "claude", id)
        }
    }

    fn ranked(names: &[(&str, f64)]) -> Vec<(String, f64)> {
        names.iter().map(|(n, p)| (n.to_string(), *p)).collect()
    }

    /// Every emoji has a name of its own, an emoji and what it stands for, and Jev's limit of
    /// 255 options holds.
    #[test]
    fn the_emoji_are_named_once_each() {
        let names: std::collections::HashSet<&str> = EMOJI.iter().map(|e| e.0).collect();
        assert_eq!(names.len(), EMOJI.len(), "an emoji named twice");
        let emoji: std::collections::HashSet<&str> = EMOJI.iter().map(|e| e.1).collect();
        assert_eq!(emoji.len(), EMOJI.len(), "an emoji listed twice");
        assert!(EMOJI.len() <= 255);
        assert!(EMOJI.iter().all(|(n, e, about)| !n.is_empty() && !e.is_empty() && about.ends_with('.')));
    }

    /// The likeliest emoji no session beside it shows, among those nearly as likely; else the
    /// likeliest, rather than one that doesn't fit.
    #[test]
    fn picks_the_likeliest_emoji_nobody_beside_it_shows() {
        let ranking = ranked(&[("bug", 0.6), ("ladybug", 0.3), ("fire", 0.05)]);
        assert_eq!(pick_emoji(&ranking, &[]), Some("bug"));
        assert_eq!(pick_emoji(&ranking, &["bug".into()]), Some("ladybug"));
        assert_eq!(pick_emoji(&ranking, &["bug".into(), "ladybug".into()]), Some("bug"), "fire is too unlikely to stand in");
        assert_eq!(pick_emoji(&ranked(&[("not-an-emoji", 0.9), ("crab", 0.1)]), &[]), Some("crab"));
        assert_eq!(pick_emoji(&ranked(&[("not-an-emoji", 0.9)]), &[]), None);
    }

    #[test]
    fn asks_one_choice_over_the_emoji() {
        let s = session("a", "Make the settings dialog look right in dark mode");
        let body = emoji_request(&s, "jev-latest");
        assert_eq!(body["state"]["first prompt"], "Make the settings dialog look right in dark mode");
        assert_eq!(body["questions"]["emoji"]["type"], "choice");
        assert_eq!(body["questions"]["emoji"]["criteria"].as_object().unwrap().len(), EMOJI.len());
        // Where a harness keeps only the latest prompt (OpenCode), that's what it goes by.
        let latest = Session { first_prompt: None, last_prompt: Some("Speed up the search index".into()), ..session("oc", "x") };
        let body = emoji_request(&latest, "m");
        assert_eq!(body["state"]["latest prompt"], "Speed up the search index");
        assert!(body["state"].get("first prompt").is_none());
    }

    /// Current sessions with a prompt and an emblem of their own, newest first; not ones quiet
    /// for days, archived, sent nothing yet, run by nobody, or helping another session.
    #[test]
    fn looks_for_the_sessions_that_want_one() {
        let mut store = Store::default();
        let a = session("a", "Fix the flaky login test");
        let b = Session { last_prompt_at: NOW - 60_000, ..session("b", "Dark mode for the settings") };
        let old = Session { last_activity: NOW - 3 * DAY_MS, ..session("old", "Old work") };
        let archived = Session { archived: true, ..session("arch", "Archived work") };
        let empty = Session { first_prompt: None, title: "Codex thread".into(), ..session("empty12", "") };
        let headless = Session { headless: true, ..session("run", "Summarize the logs") };
        let helper = Session { helper: Some(Helper { parent: Some(a.key.clone()), ..Default::default() }), ..session("help", "Review the login fix") };
        let review = Session { helper: Some(Helper { pr: Some("acme/app#7".into()), ..Default::default() }), ..session("rev", "Review PR 7") };
        let board = [("A", &a), ("A", &b), ("A", &old), ("A", &archived), ("A", &empty), ("A", &headless), ("A", &helper), ("A", &review), ("B", &a)];
        assert_eq!(store.look(board, EmblemKind::Emoji, NOW), vec![b.key.clone(), a.key.clone(), review.key.clone()]);
        // Given one, a session isn't asked about again; another kind asks again.
        assert!(store.answered_emoji(&a.key, &ranked(&[("bug", 0.9)]), NOW));
        assert_eq!(store.look(board, EmblemKind::Emoji, NOW), vec![b.key.clone(), review.key.clone()]);
        assert_eq!(store.look(board, EmblemKind::Drawn, NOW), vec![b.key.clone(), a.key.clone(), review.key.clone()]);
        // A failed one waits.
        store.failed(&b.key, NOW);
        assert_eq!(store.look(board, EmblemKind::Emoji, NOW + 60_000), vec![review.key.clone()]);
        assert_eq!(store.look(board, EmblemKind::Emoji, NOW + RETRY_MS + 1).len(), 2);
    }

    /// A session shows what it was given for the kind that's on, a drawn one its emoji until its
    /// picture comes, and a helper the emblem of the session it works for.
    #[test]
    fn shows_what_it_was_given() {
        let mut store = Store::default();
        let a = session("a", "Fix the flaky login test");
        let helper = Session { helper: Some(Helper { parent: Some(a.key.clone()), ..Default::default() }), ..session("help", "Review it") };
        store.look([("A", &a), ("A", &helper)], EmblemKind::Emoji, NOW);
        assert_eq!(store.emblem_of(&a, EmblemKind::Emoji), None);
        store.answered_emoji(&a.key, &ranked(&[("test-tube", 0.7)]), NOW);
        let emoji = Emblem { emoji: Some("🧪".into()), picture: None, name: "test tube".into() };
        assert_eq!(store.emblem_of(&a, EmblemKind::Emoji), Some(emoji.clone()));
        assert_eq!(store.emblem_of(&a, EmblemKind::Drawn), Some(emoji.clone()), "its emoji until it's drawn");
        assert_eq!(store.emblem_of(&helper, EmblemKind::Emoji), Some(emoji));
        assert_eq!(store.emblem_of(&a, EmblemKind::Off), None);
        store.drawing = Some(a.key.clone());
        store.drawn(&a.key, "0123456789abcdef.png".into(), "beaker".into(), NOW);
        // With its emoji, for when the picture doesn't load.
        let picture = Emblem { emoji: Some("🧪".into()), picture: Some("0123456789abcdef.png".into()), name: "beaker".into() };
        assert_eq!(store.emblem_of(&a, EmblemKind::Drawn), Some(picture.clone()));
        assert_eq!(store.emblem_of(&helper, EmblemKind::Drawn), Some(picture));
        assert_eq!(store.emblem_of(&a, EmblemKind::Emoji).and_then(|e| e.emoji).as_deref(), Some("🧪"));
        assert!(store.drawing.is_none());
        // One drawn with no emoji is the picture alone.
        let b = session("b", "Draw it");
        store.drawn(&b.key, "b.png".into(), "kite".into(), NOW);
        assert_eq!(store.emblem_of(&b, EmblemKind::Drawn), Some(Emblem { emoji: None, picture: Some("b.png".into()), name: "kite".into() }));
        let mut pictures = store.pictures().unwrap();
        pictures.sort();
        assert_eq!(pictures, vec!["images/0123456789abcdef.png".to_string(), "images/b.png".to_string()]);
        // From an emblems.json that didn't read, no picture can be told from an unused one.
        store.unread = true;
        assert_eq!(store.pictures(), None);
    }

    /// What the sessions beside one show is what its project's other current sessions show.
    #[test]
    fn sets_a_session_apart_from_its_projects() {
        let mut store = Store::default();
        let (a, b, c) = (session("a", "Fix the login"), session("b", "Fix the logout"), session("c", "Other work"));
        store.look([("A", &a), ("A", &b), ("B", &c)], EmblemKind::Emoji, NOW);
        store.answered_emoji(&a.key, &ranked(&[("bug", 0.6), ("ladybug", 0.4)]), NOW);
        store.answered_emoji(&c.key, &ranked(&[("ladybug", 0.9)]), NOW);
        assert_eq!(store.taken(&b.key, EmblemKind::Emoji), vec!["bug".to_string()]);
        store.answered_emoji(&b.key, &ranked(&[("bug", 0.6), ("ladybug", 0.4)]), NOW);
        assert_eq!(store.kept[&b.key].emoji.as_deref(), Some("ladybug"), "another project's ladybug doesn't count");
        // Drawn ones go by what each picture shows.
        store.drawn(&a.key, "a.png".into(), "padlock".into(), NOW);
        assert_eq!(store.taken(&b.key, EmblemKind::Drawn), vec!["padlock".to_string()]);
    }

    /// One drawing at a time, and none for a while after one failed.
    #[test]
    fn draws_one_at_a_time_and_waits_after_a_failure() {
        let mut store = Store::default();
        assert!(store.can_draw(NOW));
        store.drawing = Some("a".into());
        assert!(!store.can_draw(NOW));
        store.draw_failed("a", NOW);
        assert!(!store.can_draw(NOW + 60_000));
        assert!(store.can_draw(NOW + RETRY_MS + 1));
    }

    #[test]
    fn asks_for_one_thing_set_apart_from_the_sessions_beside_it() {
        let s = Session { color: Some("cyan".into()), ..session("a", "Make the threads view faster") };
        let p = draw_prompt(&s, &["dinosaur".into(), "spool".into()], "");
        assert!(p.contains("“Make the threads view faster”"), "{p}");
        assert!(!p.contains("It was asked"), "the prompt is its title: said once");
        assert!(p.contains("show dinosaur and spool, so pick something else"), "{p}");
        assert!(p.contains("Make it mostly cyan."), "{p}");
        assert!(p.contains(&format!("How to draw it: {DRAWN_STYLE}")), "{p}");
        let p = draw_prompt(&Session { title: "threads view".into(), color: None, ..s }, &[], "  Pixel art on a 16 by 16 grid.  ");
        assert!(p.contains("It was asked: “Make the threads view faster”") && p.contains("How to draw it: Pixel art on a 16 by 16 grid.\n"), "{p}");
        assert!(!p.contains("beside it show") && !p.contains("mostly"), "{p}");
    }

    #[test]
    fn names_the_thing_from_the_answer() {
        assert_eq!(name_of("Cactus"), "Cactus");
        assert_eq!(name_of("I drew it.\n\n**“Spool of thread.”**\n"), "Spool of thread");
        assert_eq!(name_of("  "), "picture");
    }

    /// A test picture: `w` by `h` white, with a red square from (x0, y0) to (x1, y1).
    fn picture(w: u32, h: u32, square: (u32, u32, u32, u32)) -> Vec<u8> {
        let (x0, y0, x1, y1) = square;
        let px: Vec<u8> = (0..h).flat_map(|y| (0..w).flat_map(move |x| if x >= x0 && x < x1 && y >= y0 && y < y1 { [200, 20, 20] } else { [255, 255, 255] })).collect();
        let mut out = Vec::new();
        let mut e = png::Encoder::new(&mut out, w, h);
        e.set_color(png::ColorType::Rgb);
        e.set_depth(png::BitDepth::Eight);
        let mut writer = e.write_header().unwrap();
        writer.write_image_data(&px).unwrap();
        writer.finish().unwrap();
        out
    }

    /// The white around the thing goes clear, the picture is cut to the thing and made small.
    #[test]
    fn shrinks_a_drawing_to_the_thing_on_clear() {
        let small = shrink(&picture(400, 300, (100, 50, 300, 250))).unwrap();
        let (w, h, px) = decode(&small).unwrap();
        assert_eq!((w, h), (PICTURE_PX, PICTURE_PX));
        let at = |x: usize, y: usize| &px[(y * w + x) * 4..(y * w + x) * 4 + 4];
        assert_eq!(at(0, 0)[3], 0, "the corner is clear");
        assert_eq!(at(PICTURE_PX / 2, PICTURE_PX / 2), &[200, 20, 20, 255], "the middle is the thing");
        // The thing fills the square but for the room left round it.
        let opaque = (0..PICTURE_PX).filter(|x| at(*x, PICTURE_PX / 2)[3] == 255).count();
        assert!((52..=60).contains(&opaque), "{opaque} of {PICTURE_PX} across");
        // A picture whose edge is mostly drawn on keeps its background, and its whole square.
        let full = shrink(&picture(100, 100, (0, 0, 100, 70))).unwrap();
        let (_, _, px) = decode(&full).unwrap();
        assert_eq!(&px[px.len() - 4..], &[255, 255, 255, 255]);
        assert!(shrink(b"not a png").is_err());
    }

    /// emblems.json from before, icons on colors, reads: as nothing to show, given again.
    #[test]
    fn reads_emblems_kept_before() {
        let kept: HashMap<String, Kept> = serde_json::from_str(r#"{"k":{"color":"teal","accent":"blue","light":3,"palette":2,"icon":"bug","seenAt":1}}"#).unwrap();
        assert_eq!(kept["k"], Kept { seen_at: 1, ..Default::default() });
        assert_eq!(kept["k"].shown(EmblemKind::Emoji), None);
    }

    /// Draws one emblem with this Mac's Codex account (`~/.codex`), for real: it spends an image
    /// turn of that account's usage, so it runs only when asked
    /// (`cargo test draws_with_codex -- --ignored --nocapture`), and leaves the emblem in the
    /// temp folder to look at.
    #[test]
    #[ignore]
    fn draws_with_codex() {
        let account = AccountCfg { name: "codex".into(), harness: "codex".into(), home: "~/.codex".into(), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false };
        let s = Session { first_prompt: Some("Add 30 minutes and next Monday to the snooze menu".into()), ..session("x", "Snooze duration options") };
        let cwd = std::env::temp_dir().join("codealong-emblem-drawing");
        std::fs::create_dir_all(&cwd).unwrap();
        let started = std::time::Instant::now();
        let drawn = crate::harness::of(&account).unwrap().draw(&crate::harness::Ctx { account: &account }, &cwd, &draw_prompt(&s, &["dinosaur".into()], "")).unwrap();
        let out = cwd.join("emblem.png");
        std::fs::write(&out, shrink(&drawn.image).unwrap()).unwrap();
        println!("Drew “{}” in {:?}: {}", name_of(&drawn.said), started.elapsed(), out.display());
    }

    /// Settings shows the style a picture is drawn in when you've written none.
    #[test]
    fn settings_shows_the_style_it_draws_in() {
        let ui = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../src/ui/SettingsDialog.tsx")).unwrap();
        assert!(ui.contains(&format!("const DRAWN_STYLE = '{DRAWN_STYLE}';")), "SettingsDialog.tsx's DRAWN_STYLE differs from emblems.rs's");
    }
}
