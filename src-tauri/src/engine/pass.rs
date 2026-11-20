//! An engine for one pass: everything read once, with no watcher and nothing asked for an answer
//! later, for `--snapshot` (snapshot.rs) and for the `codealong` command when no server answers
//! (`light_snapshot`). Its caller's thread runs the pass and waits for each step, but not all the
//! reading is on that thread: git in every worktree, the records and the team files are read on
//! a few threads at once (`parallel_map`), and the repo scan's reads of folders it doesn't own
//! are each on a thread of its own (held.rs), where one macOS holds goes on after the scan has
//! gone on without it. Such an engine is `passing`: it's gone before what it would ask in the
//! background came back.

use super::*;

/// One synchronous pass for `--snapshot`: everything the board needs, no watchers. Claude usage
/// is read only when asked: the usage endpoint rate-limits, and the app polls it already.
pub fn snapshot(with_github: bool, with_usage: bool) -> Board {
    snapshot_engine(with_github, with_usage).compose()
}

/// `--snapshot --pr-card`: a PR as a link to it gets it when the board doesn't list it.
pub fn snapshot_pr_card(slug: &str, number: u64) -> Result<PrItem> {
    let (raw, login) = github::one(slug, number)?;
    let e = snapshot_engine(false, false);
    let lanes = e.compose().lanes;
    Ok(e.pr_card(&lanes, slug, &raw, login.as_deref()))
}

pub(super) fn snapshot_engine(with_github: bool, with_usage: bool) -> Engine {
    let (tx, _rx) = channel();
    let mut e = Engine::new(Box::new(|_| {}), tx, Arc::new(RwLock::new(Board::default())));
    e.passing = true;
    // Which worktrees are at a merged PR's last commit, as the server's engine last heard.
    e.github.read_heads_from(&github::heads_file());
    let t = Instant::now();
    let lap = |label: &str, t: &Instant| eprintln!("  {label:<10} {:>6} ms", t.elapsed().as_millis());
    e.load_config();
    e.load_state();
    e.load_todos();
    lap("config", &t);
    e.scan_sessions();
    lap("sessions", &t);
    e.load_repos();
    lap("repos", &t);
    e.refresh_visitors(true);
    lap("visitors", &t);
    e.load_pr_links();
    e.load_reviews();
    e.scan_pr_marks(true);
    lap("pr marks", &t);
    e.load_previews();
    e.load_reusable();
    e.previews.servers = previews::scan_once(&e.server_roots());
    lap("previews", &t);
    e.load.on_tick(now_ms(), crate::os::system::system);
    e.loading = false;
    let _ = e.compose();
    lap("1st paint", &t);
    let jobs: Vec<(PathBuf, Option<String>)> = e.git_targets().into_iter().map(|p| (p.clone(), e.base_for(&p))).collect();
    for (p, info) in parallel_map(jobs, |(p, base)| Some((p.clone(), git::status(&p, base.as_deref())))) {
        e.git.insert(p, info);
    }
    lap("git", &t);
    if with_usage {
        for u in usage::read_all(&e.cfg.accounts) {
            e.usage.insert(u.account.clone(), u);
        }
        lap("usage", &t);
    }
    if with_github {
        let slugs: Vec<String> = board::polled_slugs(&e.seeds).into_iter().collect();
        e.github.set_marked(board::worked_prs(&e.pr_links(), &e.compose().lanes));
        let made = e.github.made_to_read(&e.seeds);
        for answer in github::read_once(&slugs, &made) {
            e.github.take(answer, &e.seeds, Now::real());
        }
        e.github.took_all(slugs);
        lap("github", &t);
    }
    let answers = parallel_map(e.team_file_asks(), |ask| Some(teamfile::read(&ask)));
    e.team_files.on_read(Some(answers));
    lap("team files", &t);
    e
}

/// The board without git, GitHub, visitors or usage: sessions, worktrees and todos as the
/// files say, in a couple of seconds. For the `codealong` command when no server answers.
pub fn light_snapshot() -> Board {
    let (tx, _rx) = channel();
    let mut e = Engine::new(Box::new(|_| {}), tx, Arc::new(RwLock::new(Board::default())));
    e.passing = true;
    e.load_config();
    e.load_state();
    e.load_todos();
    e.scan_sessions();
    e.load_repos();
    e.load_previews();
    e.load_reusable();
    e.previews.servers = previews::scan_once(&e.server_roots());
    e.loading = false;
    e.compose()
}
