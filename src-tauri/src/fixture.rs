//! A made-up Mac, for the browser checks: `codealong --snapshot --fixture …` prints what the
//! snapshot modes print (the board, the bar's feed, the accounts, the start options, the
//! settings, the skills, the editors' icons, a shell's commands, a session's conversation and
//! what it waits on), made up here instead of read from this Mac. `PREVIEW_FIXTURE=1
//! scripts/preview.sh` and `scripts/checks.sh --fixture` render and check against it, so a check
//! finds the same sessions, PRs and todos on every Mac and every day, and a failure is the
//! code's.
//!
//! It's built the way the tests' fixtures are (peer/src/fixtures.rs, board.rs `rule_cases`):
//! sessions from `Session::new`, worktrees composed by `board::compose` from seeds, the settings
//! parsed from a config.toml (`CONFIG`), todos read as todos.json has them. So a field added to
//! the model gets its neutral value here with no line to write, and what a rule makes of a
//! session (its worktree's bay, its sentence, why it needs you) is the rule's own. What a Mac
//! would be asked for (which apps are installed, where an account can start, its models) is
//! said here as a Mac with everything installed would have it.
//!
//! Times count back from now, so nothing ages. Everything is made up: the projects (Northwind,
//! Atlas, Kestrel), their repos and people, the folders under /Users/me. Nothing here may name
//! a real project, repo, account or person.
//!
//! And nothing of the machine it's made on may be in it, or it isn't the same Mac everywhere:
//! not its home folder, its temporary folder, its sounds or the day on its clock. Only the
//! time it's given, and the time zone that time's day is counted in, which the page that shows
//! it counts by too. A rule of the engine's that reads any of those is given the made-up Mac's
//! in its place, or its answer is said here (`repo`, `board`, `config`); a test holds that.
//!
//! Only debug builds have it (`scripts/engine.sh` builds one): the app people install doesn't.
//! And it's a Mac's, with a Mac's terminals in its settings, so only macOS prints it; its tests
//! run as if on macOS wherever they run.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde_json::json;

use crate::board::{self, Inputs, LaneSeed, PrLinks, Previews};
use crate::config::{self, Config, ConfigView, Profile};
use crate::github::{ChangesAsked, PrRaw};
use crate::harness::{Pending, Question};
use crate::model::*;
use crate::start::{ModelOption, StartAccount, StartOptions};
use crate::todos::Todo;
use crate::transcript::{self, Page};

/// What the made-up Mac is called: `--snapshot --fixture name` prints it, and `scripts/preview.sh`
/// takes an engine for one that has a made-up Mac only when it answers with it.
pub const NAME: &str = "the made-up Mac";

const MIN: i64 = 60_000;
const HOUR: i64 = 60 * MIN;
const DAY: i64 = 24 * HOUR;

/// Who `gh` is signed in as, and the two people whose PRs ask for a review.
const YOU: &str = "sam-northwind";
const DANA: &str = "dana-kestrel";
const LEE: &str = "lee-atlas";

/// The made-up Mac's config.toml, as Settings shows it.
const CONFIG: &str = r#"# Codealong's settings on a made-up Mac (src-tauri/src/fixture.rs).
scan = ["~/projects"]

[apps]
terminal = "ghostty"
editor = "cursor"

[[accounts]]
name = "claude"
harness = "claude"
home = "~/.claude"
command = "claude"
monthly = 200

[[accounts]]
name = "claude2"
harness = "claude"
home = "~/.claude2"
command = "claude2"

[[accounts]]
name = "codex"
harness = "codex"
home = "~/.codex"
command = "codex"

[[accounts]]
name = "opencode"
harness = "opencode"
home = "~/.local/share/opencode"

[[accounts]]
name = "cursor"
harness = "cursor"
home = "~/.cursor"

[[accounts]]
name = "antigravity"
harness = "antigravity"
home = "~/.gemini/antigravity"

[[projects]]
name = "Northwind"
repos = ["github.com/northwind/*"]

[[projects]]
name = "Atlas"
repos = ["github.com/atlas-labs/*"]

[[projects]]
name = "Kestrel"
repos = ["github.com/kestrel-hq/*"]
"#;

/// The made-up Mac's home folder, which the engine's rules for a folder's name go by.
const HOME: &str = "/Users/me";

// The worktrees, by folder. A repo's main checkout, then the worktrees beside it.
const APP: &str = "/Users/me/projects/northwind-app";
const CHECKOUT: &str = "/Users/me/projects/northwind-app-checkout";
const SEARCH: &str = "/Users/me/projects/northwind-app-search";
const INVOICES: &str = "/Users/me/projects/northwind-app-invoices";
const ONBOARDING: &str = "/Users/me/projects/northwind-app-onboarding";
const IMPORTS: &str = "/Users/me/projects/northwind-app/.claude/worktrees/tidy-imports";
const BILLING: &str = "/Users/me/projects/northwind-app-billing";
const LIMITS: &str = "/Users/me/projects/northwind-app-rate-limits";
const FLAGS: &str = "/Users/me/projects/northwind-app-flags";
const NAV: &str = "/Users/me/projects/northwind-app-old-nav";
const LEGACY: &str = "/Users/me/projects/northwind-app-legacy";
const SPARE: &str = "/Users/me/projects/northwind-app-spare";
const API: &str = "/Users/me/projects/northwind-api";
const WEBHOOKS: &str = "/Users/me/projects/northwind-api-webhooks";
const ATLAS: &str = "/Users/me/projects/atlas-core";
const IMPORTER: &str = "/Users/me/projects/atlas-core-importer";
const DATES: &str = "/Users/me/.codex/worktrees/4f2a/atlas-core";
const SITE: &str = "/Users/me/projects/kestrel-site";
const FORMS: &str = "/Users/me/projects/kestrel-site-forms";
const CHATS: &str = "/Users/me/Library/Application Support/Codealong/chats";
const NOTES: &str = "/Users/me/notes";

const APP_SLUG: &str = "northwind/northwind-app";
const API_SLUG: &str = "northwind/northwind-api";
const ATLAS_SLUG: &str = "atlas-labs/atlas-core";
const SITE_SLUG: &str = "kestrel-hq/kestrel-site";

/// The settings `CONFIG` holds. Read, not checked as a saved config.toml is (`config::parse`):
/// that asks the machine it runs on for its sounds and its terminals, and this Mac is made up.
pub fn config() -> Config {
    config::load_text(CONFIG).expect("the fixture's config.toml parses")
}

/// A commit's short name for a branch, the same every time: a worktree's HEAD, and the commit a
/// PR of that branch is at.
fn head_of(branch: &str) -> String {
    let hash = branch.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3));
    format!("{:07x}", hash & 0x0fff_ffff)
}

/// A repo's worktrees as the engine seeds them from its main checkout (engine/reposcan.rs `load_repos`):
/// the checkout at `root` on `main`, then each `(folder, branch, who made it)`. What each is
/// called is the engine's own rule for its folder (`repos::callsign`); who made it is said here,
/// as `repos::origin` has it of such a folder on a Mac whose temporary folder is elsewhere: that
/// rule reads `TMPDIR`, and nothing of the machine this runs on may change the made-up one.
fn repo(project: &str, slug: &str, root: &str, worktrees: &[(&str, &str, Origin)]) -> Vec<LaneSeed> {
    let (owner, name) = slug.split_once('/').expect("a repo's slug is owner/name");
    let checkout = crate::repos::Checkout {
        root: PathBuf::from(root),
        git_dir: PathBuf::from(root).join(".git"),
        name: root.rsplit('/').next().unwrap_or(root).to_string(),
        identity: Some(format!("github.com/{slug}")),
        github: Some((owner.into(), name.into())),
        github_owners: vec![owner.into(), YOU.into()],
    };
    let home = Path::new(HOME);
    std::iter::once(&(root, "main", Origin::Main))
        .chain(worktrees)
        .map(|(path, branch, origin)| LaneSeed {
            callsign: crate::repos::callsign(&checkout, Path::new(path), Some(*branch), home),
            origin: *origin,
            repo_slug: checkout.slug(),
            head_owners: checkout.github_owners.clone(),
            branch: Some(branch.to_string()),
            head: Some(head_of(branch)),
            default_branch: Some("main".into()),
            ..crate::engine::folder_seed(PathBuf::from(path), project, name, false)
        })
        .collect()
}

/// A folder outside every project, which is no repo's checkout (engine/reposcan.rs `stray_seeds`).
fn elsewhere(path: &str, name: &str) -> LaneSeed {
    LaneSeed { origin: Origin::Folder, ..crate::engine::folder_seed(PathBuf::from(path), ELSEWHERE, name, true) }
}

