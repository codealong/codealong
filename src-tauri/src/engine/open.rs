//! Opening things where they live, as a click in a window does: a lane's target (a session in
//! its terminal or its app, a folder in your editor or a new terminal, a PR on GitHub), a web
//! link in the browser profile it belongs in, config.toml and a repo's team file in your editor.
//! Free functions over a board read once, with nothing of the engine's thread in them: the app's
//! commands call them on its copy of the board (lib.rs), `Handle::open` on the engine's, and
//! `--snapshot --open` and `--link` on a board read once.

use super::*;

/// Your editor (`[apps] editor`), as the board names it: Cursor, `config.rs`'s default, until
/// the board has one.
pub fn your_editor(b: &Board) -> config::Profile {
    if b.apps.editor_id.is_empty() {
        return config::Profile { id: "cursor".into(), name: editors::name("cursor") };
    }
    config::Profile { id: b.apps.editor_id.clone(), name: b.apps.editor.clone() }
}

/// Opens config.toml in your editor.
pub fn open_config(b: &Board) -> Result<()> {
    editors::open(&your_editor(b).id, &config::config_path())
}

/// Opens repo `slug`'s team file as its main checkout here has it, in your editor.
pub fn open_team_file(b: &Board, slug: &str) -> Result<()> {
    let lanes = b.lanes.iter().filter(|l| l.device.is_none() && l.repo_slug.as_deref() == Some(slug));
    let root = lanes.clone().find(|l| l.origin == Origin::Main).or_else(|| lanes.clone().next()).map(|l| PathBuf::from(&l.path));
    let root = root.ok_or_else(|| anyhow!("There's no checkout of {slug} on this machine."))?;
    let file = root.join(teamfile::FILE);
    if !file.is_file() {
        bail!("{} has no {}.", tilde(&root), teamfile::FILE);
    }
    editors::open(&your_editor(b).id, &file)
}

/// Opens a web link where `link_opens` says. A profile Chrome no longer has opens the link in
/// the default browser rather than not at all: a link's click has nowhere to say why, and the
/// preview's own Open and `codealong config check` say it.
pub fn open_link(b: &Board, url: &str) -> Result<()> {
    let accounts = config::current().map(|c| c.accounts).unwrap_or_default();
    let opened = match link_opens(b, &accounts, url) {
        LinkOpens::Preview(lane) => open_as_preview(url, &lane),
        LinkOpens::Account { account, profile } => account_profile(&account, &profile).and_then(|p| chrome::open(url, &p)),
        LinkOpens::Default => return launch::open_url(url),
    };
    opened.or_else(|e| {
        eprintln!("Opening {url} in the default browser: {e:#}");
        launch::open_url(url)
    })
}

/// Where a click on a web link opens it (`open_link`), by what it's a link to.
pub enum LinkOpens {
    /// Into a worktree's app whose preview opens in a Chrome profile: there, so the app has that
    /// profile's sign-in (`board::app_of_link`).
    Preview(Lane),
    /// A page a session made, whose account names a Chrome profile (`AccountCfg::chrome_profile`):
    /// there, where you're signed in to that account and can see it (`board::sessions_with_page`).
    Account { account: String, profile: String },
    /// Anything else: the default browser.
    Default,
}

/// Where a click on `url` opens it. When sessions on more than one account made the page, the
/// one that last did something with it and has a profile decides.
pub fn link_opens(board: &Board, accounts: &[config::AccountCfg], url: &str) -> LinkOpens {
    if let Some(lane) = board::app_of_link(&board.lanes, url) {
        return LinkOpens::Preview(lane.clone());
    }
    let profile_of = |name: &str| accounts.iter().find(|a| a.name == name)?.chrome_profile.as_deref().map(str::trim).filter(|p| !p.is_empty()).map(str::to_string);
    board::sessions_with_page(&board.lanes, url).into_iter().find_map(|s| Some(LinkOpens::Account { account: s.account.clone(), profile: profile_of(&s.account)? })).unwrap_or(LinkOpens::Default)
}

