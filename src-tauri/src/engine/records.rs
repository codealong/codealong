//! What sessions' records show besides the sessions themselves, read off the engine's thread in
//! a scan of every record, and here as each new line is written (`check_new_lines`): which
//! sessions worked in a worktree no session runs in (`Visitors`, by what marks a command run in
//! a folder: `worked_in`), and the PRs each made or changed, the branches it pushed to, the
//! pages on the web it made and the Chrome tabs it drove (`read_logs`). The board gets them as
//! its lanes' visitors, its PRs' links (engine/publish.rs `pr_links`) and its sessions' artifacts
//! and Chrome tabs (engine/sessions.rs).

use super::*;

impl Engine {
    /// Worktrees an agent made from somewhere else (temporary folders, Codex worktrees, a
    /// `git worktree add` next to the checkout) that no session runs in, the text that
    /// identifies each in a log (its folder name, or its path under `~/.codex/worktrees`), and
    /// when it was made.
    fn orphan_needles(&self) -> Vec<(PathBuf, String, i64)> {
        let index = board::SeedIndex::new(&self.seeds);
        // The folders of the lanes sessions run in.
        let lived: HashSet<&Path> = self.session_cwds().iter().filter_map(|c| index.best(Path::new(c))).map(|i| self.seeds[i].path.as_path()).collect();
        let h = home();
        self.seeds
            .iter()
            .filter(|s| matches!(s.origin, Origin::Temp | Origin::Codex | Origin::Script | Origin::Other))
            .filter(|s| !lived.contains(s.path.as_path()))
            .filter_map(|s| {
                let needle = match s.path.strip_prefix(&h) {
                    Ok(rel) if s.origin == Origin::Codex => rel.to_string_lossy().into_owned(),
                    _ => file_name(&s.path),
                };
                let made = std::fs::metadata(&s.path).and_then(|m| m.created()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as i64).unwrap_or(0);
                (needle.len() >= 12).then(|| (s.path.clone(), needle, made))
            })
            .collect()
    }

    /// Searches session logs, subagents' included, for orphan worktrees. A file is read only
    /// for the orphans made before its last write (give or take an hour), in the background
    /// unless `sync`.
    pub(super) fn refresh_visitors(&mut self, sync: bool) {
        self.search_visitors(sync, false)
    }

    /// `refresh_visitors`, and `again` whatever the orphans are: what's been found, and a scan
    /// still out, may be from before lines the readers have since passed over (`pick_up`).
    pub(super) fn search_visitors(&mut self, sync: bool, again: bool) {
        let needles = self.orphan_needles();
        if needles == self.needles && !sync && !again {
            return;
        }
        self.needles = needles.clone();
        if needles.is_empty() {
            self.visitors.clear();
            return;
        }
        let number = self.visitors.begin();
        let files = self.session_logs();
        let scan = move || -> HashMap<PathBuf, Vec<String>> {
            let found = parallel_map(files, |(path, key)| {
                let written = mtime_ms(&path)?;
                let could: Vec<(PathBuf, String)> = needles.iter().filter(|(_, _, made)| written >= made - 3_600_000).map(|(l, n, _)| (l.clone(), n.clone())).collect();
                if could.is_empty() {
                    return None;
                }
                let hits = file_mentions(&path, &could);
                (!hits.is_empty()).then(|| (key, hits))
            });
            // Earliest work first: usually the session that made the worktree.
            let mut timed: HashMap<PathBuf, Vec<(i64, String)>> = HashMap::new();
            for (key, lanes) in found {
                for (lane, at) in lanes {
                    timed.entry(lane).or_default().push((at, key.clone()));
                }
            }
            timed
                .into_iter()
                .map(|(lane, mut v)| {
                    v.sort();
                    (lane, v.into_iter().map(|(_, k)| k).collect())
                })
                .collect()
        };
        if sync {
            self.visitors.scanned(number, scan());
        } else {
            let tx = self.tx.clone();
            std::thread::spawn(move || {
                let _ = tx.send(Msg::Visitors(number, scan()));
            });
        }
    }