fn seeds() -> Vec<LaneSeed> {
    // Made by a script beside the checkout, but for one Claude Code made and one the Codex app did.
    let script = Origin::Script;
    let app = [
        (CHECKOUT, "checkout-flow", script),
        (SEARCH, "search-filters", script),
        (INVOICES, "invoice-export", script),
        (ONBOARDING, "onboarding-copy", script),
        (IMPORTS, "worktree-tidy-imports", Origin::ClaudeApp),
        (BILLING, "billing-retries", script),
        (LIMITS, "rate-limits", script),
        (FLAGS, "feature-flags", script),
        (NAV, "old-nav", script),
        (LEGACY, "legacy-cleanup", script),
        (SPARE, "spare", script),
    ];
    let mut seeds = repo("Northwind", APP_SLUG, APP, &app);
    seeds.extend(repo("Northwind", API_SLUG, API, &[(WEBHOOKS, "webhook-retries", script)]));
    seeds.extend(repo("Atlas", ATLAS_SLUG, ATLAS, &[(IMPORTER, "csv-importer", script), (DATES, "codex/fix-dates", Origin::Codex)]));
    seeds.extend(repo("Kestrel", SITE_SLUG, SITE, &[(FORMS, "contact-form", script)]));
    // In no project: Codealong's own chats, always there to start one in, and a plain folder.
    seeds.push(LaneSeed { chats: true, starts_chats: true, ..elsewhere(CHATS, "Chats") });
    seeds.push(LaneSeed { exact: true, ..elsewhere(NOTES, "notes") });
    seeds
}

/// What the cast is built with: the time everything counts back from, the day that's on (days
/// since 1970), and the settings.
struct Cast {
    now: i64,
    today: i64,
    cfg: Config,
}

impl Cast {
    /// The day is `now`'s, read once, so a board made as midnight passes is all of one day: in
    /// the time zone of the machine this runs on, which the page that shows it counts days in.
    fn at(now: i64) -> Cast {
        Cast { now, today: crate::schedules::day_of(now, &crate::schedules::rule::Local), cfg: config() }
    }

    fn harness_of(&self, account: &str) -> Harness {
        let a = self.cfg.accounts.iter().find(|a| a.name == account).expect("an account of the fixture's config.toml");
        Harness::from_config(&a.harness)
    }

    /// The key of `account`'s session called `id`: its harness, its account and `id`
    /// (`claude:claude:checkout-approval`).
    fn key(&self, account: &str, id: &str) -> String {
        Session::key_for(self.harness_of(account), account, id)
    }

    /// A session of `account` called `id`, started in `folder`, running in a terminal tab: last
    /// prompted and last active so many minutes ago, its status as old as its last activity.
    fn session(&self, account: &str, id: &str, folder: &str, title: &str, status: Status, prompted: i64, active: i64) -> Session {
        let known = Session::new(self.harness_of(account), account, id);
        Session {
            status,
            live: true,
            // A process of its own, the same every time.
            pid: Some(20_000 + id.bytes().map(u32::from).sum::<u32>()),
            title: title.into(),
            cwd: folder.into(),
            started_at: Some(self.now - (prompted + 45) * MIN),
            status_since: Some(self.now - active * MIN),
            last_prompt_at: self.now - prompted * MIN,
            last_activity: self.now - active * MIN,
            first_prompt: Some(title.into()),
            open_target: format!("s|{}|focus", known.key),
            terminal: Some(crate::launch::terminal_name(&self.cfg.apps.terminal)),
            model: model_of(account).map(str::to_string),
            ..known
        }
    }

    fn ago(&self, minutes: i64) -> i64 {
        self.now - minutes * MIN
    }
}

/// The accounts their harness's own app runs: the ones in the folder it keeps (`~/.claude` the
/// Claude app's, `~/.codex` the Codex app's), as the readers tell them (board/sessions.rs). A
/// session of one in a terminal can be opened in the app too.
const APP_ACCOUNTS: [&str; 2] = ["claude", "codex"];

/// The model an account's sessions are on, as its harness names it in their records; none
/// where it doesn't say.
fn model_of(account: &str) -> Option<&'static str> {
    match account {
        "claude" | "claude2" => Some("claude-opus-5-5"),
        "codex" => Some("gpt-5.5"),
        "cursor" => Some("composer-2"),
        "antigravity" => Some("gemini-3-pro"),
        _ => None,
    }
}

/// What goes on with an account's session in a terminal, as its harness writes the command;
/// none for a chat in an app whose harness has no command line to go on with it.
fn resume_of(account: &str, id: &str, in_terminal: bool) -> Option<String> {
    match account {
        "claude" | "claude2" => Some(format!("{account} --resume {id}")),
        "codex" => Some(format!("codex resume {id}")),
        "opencode" => Some(format!("opencode --session {id}")),
        "antigravity" if in_terminal => Some(format!("agy --conversation {id}")),
        "cursor" if in_terminal => Some(format!("cursor-agent --resume {id}")),
        _ => None,
    }
}

fn said(s: Session, excerpt: &str) -> Session {
    Session { excerpt: Some(excerpt.into()), ..s }
}

/// One that isn't running any more: its terminal is closed, and opening it resumes it.
fn stopped(s: Session) -> Session {
    Session { live: false, pid: None, terminal: None, open_target: format!("s|{}|resume", s.key), ..s }
}

/// One in its harness's own app, which `opens` names as the target that opens it there does.
fn in_app(s: Session, opens: &str) -> Session {
    Session { surface: Surface::App, pid: None, terminal: None, open_target: format!("s|{}|{opens}", s.key), ..s }
}

/// A mode a Claude Code session in a terminal steps through with Shift+Tab.
fn mode(kind: ModeKind, label: &str, detail: &str, id: &str) -> SessionMode {
    SessionMode::new(kind, label, detail).with_id(id)
}