/// The Chrome profile an account names, or why Chrome has none by that name.
pub fn account_profile(account: &str, name: &str) -> Result<chrome::Profile> {
    chrome::resolve(name).map_err(|e| anyhow!("{e} It's set for {account} in config.toml."))
}

/// Opens a lane's target where it lives (`open_on`), and keeps an editor picked from an Open in
/// menu as yours. The session it opened, when it opened one that's now seen: where it lives shows
/// nothing of what a side conversation answered, which goes on waiting for a window of
/// Codealong's to show it. See *Surfaces*, Open where it lives.
pub fn open_here(board: &Board, lane_id: &str, target_id: &str) -> Result<Option<String>> {
    let opened = open_on(board, lane_id, target_id)?;
    // Another editor picked from an Open in menu is the one Open in <editor> uses from then on, as
    // Codex's Open button remembers its last. The watcher publishes the change.
    if let Some(id) = target_id.strip_prefix("l|editor|").filter(|id| *id != board.apps.editor_id) {
        if let Err(e) = config::save_editor(id) {
            eprintln!("Couldn't save the editor: {e:#}");
        }
    }
    Ok(opened.filter(|key| !board.session(key).is_some_and(|(_, s)| s.side_answer.is_some())))
}

/// Opens a lane's target on `board`. Returns the key of the session it opened, if any.
pub fn open_on(board: &Board, lane_id: &str, target_id: &str) -> Result<Option<String>> {
    open_with(board, &config::current()?, lane_id, target_id)
}