    /// Reads the PRs each session's records show it made or changed, the pages on the web it
    /// made (artifacts.rs) and the Chrome tabs it drove, subagents' logs included, in the
    /// background unless `sync`. New lines are read as they come (`check_new_lines`), and records
    /// found after this as they're found (`read_found_logs`). Again after the readers scan again,
    /// which hands no lines on.
    pub(super) fn scan_pr_marks(&mut self, sync: bool) {
        let files = self.session_logs();
        self.logs_read.extend(files.iter().map(|(p, _)| p.clone()));
        if sync {
            let ((prs, pushes), made, used, profiles) = read_logs(files);
            self.pr_marks = prs;
            self.pr_pushes = pushes;
            self.artifacts = made;
            self.take_chrome_use(used, profiles);
        } else {
            self.read_logs_off_thread(files);
        }
    }

    /// Reads records the full scan didn't, as `scan_pr_marks` reads every one: a transcript a
    /// reader came on after it (a copy of a session moved here, one resumed from before the
    /// window) and its subagents' logs so far. Each is read once; what they write after comes as
    /// new lines.
    pub(super) fn read_found_logs(&mut self, found: Vec<(PathBuf, String)>) {
        let files: Vec<(PathBuf, String)> = found.into_iter().filter(|(p, _)| self.logs_read.insert(p.clone())).collect();
        if !files.is_empty() {
            self.read_logs_off_thread(files);
        }
    }

    /// `read_logs` on a thread of its own, its findings sent back to be added to what's known.
    fn read_logs_off_thread(&self, files: Vec<(PathBuf, String)>) {
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let ((prs, pushes), made, used, profiles) = read_logs(files);
            let _ = tx.send(Msg::PrMarks(prs, pushes));
            let _ = tx.send(Msg::Artifacts(made));
            let _ = tx.send(Msg::ChromeUse(used, profiles));
        });
    }

    /// Takes in the full scan's Chrome use. A session's lines read while it ran are newer and
    /// kept, with the group and profile the scan found for it when they don't say.
    pub(super) fn take_chrome_use(&mut self, found: HashMap<String, chrometabs::Use>, profiles: Vec<chrome::Profile>) {
        self.chrome_profiles = profiles;
        for (key, u) in found {
            self.chrome_lookups.insert(key.clone(), (u.group, Instant::now()));
            match self.chrome_use.get_mut(&key) {
                Some(have) if have.at > u.at => {
                    if have.group.is_none() || have.group == u.group {
                        have.group = have.group.or(u.group);
                        have.profile = have.profile.clone().or(u.profile);
                    }
                }
                _ => {
                    self.chrome_use.insert(key, u);
                }
            }
        }
        self.mark_dirty("Chrome use");
    }

    /// Looks up which Chrome profile a session's tabs are in, off the engine's thread: when its
    /// tab group is new, or again a minute on when the extension hadn't written it down yet.
    fn look_up_chrome_profile(&mut self, key: &str) {
        let Some(u) = self.chrome_use.get(key).filter(|u| !u.is_empty()) else { return };
        if let Some((group, at)) = self.chrome_lookups.get(key) {
            if *group == u.group && (u.profile.is_some() || at.elapsed() < Duration::from_secs(60)) {
                return;
            }
        }
        self.chrome_lookups.insert(key.to_string(), (u.group, Instant::now()));
        let (u, key, tx) = (u.clone(), key.to_string(), self.tx.clone());
        std::thread::spawn(move || {
            let dir = chrometabs::Clues::read().profile_of(&u);
            let _ = tx.send(Msg::ChromeProfile(key, u.group, dir, chrome::profiles()));
        });
    }

    /// Checks lines a session just wrote for orphan worktrees and PRs it made or changed.
    pub(super) fn check_new_lines(&mut self, path: &Path, from: u64, to: u64, key: &str) {
        if to <= from {
            return;
        }
        let Some(text) = crate::util::read_range(path, from, to) else { return };
        let source = prlinks::Source::File(path);
        let mut marked = false;
        for (pr, at) in text.lines().flat_map(|line| prlinks::line_marks(line, &source)) {
            let have = self.pr_marks.entry(key.to_string()).or_default();
            marked |= prlinks::add_mark(have, pr, at);
        }
        self.dirty_if(marked, "a PR mark in a line");
        let mut pushed = false;
        for (branch, at) in text.lines().flat_map(|line| prlinks::line_pushes(line, &source)) {
            let have = self.pr_pushes.entry(key.to_string()).or_default();
            pushed |= prlinks::add_push(have, branch, at);
        }
        self.dirty_if(pushed, "a push in a line");
        let mut made = false;
        for a in artifacts::in_text(&text) {
            let have = self.artifacts.entry(key.to_string()).or_default();
            made |= artifacts::add(have, a);
        }
        self.dirty_if(made, "an artifact in a line");
        let mut chrome = self.chrome_use.get(key).cloned().unwrap_or_default();
        if chrometabs::fold_text(&mut chrome, &text) {
            self.chrome_use.insert(key.to_string(), chrome);
            self.look_up_chrome_profile(key);
            self.mark_dirty("Chrome tabs in a line");
        }
        let mut visited = false;
        for (lane, needle, _) in &self.needles {
            visited |= worked_in(&text, needle) && self.visitors.saw(lane, key);
        }
        self.dirty_if(visited, "a visitor in a line");
    }
}