/// The sessions, each with the minutes since it was last prompted and last active. Many checks
/// take the first of something (the board's first session, the latest three, the first PRs)
/// and then look for it in the project the window shows, the first. So the first project has
/// the head of every order, as the project you work in most has on a real board: the session
/// that has waited longest, the ones prompted and active last, the PRs changed last; and none
/// of those is a helper or a run nobody is at, which have no row of their own.
fn sessions(c: &Cast) -> Vec<Session> {
    let now = c.now;
    let claude = |id: &str| c.key("claude", id);
    let modes = vec![
        mode(ModeKind::Asks, "Manual", "Asks before it edits files or runs commands.", "default"),
        mode(ModeKind::Edits, "Accept edits", "Edits files without asking, and asks before other commands.", "acceptEdits"),
        mode(ModeKind::Plan, "Plan", "Plans, and changes nothing until you take the plan.", "plan"),
    ];
    // What a Claude Code session in a terminal says of itself besides its model.
    let opus = |s: Session| Session { effort: Some("high".into()), context: Some(ContextUse { used: 84_000, window: Some(200_000) }), mode: modes.first().cloned(), modes: modes.clone(), ..s };
    let mut sessions = vec![
        // The repo's main checkout: one quiet session, long done.
        opus(said(c.session("claude", "tidy-readme", APP, "Tidy the README's setup steps", Status::Idle, 180, 170), "Updated the setup steps and the troubleshooting list.")),
        // The ways of needing you that hold a turn, and a reply that asks, beside a turn that's
        // running. The reply has waited longest of all, so its worktree is the board's first and
        // it's that worktree's first session: a Claude Code session at its prompt in a terminal,
        // the plainest there is, for whatever takes the board's first session to make its own of.
        Session { asks: true, ..opus(said(c.session("claude", "checkout-reply", CHECKOUT, "Carry the cart across sign-in", Status::Needs, 90, 55), "Asks you · Want me to open the PR?")) },
        Session { excerpt: Some(board::asks_you("Should guests check out without an account?")), ..c.session("claude2", "checkout-question", CHECKOUT, "Decide how guests pay", Status::Needs, 40, 30) },
        opus(said(c.session("claude", "checkout-approval", CHECKOUT, "Add a guest checkout", Status::Needs, 28, 25), "Bash · `npm run db:migrate`")),
        Session { stoppable: true, steerable: true, ..opus(said(c.session("claude", "checkout-tests", CHECKOUT, "Cover checkout with tests", Status::Working, 3, 1), "`npm test -- checkout`")) },
        // Work under way: a turn that runs, one that waits on a command in the background, and
        // a turn that ended beside them. They're kin: the first was forked into the last, which
        // started the second.
        Session { stoppable: true, steerable: true, started_at: Some(c.ago(200)), ..opus(said(c.session("claude", "search-filters", SEARCH, "Add filters to search", Status::Working, 2, 0), "`npm run typecheck`")) },
        Session {
            background: Some("gh run watch".into()),
            origin: Some(SessionOrigin { key: c.key("claude2", "search-ranking"), how: OriginHow::Started }),
            ..said(c.session("codex", "search-index", SEARCH, "Rebuild the search index nightly", Status::Working, 25, 8), "Waiting on `gh run watch`")
        },
        Session { origin: Some(SessionOrigin { key: claude("search-filters"), how: OriginHow::Fork }), ..said(c.session("claude2", "search-ranking", SEARCH, "Rank exact matches first", Status::Unread, 70, 50), "Exact matches now sort first; the tests cover ties.") },
        // Your turn: finished turns you haven't seen, one whose work is done, and one put away.
        // Two came of the first: a helper its agent started, which works for it and has no row
        // of its own, and a fork. Neither is under one of the latest sessions, which a check pins.
        Session { started_at: Some(c.ago(300)), ..opus(said(c.session("claude", "invoice-export", INVOICES, "Export invoices as CSV", Status::Unread, 35, 20), "Pushed: the export is behind the `csv_export` flag.")) },
        Session {
            helper: Some(Helper { parent: Some(claude("invoice-export")), ..Default::default() }),
            origin: Some(SessionOrigin { key: claude("invoice-export"), how: OriginHow::Started }),
            ..said(c.session("codex", "invoice-review", INVOICES, "Review the invoice export", Status::Working, 26, 9), "`git diff origin/main...HEAD`")
        },
        Session { origin: Some(SessionOrigin { key: claude("invoice-export"), how: OriginHow::Fork }), insight: Some(SessionInsight { finished: Some(0.92), landing: Some("pushed".into()), ..Default::default() }), ..said(c.session("claude2", "invoice-totals", INVOICES, "Fix the rounding in invoice totals", Status::Unread, 55, 44), "All done here.") },
        Session { archived: true, ..stopped(said(c.session("claude", "invoice-spike", INVOICES, "Try a PDF export", Status::Idle, 900, 880), "A PDF export needs a headless browser; left it out.")) },
        // A PR in each state it can be in, each with the session that made it.
        Session {
            chrome: Some(ChromeTabs { tabs: vec![412, 413], url: "http://localhost:5173/welcome".into(), title: "Welcome to Northwind".into(), profile: Some("Work".into()), profile_dir: Some("Profile 1".into()), at: c.ago(300) }),
            artifacts: vec![Artifact { url: "https://claude.ai/artifact/northwind-onboarding".into(), title: "Onboarding copy".into(), note: Some("Updated · version 3".into()), at: c.ago(310) }],
            ..opus(said(c.session("claude", "onboarding-copy", ONBOARDING, "Rewrite the onboarding copy", Status::Idle, 300, 290), "Opened the PR."))
        },
        Session { attach_command: Some("claude attach tidy-imports".into()), pid: None, terminal: None, own_worktree: Some(IMPORTS.into()), open_target: format!("s|{}|attach", claude("tidy-imports")), ..said(c.session("claude", "tidy-imports", IMPORTS, "Tidy the imports", Status::Idle, 420, 410), "Sorted the imports; the draft PR has the rest.") },
        stopped(said(c.session("claude2", "billing-retries", BILLING, "Retry failed charges with backoff", Status::Idle, 430, 420), "Approved; ready to merge.")),
        said(c.session("codex", "rate-limits", LIMITS, "Rate-limit the public API", Status::Idle, 440, 430), "Two review comments left to answer."),
        stopped(said(c.session("claude", "feature-flags", FLAGS, "Put new features behind flags", Status::Idle, 610, 600), "Merged it.")),
        stopped(c.session("claude", "old-nav", NAV, "Remove the old navigation", Status::Idle, 3 * 24 * 60, 3 * 24 * 60 - 20)),
        // Long quiet: its worktree is parked.
        stopped(c.session("claude", "legacy-cleanup", LEGACY, "Clear out the legacy exports", Status::Stopped, 20 * 24 * 60, 20 * 24 * 60)),
        // Another repo of the same project: a thread in its harness's app, and one Codealong runs.
        in_app(said(c.session("codex", "api-pagination", API, "Paginate the orders endpoint", Status::Idle, 240, 230), "Cursor pagination is in; the old page numbers still work."), "codex"),
        Session { hosted: true, pid: None, terminal: None, stoppable: true, open_target: format!("s|{}|hosted", claude("api-openapi")), ..said(c.session("claude", "api-openapi", API, "Write the OpenAPI description", Status::Working, 14, 1), "`npm run docs:build`") },
        // Turns that failed: its account's limit, with another account to move to, and one cut off.
        Session {
            limit: Some(SessionLimit { meter: Some("session".into()), resets_at: Some(now + 95 * MIN), at: c.ago(10), move_to: Some("claude2".into()), ..Default::default() }),
            ..said(c.session("claude", "webhook-retries", WEBHOOKS, "Retry failed webhooks", Status::Error, 15, 10), "You've hit your limit")
        },
        Session { cut_off: Some(c.ago(20)), ..stopped(said(c.session("claude2", "webhook-signing", WEBHOOKS, "Sign outgoing webhooks", Status::Error, 30, 20), "`npm test -- webhooks`")) },
        // A second project, on other harnesses.
        said(c.session("opencode", "atlas-roadmap", ATLAS, "Draft the importer's roadmap", Status::Idle, 200, 190), "The roadmap is in docs/roadmap.md."),
        in_app(said(c.session("cursor", "atlas-schema", ATLAS, "Sketch the schema for imports", Status::Unread, 65, 60), "Three tables: imports, rows and errors."), "cursor"),
        said(c.session("antigravity", "atlas-importer", IMPORTER, "Import customers from CSV", Status::Working, 21, 11), "`pytest tests/test_import.py`"),
        Session { stalled_since: Some(c.ago(12)), excerpt: Some(format!("Stuck on `atlas login` for {{for:{}}}", c.ago(12))), ..c.session("claude", "atlas-login", IMPORTER, "Sign the importer in", Status::Needs, 16, 12) },
        in_app(said(c.session("codex", "atlas-dates", DATES, "Fix dates before 1970", Status::Idle, 500, 490), "Dates before 1970 parse now."), "codex"),
        // A third: finished turns moved to the top and snoozed, a run nobody is at, and the
        // other ways of needing you.
        Session { raised: Some(Raised { at: c.ago(2), ..Default::default() }), ..opus(said(c.session("claude", "kestrel-hero", SITE, "Redo the home page's hero", Status::Unread, 120, 100), "Ready when you are.")) },
        Session { snoozed_until: Some(now + HOUR), ..said(c.session("claude2", "kestrel-pricing", SITE, "Update the pricing table", Status::Unread, 130, 110), "Done.") },
        Session { headless: true, origin: Some(SessionOrigin { key: claude("kestrel-hero"), how: OriginHow::Started }), ..c.session("claude", "kestrel-links", SITE, "Check every link on the site", Status::Working, 27, 16) },
        Session { excerpt: Some(format!("{HELD_MESSAGE}“Redo the home page's hero”")), ..c.session("claude", "forms-held", FORMS, "Build the contact form", Status::Needs, 16, 15) },
        Session {
            excerpt: Some(format!("{TERMINAL_ASKS}`Continue? [y/N]`")),
            running: Some(RunningIn { terminal: "Codealong".into(), command: "./scripts/seed.sh".into(), yours: true }),
            ..c.session("claude", "forms-seed", FORMS, "Seed the form's test data", Status::Needs, 18, 17)
        },
        Session { excerpt: Some(format!("{START_WAITS}Codex: Hooks need review")), ..c.session("claude2", "forms-spam", FORMS, "Keep spam out of the form", Status::Needs, 19, 16) },
        // In no project: a chat, and a session in a folder of your own.
        said(c.session("claude", "chat-flights", &format!("{CHATS}/flight-times"), "Compare flight times to Lisbon", Status::Idle, 260, 250), "The morning flight is an hour shorter."),
        said(c.session("claude", "notes-recipes", NOTES, "Sort the recipes by season", Status::Idle, 270, 265), "Sorted: spring has the fewest."),
    ];
    for s in &mut sessions {
        s.resume_command = resume_of(&s.account, &s.id, s.surface == Surface::Cli);
        s.app_account = APP_ACCOUNTS.contains(&s.account.as_str());
        // As the engine does once nothing more is laid over a session.
        s.settle_need();
    }
    sessions
}

/// A PR of `branch` in `slug`, opened a couple of days ago and last changed so many minutes ago,
/// with passing checks.
fn pr(c: &Cast, slug: &str, number: u64, title: &str, branch: &str, state: PrState, author: &str, updated: i64) -> PrRaw {
    let owner = slug.split_once('/').map_or("", |(owner, _)| owner);
    PrRaw {
        number,
        title: title.into(),
        url: format!("https://github.com/{slug}/pull/{number}"),
        state,
        head_ref: branch.into(),
        head_owner: Some(owner.into()),
        head_oid: Some(head_of(branch)),
        updated_at: c.ago(updated),
        merged_at: (state == PrState::Merged).then(|| c.ago(updated)),
        closed_at: (state == PrState::Closed).then(|| c.ago(updated)),
        checks: Checks { passed: 11, ..Default::default() },
        merge: if state.is_open() { Merge::Clean } else { Merge::Unknown },
        // You keep every repo here, so each PR is yours to merge.
        writes: Some(true),
        merging: false,
        author: Some(author.into()),
        base_ref: Some("main".into()),
        created_at: c.ago(updated) - 2 * DAY,
        additions: 120,
        deletions: 8,
        changed_files: 6,
        review_requested: false,
        requested_by: None,
        requested_at: None,
        reviewers_asked: Vec::new(),
        assigned: None,
        authored_by: None,
        labels: Vec::new(),
        last_review: None,
        stale_approval: None,
        head_at: Some(c.ago(updated)),
        said: Vec::new(),
        said_from: None,
        changes_asked: Vec::new(),
    }
}