fn open_with(board: &Board, cfg: &config::Config, lane_id: &str, target_id: &str) -> Result<Option<String>> {
    let lane = board.lanes.iter().find(|l| l.id == lane_id).cloned().ok_or_else(|| anyhow!("That lane just changed. Try again."))?;
    let apps = board.apps.clone();
    let path = PathBuf::from(&lane.path);
    let near = board::terminal_neighbours(&board.lanes, &path);
    let place = launch::Placement { new_terminal: apps.new_terminal, near: &near, behind: false, label: None };
    let parts: Vec<&str> = target_id.splitn(3, '|').collect();
    match parts.as_slice() {
        ["s", key, action] => {
            let s = lane.sessions.iter().find(|s| s.key == *key).ok_or_else(|| anyhow!("That session is gone."))?;
            let cwd = if Path::new(&s.cwd).is_dir() { PathBuf::from(&s.cwd) } else { path.clone() };
            // The session's own tab says which worktree it's in; a plain terminal keeps its shell's titles.
            let label = board::tab_label(&board.lanes, &path, &s.title);
            let named = launch::Placement { label: label.as_deref(), ..place };
            match *action {
                "focus" => {
                    let pid = s.pid.ok_or_else(|| anyhow!("That session isn't running any more."))?;
                    // No tab shows it (a tmux session nothing is attached to): a new one does.
                    if let Some(cmd) = launch::attach_command(pid) {
                        launch::terminal_open(&apps.terminal_id, named, &cwd, Some(&format!("{cmd}\n")))?;
                    } else if !launch::terminal_focus_session(&apps.terminal_id, pid, &s.title, &cwd)? {
                        // It's looked for in the app it runs under, whatever config.toml names.
                        let app = launch::terminal_kind(pid).filter(|k| k.bundle().is_some()).map_or(apps.terminal.as_str(), |k| k.name());
                        bail!("Couldn't find {}'s terminal in {app}. It's running as pid {pid}.", s.account);
                    }
                }
                "attach" => {
                    let cmd = s.attach_command.clone().ok_or_else(|| anyhow!("This session isn't a background job."))?;
                    launch::terminal_open(&apps.terminal_id, named, &cwd, Some(&format!("{cmd}\n")))?;
                }
                "resume" => {
                    let account = harness::find(board, cfg, key)?.1;
                    let cmd = harness::adapter(s.harness).resume_typed(account, s).ok_or_else(|| anyhow!("This session can't be resumed in a terminal."))?;
                    // Going on starts it again, which nothing does on an account turned off.
                    account.ensure_on()?;
                    // A click opens it in front, so the one thing to say is a tab that didn't open.
                    if let ran @ launch::Ran::InTmux(launch::Shown::Not(_)) = launch::session_run(&apps.terminal_id, cfg.apps.tmux, named, &cwd, &format!("{cmd}\n"), &[])? {
                        bail!(ran.note(&apps.terminal).unwrap_or_default());
                    }
                }
                // A new terminal in the folder it works in: split from its own tab where the
                // terminal can (Rex), else beside the terminals already there (`place`).
                "terminal" => {
                    let folder = PathBuf::from(board::session_folder(&lane.path, &s.cwd));
                    launch::terminal_open_beside(&apps.terminal_id, place, s.pid, if folder.is_dir() { &folder } else { &path })?;
                    return Ok(None);
                }
                "chrome" => chrometabs::show(s.chrome.as_ref().ok_or_else(|| anyhow!("That session hasn't used Chrome."))?)?,
                // A window shows it in its own panel; nothing opens elsewhere.
                "hosted" => {}
                // A way of its harness's own: its app.
                other => harness::adapter(s.harness).open(s, other)?,
            }
            return Ok(Some(s.key.clone()));
        }
        // A start whose session isn't here yet: the tab its harness waits in, found by its
        // process as its screen was read (starting.rs), to answer what it shows there.
        ["start", at, "focus"] => {
            let s = at.parse().ok().and_then(|at| crate::starting::find(board, at)).ok_or_else(|| anyhow!("It has made its session, or was stopped."))?;
            let pid = s.pid.ok_or_else(|| anyhow!("Codealong hasn't found its terminal yet."))?;
            if let Some(cmd) = launch::attach_command(pid) {
                launch::terminal_open(&apps.terminal_id, place, &path, Some(&format!("{cmd}\n")))?;
            } else if !launch::terminal_focus_session(&apps.terminal_id, pid, s.opens.as_deref().unwrap_or_default(), &path)? {
                let app = launch::terminal_kind(pid).filter(|k| k.bundle().is_some()).map_or(apps.terminal.as_str(), |k| k.name());
                bail!("Couldn't find {}'s terminal in {app}. It's running as pid {pid}.", s.harness);
            }
        }
        ["l", "terminal"] => launch::terminal_open(&apps.terminal_id, place, &path, None)?,
        ["l", "editor"] => editors::open(&apps.editor_id, &path)?,
        ["l", "editor", id] if opens_in(&apps, id) => editors::open(id, &path)?,
        ["l", "finder"] => launch::reveal(&path)?,
        ["l", "preview"] => {
            let preview = lane.preview.as_ref().ok_or_else(|| anyhow!("This worktree has no preview URL."))?;
            // Only a web page: the URL was checked when it was set, and a found one is localhost.
            if !preview.url.starts_with("http://") && !preview.url.starts_with("https://") {
                bail!("{} isn't a web address.", preview.url);
            }
            open_as_preview(&preview.url, &lane)?
        }
        ["p", which] => {
            let pr = lane.pr.as_ref().ok_or_else(|| anyhow!("This lane has no PR."))?;
            match *which {
                "github" => launch::open_url(&pr.url)?,
                "codex-review" => launch::open_codex(&launch::codex_review_url(&pr.url))?,
                other => bail!("Unknown PR action {other:?}"),
            }
        }
        _ => bail!("Unknown target {target_id:?}"),
    }
    Ok(None)
}

/// Whether a target may open a folder in the editor `id`: yours, or another found installed, not
/// any app a target could name.
fn opens_in(apps: &AppNames, id: &str) -> bool {
    id == apps.editor_id || apps.editors.iter().any(|e| e.id == id)
}