/// Which sessions from other lanes worked in each worktree that no session runs in, by what
/// their logs show: every log is scanned in the background when the worktrees to look for
/// change, and the lines sessions write are checked as they come (`Engine::check_new_lines`).
#[derive(Default)]
pub(super) struct Visitors {
    /// Lane path → sessions from other lanes whose logs show they worked there.
    pub(super) by_lane: HashMap<PathBuf, Vec<String>>,
    /// The number of the latest scan begun. A scan looks for the worktrees there were when it
    /// began, so one that comes back after a later one began is dropped.
    scan: u64,
    /// What new lines have shown since that scan began, until it's back: it may have read a
    /// log before they were written, so they're added to what it found.
    since: Option<HashMap<PathBuf, Vec<String>>>,
}

impl Visitors {
    /// A scan begins, for other worktrees than the last looked for. Its number comes back with
    /// what it finds (`scanned`).
    fn begin(&mut self) -> u64 {
        self.scan += 1;
        self.since = Some(HashMap::new());
        self.scan
    }

    /// There's nothing to look for any more: a scan still out looked for worktrees that were.
    fn clear(&mut self) {
        self.scan += 1;
        self.since = None;
        self.by_lane.clear();
    }

    /// A session's new lines show it worked in a lane. True when that's news.
    fn saw(&mut self, lane: &Path, key: &str) -> bool {
        let add = |keys: &mut Vec<String>| {
            let new = !keys.iter().any(|k| k == key);
            if new {
                keys.push(key.to_string());
            }
            new
        };
        if let Some(since) = &mut self.since {
            add(since.entry(lane.to_path_buf()).or_default());
        }
        add(self.by_lane.entry(lane.to_path_buf()).or_default())
    }

    /// A scan is back with what it found, the earliest work in each lane first. What new lines
    /// showed while it ran comes after. False when a later scan has begun, or there's nothing
    /// to look for any more: this one changes nothing.
    pub(super) fn scanned(&mut self, scan: u64, mut found: HashMap<PathBuf, Vec<String>>) -> bool {
        if scan != self.scan {
            return false;
        }
        for (lane, keys) in self.since.take().unwrap_or_default() {
            let have = found.entry(lane).or_default();
            for key in keys {
                if !have.contains(&key) {
                    have.push(key);
                }
            }
        }
        self.by_lane = found;
        true
    }
}

/// Which lanes a file shows work in, each with when that work first appears.
fn file_mentions(path: &Path, needles: &[(PathBuf, String)]) -> Vec<(PathBuf, i64)> {
    use std::io::Read;
    let Ok(mut f) = std::fs::File::open(path) else { return vec![] };
    let fallback = mtime_ms(path).unwrap_or(0);
    let overlap = needles.iter().map(|(_, n)| n.len()).max().unwrap_or(0);
    let mut hits: Vec<(PathBuf, i64)> = Vec::new();
    let mut carry: Vec<u8> = Vec::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = match f.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        carry.extend_from_slice(&buf[..n]);
        let text = String::from_utf8_lossy(&carry);
        for (lane, needle) in needles {
            if !hits.iter().any(|(l, _)| l == lane) {
                if let Some(line) = work_line(&text, needle) {
                    hits.push((lane.clone(), line_time(line).unwrap_or(fallback)));
                }
            }
        }
        if hits.len() == needles.len() {
            break;
        }
        let keep = carry.len().saturating_sub(overlap);
        carry.drain(..keep);
    }
    hits
}