/// Someone else's PR that asks for your review, since so many minutes ago.
fn asks_review(p: PrRaw, since: i64) -> PrRaw {
    PrRaw { review_requested: true, requested_by: p.author.clone(), requested_at: Some(since), authored_by: None, ..p }
}

fn prs(c: &Cast) -> HashMap<String, Vec<PrRaw>> {
    let label = |name: &str, color: &str| PrLabel { name: name.into(), color: color.into(), description: None };
    HashMap::from([
        (
            APP_SLUG.to_string(),
            vec![
                PrRaw { authored_by: Some(c.harness_of("claude")), reviewers_asked: vec![DANA.into()], ..pr(c, APP_SLUG, 412, "Rewrite the onboarding copy", "onboarding-copy", PrState::Open, YOU, 290) },
                PrRaw { merge: Merge::Blocked, ..pr(c, APP_SLUG, 415, "Tidy the imports", "worktree-tidy-imports", PrState::Draft, YOU, 410) },
                pr(c, APP_SLUG, 409, "Retry failed charges with backoff", "billing-retries", PrState::Approved, YOU, 420),
                PrRaw {
                    checks: Checks { passed: 9, failed: 2, failing: vec!["lint".into(), "unit".into()], ..Default::default() },
                    merge: Merge::Blocked,
                    changes_asked: vec![ChangesAsked { who: DANA.into(), at: c.ago(500), commit: Some(head_of("rate-limits")) }],
                    ..pr(c, APP_SLUG, 405, "Rate-limit the public API", "rate-limits", PrState::Changes, YOU, 430)
                },
                pr(c, APP_SLUG, 398, "Put new features behind flags", "feature-flags", PrState::Merged, YOU, 600),
                pr(c, APP_SLUG, 377, "Remove the old navigation", "old-nav", PrState::Closed, YOU, 3 * 24 * 60 - 30),
                // Two that wait on your review, and one of a teammate's that doesn't.
                asks_review(PrRaw { labels: vec![label("checkout", "0e8a16")], ..pr(c, APP_SLUG, 418, "Add coupon codes to checkout", "coupon-codes", PrState::Open, DANA, 150) }, c.ago(180)),
                asks_review(PrRaw { checks: Checks { passed: 7, pending: 4, ..Default::default() }, ..pr(c, APP_SLUG, 416, "Round tax to the cent", "tax-rounding", PrState::Open, LEE, 26 * 60) }, c.ago(26 * 60)),
                pr(c, APP_SLUG, 402, "Show stock on the product page", "stock-badge", PrState::Open, DANA, 700),
            ],
        ),
        (API_SLUG.to_string(), vec![PrRaw { checks: Checks { passed: 5, pending: 3, ..Default::default() }, merge: Merge::Blocked, ..pr(c, API_SLUG, 57, "Retry failed webhooks", "webhook-retries", PrState::Open, YOU, 25) }]),
        (
            ATLAS_SLUG.to_string(),
            vec![
                pr(c, ATLAS_SLUG, 88, "Import customers from CSV", "csv-importer", PrState::Open, YOU, 800),
                asks_review(pr(c, ATLAS_SLUG, 91, "Report rows that fail to import", "import-errors", PrState::Open, LEE, 900), c.ago(920)),
            ],
        ),
    ])
}

/// What git says of each worktree: a main checkout level with its remote, a branch ahead of it,
/// uncommitted files where work is under way.
fn git(c: &Cast) -> HashMap<PathBuf, GitInfo> {
    let level = |commit: &str, at: i64| GitInfo { upstream: Some("origin/main".into()), last_commit: Some(commit.into()), last_commit_at: Some(c.ago(at)), base: Some("origin/main".into()), base_ahead: Some(0), base_behind: Some(0), unpushed: Some(0), ..Default::default() };
    let branch = |name: &str, commit: &str, at: i64, ahead: u32, dirty: u32| GitInfo {
        upstream: Some(format!("origin/{name}")),
        dirty,
        last_commit: Some(commit.into()),
        last_commit_at: Some(c.ago(at)),
        added: Some(120),
        removed: Some(8),
        base: Some("origin/main".into()),
        base_ahead: Some(ahead),
        base_behind: Some(2),
        unpushed: Some(0),
        ..Default::default()
    };
    HashMap::from([
        // The main checkout holds uncommitted work, as one that many sessions share does.
        (PathBuf::from(APP), GitInfo { dirty: 3, ..level("Put new features behind flags (#398)", 600) }),
        (PathBuf::from(CHECKOUT), GitInfo { upstream: None, unpushed: Some(3), ..branch("checkout-flow", "Add the guest checkout form", 40, 3, 4) }),
        (PathBuf::from(SEARCH), branch("search-filters", "Filter search by category", 30, 2, 2)),
        (PathBuf::from(INVOICES), branch("invoice-export", "Export invoices as CSV", 22, 4, 0)),
        (PathBuf::from(ONBOARDING), branch("onboarding-copy", "Rewrite the onboarding copy", 295, 1, 0)),
        (PathBuf::from(IMPORTS), branch("worktree-tidy-imports", "Sort the imports", 415, 1, 1)),
        (PathBuf::from(BILLING), branch("billing-retries", "Retry failed charges with backoff", 425, 2, 0)),
        (PathBuf::from(LIMITS), branch("rate-limits", "Rate-limit the public API", 435, 5, 0)),
        (PathBuf::from(FLAGS), branch("feature-flags", "Put new features behind flags", 605, 3, 0)),
        (PathBuf::from(NAV), branch("old-nav", "Remove the old navigation", 3 * 24 * 60, 1, 0)),
        (PathBuf::from(LEGACY), branch("legacy-cleanup", "Clear out the legacy exports", 20 * 24 * 60, 1, 0)),
        // Reused: reset onto the default branch after its last work, and ready for the next.
        (PathBuf::from(SPARE), GitInfo { reset_at: Some(c.ago(2 * 24 * 60)), new_work_count: Some(6), ..level("Put new features behind flags (#398)", 600) }),
        (PathBuf::from(API), level("Paginate the orders endpoint (#55)", 235)),
        (PathBuf::from(WEBHOOKS), branch("webhook-retries", "Retry failed webhooks", 26, 2, 3)),
        (PathBuf::from(ATLAS), level("Add the import tables (#86)", 195)),
        (PathBuf::from(IMPORTER), branch("csv-importer", "Read customers from CSV", 62, 6, 1)),
        (PathBuf::from(DATES), GitInfo { upstream: None, unpushed: Some(1), ..branch("codex/fix-dates", "Parse dates before 1970", 492, 1, 0) }),
        (PathBuf::from(SITE), GitInfo { dirty: 2, ..level("Update the footer links", 105) }),
        (PathBuf::from(FORMS), branch("contact-form", "Add the contact form", 18, 2, 5)),
    ])
}

/// Each account's plan and limits, read a minute ago: one near its weekly limit, one whose
/// harness keeps no limits, one not read yet.
fn usage(c: &Cast) -> Vec<Usage> {
    let meter = |key: &str, label: &str, window: &str, percent: f64, resets_in: i64, model: Option<&str>| Meter {
        key: key.into(),
        label: label.into(),
        window: window.into(),
        percent,
        resets_at: Some(c.now + resets_in),
        severity: crate::usage::severity(None, percent),
        model: model.map(str::to_string),
    };
    let session = |percent: f64| meter("session", "5h", "5-hour session", percent, 95 * MIN, None);
    let week = |percent: f64| meter("week", "Week", "Weekly", percent, 3 * DAY, None);
    c.cfg
        .accounts
        .iter()
        .map(|a| {
            let read = |plan: &str, meters: Vec<Meter>| Usage { plan: Some(plan.into()), meters, as_of: Some(c.ago(1)), loading: false, ..crate::usage::loading(a) };
            match a.name.as_str() {
                "claude" => read("Max 20x", vec![session(100.0), week(63.0), meter("model:fable", "Fable", "Weekly, Fable only", 81.0, 3 * DAY, Some("Fable"))]),
                "claude2" => read("Team", vec![session(12.0), week(30.0)]),
                "codex" => read("Pro", vec![session(71.0), week(94.0)]),
                "cursor" => read("Pro", vec![meter("month", "Month", "Monthly", 38.0, 12 * DAY, None)]),
                "antigravity" => crate::usage::loading(a),
                _ => crate::usage::none(a, "Its harness keeps no limits Codealong can read."),
            }
        })
        .collect()
}

fn labels() -> Vec<Label> {
    [("l-billing", "billing", "green"), ("l-bug", "bug", "red"), ("l-design", "design", "purple")].into_iter().map(|(id, name, color)| Label { id: id.into(), name: name.into(), color: color.into() }).collect()
}