/// Opens `url` where `lane`'s preview opens: in its Chrome profile, or with none, the default
/// browser.
fn open_as_preview(url: &str, lane: &Lane) -> Result<()> {
    let Some((preview, name)) = lane.preview.as_ref().and_then(|p| Some((p, p.chrome_profile.as_ref()?))) else {
        return launch::open_url(url);
    };
    let whose = if preview.chrome_profile_set { "this worktree".to_string() } else { format!("{} in config.toml", lane.project) };
    let profile = chrome::resolve(name).map_err(|e| anyhow!("{e} It's set for {whose}."))?;
    chrome::open(url, &profile)
}

#[cfg(test)]
mod tests {
    /// Resume in a terminal starts a session again, so an account turned off refuses it before
    /// any terminal opens.
    #[test]
    fn resume_from_open_refuses_an_account_turned_off() {
        use super::*;
        let stopped = crate::model::Session { live: false, resume_command: Some("claude --resume stopped".into()), ..crate::fixtures::session("stopped") };
        let board = Board { lanes: vec![crate::fixtures::lane("/r", vec![stopped])], ..Board::default() };
        let off = config::AccountCfg { name: "claude".into(), harness: "claude".into(), home: "~/.claude".into(), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: true, chrome_profile: None };
        let cfg = config::Config { accounts: vec![off], ..config::Config::default() };
        let e = open_with(&board, &cfg, "/r", "s|stopped|resume").unwrap_err().to_string();
        assert_eq!(e, "claude is turned off in Settings › Accounts.");
    }

    /// A page a session made opens in the Chrome profile its account names: the latest session's
    /// that names one, past a later one on an account that doesn't. Other links, and pages on
    /// accounts without one, open in the default browser.
    #[test]
    fn a_page_opens_in_its_accounts_chrome_profile() {
        use super::*;
        let made = |at: i64| crate::model::Artifact { url: "https://claude.ai/artifact/Pq7".into(), title: "Roles".into(), note: None, at };
        let on = |key: &str, account: &str, at: i64| crate::model::Session { account: account.into(), artifacts: vec![made(at)], ..crate::fixtures::session(key) };
        let board = Board { lanes: vec![crate::fixtures::lane("/r", vec![on("claude:claude:a", "claude", 20), on("claude:claude2:b", "claude2", 10)])], ..Board::default() };
        let account = |name: &str, profile: Option<&str>| config::AccountCfg { name: name.into(), harness: "claude".into(), home: format!("~/.{name}"), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false, chrome_profile: profile.map(String::from) };
        let says = |accounts: &[config::AccountCfg], url: &str| match link_opens(&board, accounts, url) {
            LinkOpens::Preview(lane) => format!("preview {}", lane.id),
            LinkOpens::Account { account, profile } => format!("{account} in {profile}"),
            LinkOpens::Default => "default".into(),
        };
        assert_eq!(says(&[account("claude", None), account("claude2", Some("Work"))], "https://claude.ai/artifact/Pq7"), "claude2 in Work");
        assert_eq!(says(&[account("claude", Some("Personal")), account("claude2", Some("Work"))], "https://claude.ai/artifact/Pq7"), "claude in Personal");
        assert_eq!(says(&[account("claude", Some(" ")), account("claude2", None)], "https://claude.ai/artifact/Pq7"), "default");
        assert_eq!(says(&[account("claude2", Some("Work"))], "https://example.com/elsewhere"), "default");
    }

    #[test]
    fn opens_folders_in_your_editor_or_one_installed() {
        use super::*;
        let apps = AppNames { editor_id: "cursor".into(), editors: vec![config::Profile { id: "zed".into(), name: "Zed".into() }], ..Default::default() };
        assert!(opens_in(&apps, "cursor"));
        assert!(opens_in(&apps, "zed"));
        assert!(!opens_in(&apps, "Terminal"));
    }
}