/// True when some line in `text` shows work in the folder `needle` names: creating it with
/// `git worktree add`, or running commands in it (a `workdir`/`cwd` field, or `cd` into it).
/// A passing mention, like a search or a listing, doesn't count.
pub(crate) fn worked_in(text: &str, needle: &str) -> bool {
    work_line(text, needle).is_some()
}

/// What marks a log line as a tool's output, and what comes right before a folder a command ran
/// in: every harness's own (`Adapter::output_marks`, `Adapter::folder_marks`), and `cd` and `git
/// worktree add`, which say it whatever the harness. Gathered once.
fn record_marks() -> &'static (Vec<&'static str>, Vec<&'static str>) {
    static MARKS: std::sync::OnceLock<(Vec<&'static str>, Vec<&'static str>)> = std::sync::OnceLock::new();
    MARKS.get_or_init(|| {
        let mut output: Vec<&'static str> = harness::all().flat_map(|a| a.output_marks().iter().copied()).collect();
        let mut folder: Vec<&'static str> = harness::all().flat_map(|a| a.folder_marks().iter().copied()).chain(["cd ", "worktree add "]).collect();
        // Two harnesses can share one (`"cwd":"`): a line needn't be looked at twice for it.
        for marks in [&mut output, &mut folder] {
            let mut seen = HashSet::new();
            marks.retain(|m| seen.insert(*m));
        }
        (output, folder)
    })
}

/// The first line in `text` that shows work in the folder `needle` names.
fn work_line<'a>(text: &'a str, needle: &str) -> Option<&'a str> {
    let (tool_output, markers) = record_marks();
    let mut from = 0;
    while let Some(i) = text[from..].find(needle) {
        let at = from + i;
        let line_start = text[..at].rfind('\n').map(|j| j + 1).unwrap_or(0);
        let line_end = text[at..].find('\n').map(|j| at + j).unwrap_or(text.len());
        let line = &text[line_start..line_end];
        // A command's output can name any folder: a listing, another session's log.
        if tool_output.iter().any(|t| line.contains(t)) {
            from = at + needle.len();
            continue;
        }
        for &marker in markers {
            let mut m = 0;
            while let Some(k) = line[m..].find(marker) {
                let path_start = m + k + marker.len();
                let path_end = |c: &char| !matches!(c, '"' | ' ' | '\\' | '&' | ';' | '\'' | '|');
                let path: String = line[path_start..].chars().take_while(path_end).collect();
                let path = path.trim_start_matches("-b ").to_string();
                if names_folder(&path, needle) {
                    return Some(line);
                }
                // `git worktree add -b <branch> <path>`: the path comes after the branch.
                if marker == "worktree add " && line[path_start..].split_whitespace().take(4).any(|w| names_folder(&w.chars().take_while(path_end).collect::<String>(), needle)) {
                    return Some(line);
                }
                m = path_start;
            }
        }
        from = at + needle.len();
    }
    None
}

/// True when `path` is the folder `needle` names or a path inside it, so `codealong-jev`
/// doesn't match `codealong-jevtodos`.
fn names_folder(path: &str, needle: &str) -> bool {
    path.match_indices(needle).any(|(i, _)| {
        let before = path[..i].chars().next_back();
        let after = path[i + needle.len()..].chars().next();
        matches!(before, None | Some('/')) && matches!(after, None | Some('/'))
    })
}

/// The `timestamp` of a log line, if it has one.
fn line_time(line: &str) -> Option<i64> {
    let start = line.find("\"timestamp\":\"")? + "\"timestamp\":\"".len();
    let end = line[start..].find('"')? + start;
    crate::util::parse_rfc3339_ms(&line[start..end])
}