/// The todos, as todos.json would hold them: under way with its session and worktree, due
/// today, overdue, waiting on something, blocked by another, done, and one in no project.
fn todos(c: &Cast) -> Vec<Todo> {
    let day = |from_today: i64| crate::util::day_key(c.today + from_today);
    let export = c.key("claude", "invoice-export");
    let list = json!([
        { "id": "t-export", "project": "Northwind", "title": "Export invoices as CSV", "description": "Finance asks for a CSV of each month's invoices.\n\nBehind a flag until they've tried it.", "status": "in-progress", "labels": ["l-billing"], "worktrees": [INVOICES], "sessions": [export], "primary": { "session": export, "since": c.ago(35) }, "createdAt": c.now - 3 * DAY, "updatedAt": c.ago(20) },
        { "id": "t-coupons", "project": "Northwind", "title": "Review the coupon codes PR", "description": "", "status": "todo", "labels": [], "due": day(0), "createdAt": c.now - 2 * DAY, "updatedAt": c.now - 2 * DAY },
        { "id": "t-keys", "project": "Northwind", "title": "Rotate the API keys", "description": "The staging keys are a year old.", "status": "todo", "labels": ["l-bug"], "due": day(-2), "createdAt": c.now - 9 * DAY, "updatedAt": c.now - 4 * DAY },
        { "id": "t-empty-cart", "project": "Northwind", "title": "Draw the empty cart", "description": "", "status": "todo", "labels": ["l-design"], "waiting": { "note": "Dana's sketches", "since": c.now - DAY }, "createdAt": c.now - 5 * DAY, "updatedAt": c.now - DAY },
        { "id": "t-receipts", "project": "Northwind", "title": "Email a receipt after checkout", "description": "", "status": "todo", "labels": [], "dependsOn": ["t-export"], "createdAt": c.now - 4 * DAY, "updatedAt": c.now - 4 * DAY },
        { "id": "t-search", "project": "Northwind", "title": "Add filters to search", "description": "", "status": "todo", "labels": [], "createdAt": c.now - 6 * DAY, "updatedAt": c.now - 6 * DAY },
        { "id": "t-flags", "project": "Northwind", "title": "Put new features behind flags", "description": "", "status": "done", "labels": [], "worktrees": [FLAGS], "doneAt": c.ago(600), "createdAt": c.now - 8 * DAY, "updatedAt": c.ago(600) },
        { "id": "t-import", "project": "Atlas", "title": "Import customers from CSV", "description": "", "status": "in-progress", "labels": [], "worktrees": [IMPORTER], "due": day(3), "createdAt": c.now - 2 * DAY, "updatedAt": c.ago(60) },
        { "id": "t-footer", "project": "Kestrel", "title": "Fix the footer's links", "description": "", "status": "todo", "labels": [], "createdAt": c.now - DAY, "updatedAt": c.now - DAY },
        { "id": "t-taxes", "project": "", "title": "Send the accountant last year's receipts", "description": "", "status": "todo", "labels": [], "due": day(5), "createdAt": c.now - 7 * DAY, "updatedAt": c.now - 7 * DAY }
    ]);
    serde_json::from_value(list).expect("the fixture's todos read as todos.json does")
}

/// The editors installed on the made-up Mac, in menu order.
fn editors() -> Vec<Profile> {
    [("cursor", "Cursor"), ("vscode", "VS Code"), ("zed", "Zed")].into_iter().map(|(id, name)| Profile { id: id.into(), name: name.into() }).collect()
}

/// The board, as the engine would compose it from this Mac's sessions, worktrees, PRs and todos
/// at `now`.
pub fn board(now: i64) -> Board {
    let c = Cast::at(now);
    let cfg = &c.cfg;
    let apps = AppNames {
        terminal: crate::launch::terminal_name(&cfg.apps.terminal),
        terminal_id: cfg.apps.terminal.clone(),
        editor: crate::editors::name(&cfg.apps.editor),
        editor_id: cfg.apps.editor.clone(),
        editors: editors(),
        diff_editor: editors().into_iter().find(|e| e.id == cfg.apps.editor),
        new_terminal: cfg.apps.new_terminal,
        run_in: cfg.apps.run_in,
    };
    let order: Vec<String> = cfg.projects.iter().map(|p| p.name.clone()).collect();
    let seeds = seeds();
    let prs = prs(&c);
    // The session that made the hero worked in the forms' worktree too.
    let visitors = HashMap::from([(PathBuf::from(FORMS), vec![c.key("claude", "kestrel-hero")])]);
    let previews = Previews { set: vec![crate::previews::Entry { worktree: ONBOARDING.into(), url: "http://localhost:5173".into(), port: None, chrome_profile: None, at: c.ago(300) }], ..Default::default() };
    let mut board = board::compose(Inputs {
        now,
        apps: &apps,
        seeds: &seeds,
        sessions: sessions(&c),
        visitors: &visitors,
        git: &git(&c),
        prs: &prs,
        requested: &HashMap::new(),
        made: &HashMap::new(),
        project_order: &order,
        // Nothing is wrong in the background, as on most days: a check that wants notices shows its
        // own (PREVIEW_NOTICES), and one that counts them counts only those.
        notices: Vec::new(),
        loading: false,
        github_at: Some(c.ago(2)),
        pinned: &[],
        github_login: Some(YOU),
        pr_links: &PrLinks::default(),
        previews: &previews,
        reusable: &HashMap::from([(SPARE.to_string(), true)]),
        review_snoozes: &[],
        auto_reviews: (&Default::default(), &cfg.reviews, &Default::default()),
        session_settings: &cfg.sessions,
        machines: &Default::default(),
        at_merged: &HashSet::from([PathBuf::from(FLAGS)]),
        working_elsewhere: &Default::default(),
    });
    // Where a worktree's new terminal opens is written under this Mac's home (`~/projects/…`):
    // the board's rule shortens it by the home of the machine it runs on (board/headlines.rs
    // `targets`), which isn't /Users/me.
    for lane in &mut board.lanes {
        let here = crate::util::tilde_in(Path::new(&lane.path), Path::new(HOME));
        for target in lane.targets.iter_mut().filter(|t| t.id == "l|terminal") {
            target.detail = Some(here.clone());
        }
    }
    // What the engine lays over a composed board (engine/publish.rs `compose`): what it read of the
    // accounts and GitHub, the settings the windows go by, the todos, and what follows from them.
    board.usage = usage(&c);
    board.github_polled = true;
    board.github_current = prs.keys().map(|slug| (slug.clone(), Some(c.ago(2)))).collect();
    board.load = Some(Load { level: LoadLevel::Calm, per_cpu: Some(0.4), cpus: 10, memory_free: Some(46), memory_short: false });
    board.when_limited = cfg.limits.when_hit;
    board.nudge_about = cfg.pull_requests.nudge_about;
    board.reviews = cfg.reviews.clone();
    board.repos = cfg.repos.clone();
    board.theme = cfg.appearance.theme;
    board.zoom = cfg.appearance.zoom;
    board.glass = cfg.appearance.glass;
    board.show_keys = cfg.appearance.show_keys;
    board.quick_shortcut = cfg.shortcuts.quick_panel.clone();
    board.chat_shortcut = cfg.shortcuts.chat.clone();
    board.waiting_shortcut = cfg.shortcuts.waiting.clone();
    board.improve_shortcut = cfg.shortcuts.improve.clone();
    board.main_window_shortcut = cfg.shortcuts.main_window.clone();
    let in_use = cfg.shortcuts_in_use();
    board.talk_shortcut = in_use.talk;
    board.talk_orchestrator_shortcut = in_use.talk_orchestrator;
    board.listen_shortcut = in_use.listen;
    board.decisions = cfg.decisions.clone();
    board.experimental = cfg.experimental.clone();
    board.queue = cfg.queue.settled();
    board.devices = cfg.devices.clone();
    board.project_rules = cfg.projects.clone();
    board.todos = crate::todos::awake_at(todos(&c), now);
    board.today = crate::util::day_key(c.today);
    board.todo_clock = crate::todos::todo_clock(&board.todos, now);
    board.labels = labels();
    board::settle_finished(&mut board.lanes, &board.prs, &board.todos);
    board.todo_queue = crate::barfeed::todo_queue(&board);
    board
}

/// The accounts as Settings lists them: every one signed in, with its folder there.
pub fn accounts() -> Vec<AccountInfo> {
    config()
        .accounts
        .iter()
        .map(|a| AccountInfo {
            name: a.name.clone(),
            harness: a.harness.clone(),
            home: a.home.clone(),
            command: a.command.clone(),
            start: a.start.clone(),
            acp: a.acp.clone(),
            monthly: a.monthly,
            per_token: a.per_token,
            usage_cap: a.usage_cap,
            disabled: a.disabled,
            chrome_profile: a.chrome_profile.clone(),
            makes_pages: crate::harness::of(a).is_some_and(|h| !h.artifact_needles().is_empty()),
            copies_settings: crate::harness::of(a).is_some_and(|h| h.copies_settings()),
            folder_exists: true,
            signed_in: true,
            email: Some(format!("sam@{}.example", if a.name == "claude2" { "northwind" } else { "home" })),
            sign_in_note: None,
            missing_cli: None,
        })
        .collect()
}