/// The PRs sessions' records show they made or changed and the branches they pushed to, the
/// pages on the web they made and the Chrome tabs they drove, by session key, from one read of
/// each record (`file_marks`), with Chrome's profiles.
fn read_logs(files: Vec<(PathBuf, String)>) -> ((prlinks::Marks, prlinks::Pushes), artifacts::Made, HashMap<String, chrometabs::Use>, Vec<chrome::Profile>) {
    let mut out: prlinks::Marks = HashMap::new();
    let mut pushed: prlinks::Pushes = HashMap::new();
    let mut made: artifacts::Made = HashMap::new();
    let mut used: HashMap<String, chrometabs::Use> = HashMap::new();
    for (key, marks, pushes, pages, chrome) in parallel_map(files, |(path, key)| {
        let (marks, pushes, pages, chrome) = file_marks(&path);
        (!marks.is_empty() || !pushes.is_empty() || !pages.is_empty() || !chrome.is_empty()).then(|| (key, marks, pushes, pages, chrome))
    }) {
        let have = out.entry(key.clone()).or_default();
        for (pr, at) in marks {
            prlinks::add_mark(have, pr, at);
        }
        if !pushes.is_empty() {
            let have = pushed.entry(key.clone()).or_default();
            for (branch, at) in pushes {
                prlinks::add_push(have, branch, at);
            }
        }
        if !pages.is_empty() {
            let have = made.entry(key.clone()).or_default();
            for a in pages {
                artifacts::add(have, a);
            }
        }
        // A session's own log and its subagents' each have their own: the latest counts.
        if !chrome.is_empty() && used.get(&key).map_or(true, |u| chrome.at > u.at) {
            used.insert(key, chrome);
        }
    }
    out.retain(|_, prs| !prs.is_empty());
    // Which profile each session's tabs are in, from one read of the extension's storage.
    if !used.is_empty() {
        let mut clues = chrometabs::Clues::read();
        for u in used.values_mut() {
            u.profile = clues.profile_of(u);
        }
    }
    ((out, pushed), made, used, chrome::profiles())
}

/// A log's PR marks and pushes, the pages on the web it made (in the order its lines name them)
/// and the Chrome tabs it drove, from one read of it.
fn file_marks(path: &Path) -> (Vec<(String, i64)>, Vec<(String, i64)>, Vec<crate::model::Artifact>, chrometabs::Use) {
    let needles: Vec<&str> = prlinks::needles().chain(artifacts::needles()).chain(chrometabs::NEEDLES.iter().copied()).collect();
    let mut marks: Vec<(String, i64)> = Vec::new();
    let mut pushes = Vec::new();
    let mut made = Vec::new();
    let mut chrome = chrometabs::Use::default();
    let source = prlinks::Source::File(path);
    crate::util::each_line_with(path, &needles, |line| {
        for m in prlinks::line_marks(line, &source) {
            if !marks.iter().any(|(k, _)| k == &m.0) {
                marks.push(m);
            }
        }
        pushes.extend(prlinks::line_pushes(line, &source));
        made.extend(artifacts::line_artifact(line));
        chrometabs::fold(&mut chrome, line);
    });
    (marks, pushes, made, chrome)
}

#[cfg(test)]
mod tests {
    use super::worked_in;

    /// The logs are scanned on a thread of its own, which takes a while, and sessions go on
    /// writing meanwhile: the scan's answer doesn't take away what their new lines showed, and
    /// an older scan that comes back late doesn't replace a newer one's.
    #[test]
    fn a_visitors_scan_keeps_what_new_lines_showed_and_an_older_one_is_dropped() {
        use super::*;
        let (a, b) = (PathBuf::from("/tmp/spark-a"), PathBuf::from("/tmp/spark-b"));
        let found = |lane: &PathBuf, keys: &[&str]| HashMap::from([(lane.clone(), keys.iter().map(|k| k.to_string()).collect::<Vec<_>>())]);
        let mut v = Visitors::default();

        // While a scan runs, a session's new lines show it worked in a worktree.
        let first = v.begin();
        assert!(v.saw(&a, "s2"));
        assert!(!v.saw(&a, "s2"), "said once");
        assert!(v.saw(&a, "s1"));
        // The scan read s2's log before those lines were written. What it found comes first,
        // the earliest work in the lane, and what the new lines showed is kept after it.
        assert!(v.scanned(first, found(&a, &["s0", "s1"])));
        assert_eq!(v.by_lane[&a], ["s0", "s1", "s2"]);
        // Lines that come once it's back add to it as before.
        assert!(v.saw(&a, "s3") && !v.saw(&a, "s0"));
        assert_eq!(v.by_lane[&a], ["s0", "s1", "s2", "s3"]);

        // The worktrees to look for change, and again before the scan for the first change is
        // back: that one looked for worktrees that aren't the ones looked for now.
        let second = v.begin();
        let third = v.begin();
        assert!(v.saw(&b, "s4"));
        assert!(!v.scanned(second, found(&a, &["stale"])), "the older scan changes nothing");
        assert_eq!((v.by_lane[&a].len(), v.by_lane[&b].len()), (4, 1));
        assert!(v.scanned(third, found(&b, &["s5"])));
        assert_eq!(v.by_lane.get(&a), None, "what the latest scan didn't look for is gone");
        assert_eq!(v.by_lane[&b], ["s5", "s4"]);

        // Nothing to look for any more: a scan still out is dropped when it's back.
        let fourth = v.begin();
        v.clear();
        assert!(!v.scanned(fourth, found(&a, &["late"])));
        assert!(v.by_lane.is_empty());
    }