/// The models each account's command line offers, by account: what a harness reads from the
/// account's own folder, or asks its program for, on a real Mac.
fn models_of(account: &str) -> Vec<ModelOption> {
    let listed: &[(&str, &str)] = match account {
        "claude" | "claude2" => &[("fable", "Fable"), ("opus", "Opus"), ("sonnet", "Sonnet"), ("haiku", "Haiku")],
        "codex" => &[("gpt-5.5", "GPT-5.5"), ("gpt-5.5-mini", "GPT-5.5 mini")],
        "cursor" => &[("auto", "Auto"), ("composer-2", "Composer 2")],
        "antigravity" => &[("gemini-3-pro", "Gemini 3 Pro"), ("gemini-3-flash", "Gemini 3 Flash")],
        _ => &[],
    };
    listed.iter().map(|(id, label)| ModelOption { id: id.to_string(), label: label.to_string() }).collect()
}

/// Where each account can start a session, as `start::options` says it of a Mac that has every
/// harness's command line, app and Agent Client Protocol adapter: each account starts wherever
/// its harness lets it (`Adapter::places`), with the models above, and a place under a harness's
/// own background service moves a session out of a main checkout that holds other work.
pub fn start_options(board: &Board) -> StartOptions {
    let cfg = config();
    let terminal = crate::launch::terminal_name(&cfg.apps.terminal);
    // The accounts' folders are under `~`, so this only tells an account in its harness's usual
    // folder from one in a folder of its own: nothing is read there.
    let home = crate::util::home();
    let mains: Vec<String> = board.lanes.iter().filter(|l| l.origin == Origin::Main).map(|l| l.path.clone()).collect();
    let accounts: Vec<StartAccount> = cfg
        .accounts
        .iter()
        .map(|a| {
            let places = crate::harness::of(a).map_or_else(|| Ok(Vec::new()), |h| h.places(a, &home));
            let models = models_of(&a.name);
            let mut account = crate::start::account_options(a, places, &terminal, || models.first().map(|m| m.id.clone()), || models.clone());
            account.buffer = cfg.routing.buffer;
            for place in account.places.iter_mut().filter(|p| p.service) {
                place.isolates_in = mains.clone();
            }
            account
        })
        .collect();
    // The lists a device from before each account had its own reads: the first Claude Code
    // account's (`start::options`).
    let claude = accounts.iter().find(|a| a.name == "claude");
    StartOptions { claude_models: claude.map(|a| a.models.clone()).unwrap_or_default(), claude_efforts: claude.map(|a| a.efforts.clone()).unwrap_or_default(), accounts, ..Default::default() }
}

/// The settings as Settings gets them (`config::view`), on a Mac with these terminals, editors
/// and Chrome profiles.
pub fn config_view() -> ConfigView {
    let profile = |id: &str, name: &str| Profile { id: id.into(), name: name.into() };
    let chrome = |name: &str, dir: &str, email: &str| crate::chrome::Profile { id: name.into(), name: name.into(), dir: dir.into(), email: Some(email.into()) };
    ConfigView {
        config: config(),
        text: CONFIG.to_string(),
        path: "~/Library/Application Support/Codealong/config.toml".into(),
        error: None,
        terminals: vec![profile("ghostty", "Ghostty"), profile("iterm", "iTerm2"), profile("terminal", "Terminal")],
        editors: editors(),
        chrome_profiles: vec![chrome("Personal", "Default", "sam@home.example"), chrome("Work", "Profile 1", "sam@northwind.example")],
        review_prompt: crate::autoreview::INSTRUCTIONS.to_string(),
        sounds: ["Pop", "Tink", "Glass", "Ping", "Submarine"].map(String::from).to_vec(),
        default_shortcuts: Default::default(),
        tmux_installed: true,
        holds_sessions: crate::launch::profiles().filter(|(_, t)| t.holds_sessions()).map(|(id, _)| id.to_string()).collect(),
    }
}

/// The skills of the folders asked about and of every account, as the `/` lists get them: two
/// every worktree of Northwind's app has, one of each other repo's, and one of each account's.
pub fn skills(folders: &[String]) -> crate::skills::Listed {
    let cfg = config();
    let harnesses: Vec<Harness> = cfg.accounts.iter().map(|a| Harness::from_config(&a.harness)).collect();
    let skill = |name: &str, description: &str, hint: Option<&str>, typed: bool, uses: u32| crate::skills::Skill {
        name: name.into(),
        description: description.into(),
        hint: hint.map(str::to_string),
        typed,
        by_model: true,
        // As each harness that can be told to run one by name writes it.
        written: harnesses.iter().filter_map(|h| Some((h.config().to_string(), crate::harness::adapter(*h).skill_mention(name)?))).collect::<BTreeMap<_, _>>(),
        uses,
        last_used: 0,
    };
    let of_folder = |folder: &str| {
        if folder.starts_with(APP) {
            vec![skill("ship", "Commit this session's changes, push them and watch the checks", Some("[branch]"), true, 58), skill("house-rules", "How this repo writes its code", None, false, 90)]
        } else if folder.starts_with(CHATS) {
            Vec::new()
        } else {
            vec![skill("release-notes", "Write the release notes from the commits since the last tag", Some("[tag]"), true, 7)]
        }
    };
    crate::skills::Listed {
        folders: folders.iter().map(|f| crate::skills::FolderSkills { folder: f.clone(), skills: of_folder(f) }).collect(),
        accounts: cfg.accounts.iter().map(|a| crate::skills::AccountSkills { account: a.name.clone(), skills: vec![skill("notes:tidy", "Tidy the notes of this conversation", None, true, 2)] }).collect(),
    }
}

/// The installed editors' icons: a plain square each, the size the engine draws an app's at.
pub fn editor_icons() -> Vec<crate::editors::Icon> {
    const SIZE: usize = crate::editors::ICON_PX;
    let colors = [[38_u8, 38, 38, 255], [0, 122, 204, 255], [8, 76, 207, 255]];
    editors().into_iter().zip(colors).map(|(e, color)| crate::editors::Icon { id: e.id, size: SIZE, rgba: color.repeat(SIZE * SIZE) }).collect()
}

/// What a new terminal's shell takes as a command's first word.
pub fn shell_commands() -> Vec<String> {
    ["cargo", "cd", "curl", "export", "gh", "git", "ls", "make", "node", "npm", "npx", "pnpm", "pytest", "python3", "source", "yarn"].map(String::from).to_vec()
}

/// An account on this Mac that config.toml doesn't list, for Settings › Accounts.
pub fn found() -> Vec<crate::detect::Found> {
    vec![crate::detect::Found {
        name: "codex-work".into(),
        harness: "codex".into(),
        home: "~/.codex-work".into(),
        command: Some("CODEX_HOME=~/.codex-work codex".into()),
        via: Some("~/.zshrc".into()),
        state: crate::detect::State::SignedIn,
        signed_in: true,
        folder_exists: true,
        email: Some("sam@northwind.example".into()),
        sign_in_note: None,
        last_used: None,
        new: true,
    }]
}

/// What the setup wizard offers: the accounts listed, the one that isn't, and the folders.
pub fn setup() -> crate::setup::View {
    let listed = accounts().into_iter().map(|a| crate::setup::Account {
        found: crate::detect::Found { name: a.name, harness: a.harness, home: a.home, command: a.command, via: None, state: crate::detect::State::Active, signed_in: true, folder_exists: true, email: a.email, sign_in_note: None, last_used: None, new: false },
        listed: true,
        draws: false,
    });
    let unlisted = found().into_iter().map(|found| crate::setup::Account { found, listed: false, draws: false });
    let folder = |path: &str, repos: usize, worked: usize, scanned: bool| crate::setup::Folder { path: path.into(), exists: true, repos: Some(repos), worked, scanned };
    crate::setup::View { accounts: listed.chain(unlisted).collect(), folders: vec![folder("~/projects", 4, 4, true), folder("~/code", 2, 0, false)] }
}

/// What a session waits on you to allow or answer (`--pending`): a call for one that waits on
/// an approval, its question for one that asks, the message for one that holds another's. None
/// for any other, and for a session that isn't on the board.
pub fn pending(board: &Board, key: &str) -> Option<Pending> {
    let (lane, s) = board.session(key)?;
    let label = s.excerpt.clone().unwrap_or_default();
    match s.need? {
        Need::Approval => Some(Pending {
            id: "call-migrate".into(),
            label,
            input: Some("npm run db:migrate".into()),
            always: true,
            always_rule: Some("Allows `Bash(npm run db:migrate)` in this project, for you.".into()),
            ..Default::default()
        }),
        Need::Question => Some(Pending {
            id: "call-ask".into(),
            label,
            questions: vec![Question {
                question: "Should guests check out without an account?".into(),
                header: Some("Guests".into()),
                options: [("Yes, with an email", "They get a receipt and can make an account after."), ("No, sign in first", "Every order belongs to an account.")]
                    .into_iter()
                    .map(|(label, description)| crate::api::Choice { label: label.into(), description: Some(description.into()) })
                    .collect(),
                multi_select: false,
            }],
            ..Default::default()
        }),
        Need::Message => Some(Pending {
            id: "held-hero".into(),
            label,
            input: Some("The hero links to /contact now: does the form live there yet?".into()),
            message: true,
            from_key: lane.sessions.iter().find(|v| v.visitor.is_some()).map(|v| v.key.clone()),
            ..Default::default()
        }),
        _ => None,
    }
}