    #[test]
    fn counts_real_work_not_mentions() {
        let n = "spark-validate-ghostscript-20260923";
        assert!(worked_in(r#"{"cmd":"git worktree add /tmp/spark-validate-ghostscript-20260923 origin/main"}"#, n));
        assert!(worked_in(r#"{"cmd":"git worktree add -b codex/x /tmp/spark-validate-ghostscript-20260923"}"#, n));
        assert!(worked_in(r#"{"arguments":"{\"cmd\":\"yarn test\",\"workdir\":\"/private/tmp/spark-validate-ghostscript-20260923\"}"}"#, n));
        assert!(worked_in(r#"{"command":"cd /private/tmp/spark-validate-ghostscript-20260923 && git status"}"#, n));
        assert!(!worked_in(r#"{"command":"cd ~/projects/codealong && grep -F spark-validate-ghostscript-20260923 logs"}"#, n));
        assert!(!worked_in(r#"{"content":"/private/tmp/spark-validate-ghostscript-20260923  aad4df0 [codex/x]"}"#, n));
        assert_eq!(super::line_time(r#"{"timestamp":"1970-01-01T00:00:01.000Z","cmd":"x"}"#), Some(1000));
    }

    /// The marks the harnesses hand the engine are the ones it looked for when it knew them
    /// itself: Claude Code's tool results and line folders, Codex's call outputs, workdirs and
    /// folders, and the shell's own.
    #[test]
    fn looks_for_every_harnesss_marks_and_the_shells() {
        let (output, folder) = super::record_marks();
        let sorted = |v: &[&'static str]| {
            let mut v = v.to_vec();
            v.sort();
            v
        };
        assert_eq!(sorted(output), sorted(&["\"type\":\"tool_result\"", "\"type\":\"function_call_output\"", "\"type\":\"custom_tool_call_output\"", "\"type\":\"GENERIC\"", "\"type\":\"SYSTEM_MESSAGE\""]));
        assert_eq!(sorted(folder), sorted(&["\"workdir\":\"", "\\\"workdir\\\":\\\"", "\"cwd\":\"", "\\\"cwd\\\":\\\"", "\"Cwd\":\"", "\\\"Cwd\\\":\\\"", "cd ", "worktree add "]));
    }

    #[test]
    fn a_worktree_next_to_the_checkout_counts_only_by_its_whole_name() {
        let n = "spark-org-redis-keys";
        assert!(worked_in(r#"{"command":"cd /Users/me/projects/spark && git worktree add -q -b me/org-redis-keys ../spark-org-redis-keys origin/main"}"#, n));
        assert!(worked_in(r#"{"command":"cd /Users/me/projects/spark-org-redis-keys/apps/server && npx vitest run"}"#, n));
        assert!(!worked_in(r#"{"command":"cd /Users/me/projects/spark-org-redis-keys-v2 && git status"}"#, n));
        assert!(!worked_in(r#"{"command":"cd /Users/me/projects/old-spark-org-redis-keys && git status"}"#, n));
        assert!(!worked_in(r#"{"type":"user","message":{"content":[{"type":"tool_result","content":"Bash cd /Users/me/projects/spark-org-redis-keys && ls"}]}}"#, n));
    }
}