/// A session's conversation, newest page (`--transcript`): its prompt, what its agent ran and
/// read, a reply written in Markdown, a message of Codealong's and one of another session's, a
/// turn you stopped, and last the call it waits on you to allow, when it does. None for a
/// session that isn't on the board; with `before`, the page before it, which is empty.
pub fn page(board: &Board, key: &str, before: Option<u64>) -> Option<Page> {
    let (lane, s) = board.session(key)?;
    let item = |kind: &'static str, text: &str| transcript::item(0, kind, text);
    let call = |i: transcript::Item, id: &str, input: &str| transcript::Item { id: Some(id.into()), input: Some(input.into()), ..i };
    let result = |id: &str, output: &str| transcript::Item { id: Some(id.into()), output: Some(output.into()), ..item("result", "") };
    let mut items = vec![
        item("you", &s.title),
        item("agent", "I'll read how it's done today, then make the change."),
        transcript::Item { file: Some(format!("{}/src/index.ts", lane.path)), ..call(item("tool", "Read · `src/index.ts`"), "call-read", "src/index.ts") },
        result("call-read", "export function main() {\n  return start();\n}"),
        transcript::Item { shell: true, cwd: Some(lane.path.clone()), ..call(item("tool", "Bash · `npm test`"), "call-test", "npm test") },
        result("call-test", "Tests  12 passed (12)"),
        item("agent", "The change is in, and the tests pass:\n\n- `src/index.ts` starts the new path behind a flag\n- `src/index.test.ts` covers it\n\n```sh\nnpm test\n```\n\nNext I'd open the PR."),
        item("codealong", "Your helper “Review the invoice export” finished its turn."),
        transcript::Item { from: Some("Redo the home page's hero".into()), ..item("peer", "The hero links to /contact now: does the form live there yet?") },
        transcript::Item { withdrawn: true, ..item("you", "Actually, hold on") },
        transcript::stopped(0),
        item("you", "Go on, and open the PR when it's green."),
        item("agent", s.excerpt.as_deref().filter(|_| s.status != Status::Needs).unwrap_or("On it.")),
    ];
    if let Some(p) = pending(board, key) {
        items.push(transcript::Item { questions: p.questions.clone(), ..call(item("tool", &p.label), &p.id, p.input.as_deref().unwrap_or_default()) });
    }
    // A minute apart, the last when the session was last active, each at a place of its own.
    let count = items.len() as i64;
    for (n, i) in items.iter_mut().enumerate() {
        i.at = s.last_activity - (count - 1 - n as i64) * MIN;
        i.cursor = (n as u64 + 1) * 100;
    }
    let after = (items.len() as u64 + 1) * 100;
    Some(match before {
        None => Page { items, before: None, after },
        Some(_) => Page { items: Vec::new(), before: None, after },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_790_200_000_000;

    /// The checks find what they look for only while the board has it: this holds the fixture
    /// to what it's for.
    #[test]
    fn the_board_has_what_the_checks_look_for() {
        // A Mac, wherever the tests run: its terminals and its shortcuts are macOS's.
        let _mac = crate::os::AsIf::new(crate::os::Os::MacOs);
        let b = board(NOW);
        let lanes: Vec<&Lane> = b.lanes.iter().filter(|l| !l.hidden).collect();
        let all = || b.lanes.iter().flat_map(|l| &l.sessions).filter(|s| s.visitor.is_none());
        // Projects in config.toml's order, then Elsewhere, more than one repo in the first.
        assert_eq!(b.projects.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["Northwind", "Atlas", "Kestrel", ELSEWHERE]);
        assert!(b.projects[0].repos.len() > 1);
        // The head of every order is the first project's, and plain (`sessions`).
        let first = &b.lanes[0];
        assert_eq!((first.project.as_str(), first.sessions[0].need, first.sessions[0].harness, first.sessions[0].surface), ("Northwind", Some(Need::Reply), Harness::Claude, Surface::Cli));
        let mut latest: Vec<(&Lane, &Session)> = lanes.iter().flat_map(|l| l.sessions.iter().map(move |s| (*l, s))).filter(|(_, s)| s.visitor.is_none() && !s.archived).collect();
        let orders: [fn(&Session) -> i64; 2] = [|s| s.last_prompt_at, |s| s.last_activity];
        for by in orders {
            latest.sort_by_key(|(_, s)| std::cmp::Reverse(by(s)));
            assert!(latest[..3].iter().all(|(l, s)| l.project == "Northwind" && !s.unattended()), "{:?}", latest[..3].iter().map(|(_, s)| &s.key).collect::<Vec<_>>());
        }
        assert!(b.prs[..4].iter().all(|p| p.project == "Northwind"), "{:?}", b.prs[..4].iter().map(|p| &p.key).collect::<Vec<_>>());
        // Every bay, and a worktree with no session, one parked, one reusable and free.
        let bays: std::collections::BTreeSet<Bay> = lanes.iter().map(|l| l.bay).collect();
        assert_eq!(bays.len(), 6, "{bays:?}");
        assert!(lanes.iter().any(|l| l.sessions.is_empty()) && lanes.iter().any(|l| l.parked) && lanes.iter().any(|l| l.reuse == Some(Reuse::Free)));
        // Every status, on more than one harness and account, and every way of needing you.
        let statuses: std::collections::BTreeSet<Status> = all().map(|s| s.status).collect();
        assert_eq!(statuses.len(), 6, "{statuses:?}");
        let harnesses: HashSet<Harness> = all().map(|s| s.harness).collect();
        assert_eq!(harnesses.len(), 5, "{harnesses:?}");
        assert!(all().map(|s| s.account.as_str()).collect::<HashSet<_>>().len() >= 5);
        let needs: Vec<Option<Need>> = all().filter(|s| s.status == Status::Needs).map(|s| s.need).collect();
        for need in [Need::Approval, Need::Question, Need::Reply, Need::Message, Need::Stuck, Need::Input, Need::Start] {
            assert!(needs.contains(&Some(need)), "{need:?}");
        }
        // The ways a session is other than one in a terminal tab, and what's laid over one.
        let any = |f: fn(&Session) -> bool| all().any(f);
        assert!(any(|s| s.surface == Surface::App) && any(|s| s.hosted) && any(|s| s.attach_command.is_some()) && any(|s| !s.live));
        assert!(any(|s| s.archived) && any(|s| s.helper.is_some()) && any(|s| s.headless) && any(|s| s.snoozed_until.is_some()) && any(|s| s.raised.is_some()));
        assert!(any(|s| s.limit.is_some()) && any(|s| s.cut_off.is_some()) && any(|s| s.chrome.is_some()) && any(|s| !s.artifacts.is_empty()) && any(|s| s.finished.is_some()));
        assert!(b.lanes.iter().flat_map(|l| &l.sessions).any(|s| s.visitor.is_some()), "a session that worked in another worktree");
        // A PR in each state on its worktree, and ones that wait on your review in two projects.
        let states: Vec<PrState> = lanes.iter().filter_map(|l| l.pr.as_ref().map(|p| p.state)).collect();
        for state in [PrState::Draft, PrState::Open, PrState::Approved, PrState::Changes, PrState::Merged, PrState::Closed] {
            assert!(states.contains(&state), "{state:?}");
        }
        let asked: HashSet<&str> = b.prs.iter().filter(|p| p.waits_on_review()).map(|p| p.project.as_str()).collect();
        assert_eq!(asked.len(), 2, "{asked:?}");
        assert!(b.prs.iter().any(|p| p.mine && !p.sessions.is_empty()));
        // Todos in every standing, some that wait on you, no notice, and usage with limits.
        assert!(b.todos.iter().any(|t| t.status == crate::todos::TodoStatus::Done) && b.todos.iter().any(|t| t.waiting.is_some()) && b.todos.iter().any(|t| !t.sessions.is_empty()));
        assert!(!b.todo_queue.due.is_empty() && !b.todo_queue.ready.is_empty());
        assert!(b.notices.is_empty());
        assert_eq!(b.usage.len(), config().accounts.len());
        assert!(b.usage.iter().any(|u| !u.meters.is_empty()) && b.usage.iter().any(|u| u.no_limits));
    }

    #[test]
    fn everything_counts_back_from_now_under_the_made_up_home() {
        // A Mac, wherever the tests run: its terminals and its shortcuts are macOS's.
        let _mac = crate::os::AsIf::new(crate::os::Os::MacOs);
        // An hour on, every session and PR is as long ago as it was.
        let (b, later) = (board(NOW), board(NOW + HOUR));
        let ages = |b: &Board, now: i64| -> Vec<(String, i64, i64)> { b.lanes.iter().flat_map(|l| &l.sessions).map(|s| (s.key.clone(), now - s.last_activity, now - s.last_prompt_at)).collect() };
        assert_eq!(ages(&b, NOW), ages(&later, NOW + HOUR));
        assert_eq!(b.prs.iter().map(|p| NOW - p.updated_at).collect::<Vec<_>>(), later.prs.iter().map(|p| NOW + HOUR - p.updated_at).collect::<Vec<_>>());
        for lane in &b.lanes {
            assert!(lane.path.starts_with("/Users/me/"), "{}", lane.path);
        }
    }

    #[test]
    fn what_a_session_waits_on_is_in_its_conversation() {
        // A Mac, wherever the tests run: its terminals and its shortcuts are macOS's.
        let _mac = crate::os::AsIf::new(crate::os::Os::MacOs);
        let (b, c) = (board(NOW), Cast::at(NOW));
        let key = |id: &str| c.key("claude", id);
        let waits = pending(&b, &key("checkout-approval")).expect("it waits on a call");
        assert_eq!(waits.label, "Bash · `npm run db:migrate`");
        let conversation = page(&b, &key("checkout-approval"), None).unwrap();
        assert_eq!(conversation.items.last().and_then(|i| i.id.clone()), Some(waits.id));
        assert!(conversation.items.windows(2).all(|w| w[0].cursor < w[1].cursor));
        assert!(page(&b, &key("checkout-approval"), Some(100)).unwrap().items.is_empty());
        assert!(!pending(&b, &c.key("claude2", "checkout-question")).unwrap().questions.is_empty());
        assert!(pending(&b, &key("forms-held")).is_some_and(|p| p.message && p.from_key.is_some()));
        assert_eq!(pending(&b, &key("search-filters")), None);
        assert_eq!(page(&b, "claude:claude:nobody", None), None);
    }

    #[test]
    fn the_rest_of_the_snapshot_is_the_same_macs() {
        // A Mac, wherever the tests run: its terminals and its shortcuts are macOS's.
        let _mac = crate::os::AsIf::new(crate::os::Os::MacOs);
        let b = board(NOW);
        let cfg = config();
        assert_eq!(accounts().iter().map(|a| a.name.clone()).collect::<Vec<_>>(), cfg.accounts.iter().map(|a| a.name.clone()).collect::<Vec<_>>());
        let options = start_options(&b);
        assert!(options.accounts.iter().all(|a| !a.places.is_empty()));
        assert!(options.accounts[0].places.iter().any(|p| !p.isolates_in.is_empty()));
        let view = config_view();
        assert_eq!((view.config, view.error), (cfg, None));
        let listed = skills(&[APP.to_string(), ATLAS.to_string()]);
        assert!(listed.folders[0].skills.iter().any(|s| s.written.len() > 1) && listed.folders[0].skills != listed.folders[1].skills);
        assert_eq!(editor_icons().len(), editors().len());
        assert!(setup().accounts.iter().any(|a| !a.listed));
    }

    /// The settings are read from `CONFIG` alone. Checked as a saved config.toml is, they'd be
    /// held to the sounds and terminals of the machine the tests run on, as if it were a Mac:
    /// a Linux or Windows runner has neither, and every test here would stop at that.
    #[test]
    fn the_settings_are_read_without_asking_the_machine() {
        use crate::os::{as_if, Os};
        for os in [Os::MacOs, Os::Linux, Os::Windows] {
            let cfg = as_if(os, config);
            assert_eq!((cfg.accounts.len(), cfg.projects.len(), cfg.apps.terminal.as_str()), (6, 3, "ghostty"), "{os:?}");
            // And they're ones a config.toml can hold, by the checks that go by the settings alone.
            assert_eq!((config::check_accounts(&cfg.accounts), config::check_routing(&cfg), config::check_projects(&cfg.projects), config::check_repos(&cfg)), (Ok(()), Ok(()), Ok(()), Ok(())), "{os:?}");
        }
    }

    /// The made-up Mac is the same wherever and whenever it's made: nothing in it comes from
    /// the home folder, the temporary folder or the clock of the machine it's made on.
    #[test]
    fn nothing_of_the_machine_its_made_on_is_in_it() {
        // Once more on a machine whose temporary folder is above the made-up worktrees, where a
        // rule that asks the machine (`repos::origin` reads `TMPDIR`) has every one of them made
        // in a temporary folder; under the usual one it says what's said here, and would pass.
        // The environment is the whole process's, so that's a copy of this test binary, as
        // engine.rs's first-scan test runs itself in a home of its own.
        const ABOVE: &str = "/Users/me/projects";
        if std::env::var("TMPDIR").ok().as_deref() != Some(ABOVE) {
            let again = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["fixture::tests::nothing_of_the_machine_its_made_on_is_in_it", "--exact", "--nocapture"])
                .env("TMPDIR", ABOVE)
                .output()
                .unwrap();
            let said = format!("{}{}", String::from_utf8_lossy(&again.stdout), String::from_utf8_lossy(&again.stderr));
            assert!(again.status.success() && said.contains("1 passed"), "with TMPDIR={ABOVE}: {said}");
        }
        let _mac = crate::os::AsIf::new(crate::os::Os::MacOs);
        let b = board(NOW);
        // Who made each worktree is said, not worked out with this machine's `TMPDIR`.
        let made = |path: &str| b.lanes.iter().find(|l| l.path == path).map(|l| l.origin);
        assert_eq!([made(APP), made(CHECKOUT), made(IMPORTS), made(DATES), made(NOTES)], [Origin::Main, Origin::Script, Origin::ClaudeApp, Origin::Codex, Origin::Folder].map(Some));
        // A new terminal opens under the made-up home, whatever the home here is.
        for lane in &b.lanes {
            let at = lane.targets.iter().find(|t| t.id == "l|terminal").and_then(|t| t.detail.as_deref());
            assert!(at.is_some_and(|at| at.starts_with("~/")), "{}: {at:?}", lane.path);
        }
        // The day is `now`'s (months before these tests were written), not the clock's: the todo
        // due today is due that day, and the overdue one two days before it.
        let day = crate::schedules::day_of(NOW, &crate::schedules::rule::Local);
        let due = |id: &str| b.todos.iter().find(|t| t.id == id).and_then(|t| t.due.clone());
        assert_eq!((b.today.clone(), due("t-coupons"), due("t-keys")), (crate::util::day_key(day), Some(crate::util::day_key(day)), Some(crate::util::day_key(day - 2))));
    }

    /// The accounts in their harness's own folder are the ones its app runs (`~/.claude` the
    /// Claude app's, `~/.codex` the Codex app's), so a session of one in a terminal opens in
    /// the app too; one in a folder of its own doesn't.
    #[test]
    fn a_session_of_the_account_an_app_runs_opens_in_the_app_too() {
        let _mac = crate::os::AsIf::new(crate::os::Os::MacOs);
        let (b, c) = (board(NOW), Cast::at(NOW));
        for s in b.lanes.iter().flat_map(|l| &l.sessions) {
            assert_eq!(s.app_account, ["claude", "codex"].contains(&s.account.as_str()), "{}", s.key);
        }
        let ways = |account: &str, id: &str| -> Vec<String> {
            let of = format!("s|{}|", c.key(account, id));
            b.lanes.iter().flat_map(|l| &l.targets).filter(|t| t.id.starts_with(&of)).map(|t| t.label.clone()).collect()
        };
        // Codex in a terminal, on `~/.codex`; and Claude Code in one, on `~/.claude2`.
        assert!(ways("codex", "rate-limits").iter().any(|w| w == "Open in Codex"), "{:?}", ways("codex", "rate-limits"));
        assert!(!ways("claude2", "billing-retries").is_empty() && ways("claude2", "billing-retries").iter().all(|w| !w.contains("Claude app")), "{:?}", ways("claude2", "billing-retries"));
    }

    /// `scripts/checks.sh --fixture` runs the runs scripts/preview/fixture-passes.txt lists, each
    /// named by a line of its check's own header. One whose check was renamed, or whose line
    /// was changed there, would only fail the next time the gate ran: this says so sooner. And
    /// a line that's run by hand on data picked for it (`<key>`) is never one the gate can run.
    #[test]
    fn every_run_the_gate_lists_is_a_line_of_its_checks_header() {
        let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let list = std::fs::read_to_string(repo.join("scripts/preview/fixture-passes.txt")).expect("the gate's list");
        let runs: Vec<&str> = list.lines().filter(|l| !l.is_empty() && !l.starts_with('#')).collect();
        assert!(!runs.is_empty(), "the gate lists no run");
        for (n, run) in runs.iter().enumerate() {
            assert!(!runs[..n].contains(run), "listed twice: {run}");
            assert!(!run.contains('<'), "scripts/preview/fixture-passes.txt lists a run that's made by hand, which the gate can't run: {run}");
            let script = run.rsplit(' ').next().unwrap_or_default();
            let check = std::fs::read_to_string(repo.join(script)).unwrap_or_default();
            let said = check.lines().take_while(|l| l.starts_with("//")).any(|l| l.strip_prefix("//   ") == Some(*run));
            assert!(said, "scripts/preview/fixture-passes.txt lists a run that isn't a line of {script}'s header: {run}");
        }
    }
}
