//! PRs: the one a lane's branch has (`find_pr`), every PR of the repos a project covers with
//! the worktrees that hold it and the sessions that worked on it (`pr_items`, and `pr_card`
//! for one the board doesn't list), which of its sessions is its primary, whose move an open
//! PR is by what was said on it (`turn_of`), and the reviews you snoozed.
//!
//! Pure functions over PRs as GitHub gave them (`github::PrRaw`, which ghpoll.rs keeps read)
//! and the sessions tied to them other than by branch (`PrLinks`, from prlinks.rs): nothing
//! here asks GitHub. A rule's test goes in `tests` below.

use super::*;

/// Whether a checkout's remotes (`LaneSeed::head_owners`) include a PR's head owner. GitHub
/// names owners in its own case, and a remote's URL in whatever case it was typed.
fn has_owner(owners: &[String], owner: &str) -> bool {
    owners.iter().any(|o| o.eq_ignore_ascii_case(owner))
}

/// The PR for a lane: an open one on its branch, else one merged or closed in the last
/// seven days whose head is still the lane's HEAD.
pub fn find_pr(seed: &LaneSeed, prs: &HashMap<String, Vec<PrRaw>>, now: i64) -> Option<Pr> {
    let slug = seed.repo_slug.as_ref()?;
    let branch = seed.branch.as_ref()?;
    if seed.default_branch.as_ref() == Some(branch) {
        return None;
    }
    let list = prs.get(slug)?;
    let ours = |p: &&PrRaw| &p.head_ref == branch && p.head_owner.as_ref().map_or(true, |o| has_owner(&seed.head_owners, o));
    let open = list.iter().filter(ours).filter(|p| p.state.is_open()).max_by_key(|p| p.updated_at);
    let pick = open.or_else(|| {
        list.iter()
            .filter(ours)
            .filter(|p| !p.state.is_open())
            .filter(|p| p.merged_at.or(p.closed_at).map(|t| now - t < 7 * DAY_MS).unwrap_or(false))
            .filter(|p| match (&p.head_oid, &seed.head) {
                (Some(pr_head), Some(lane_head)) => pr_head == lane_head,
                _ => true,
            })
            .max_by_key(|p| p.merged_at.or(p.closed_at))
    })?;
    Some(Pr {
        number: pick.number,
        title: pick.title.clone(),
        url: pick.url.clone(),
        state: pick.state,
        checks: pick.checks.clone(),
        merge: pick.merge,
        updated_at: pick.updated_at,
        merged_at: pick.merged_at,
        closed_at: pick.closed_at,
        stale_approval: pick.stale_approval.clone(),
    })
}

/// Every PR in a repo a project covers, with the worktrees that hold it and the sessions that
/// worked on it.
///
/// A worktree holds the PR its heading in Sessions shows (`find_pr`), so a long-lived branch
/// that carries one PR after another holds only the latest.
///
/// A session worked on the PR if it was on the PR's branch while the PR was open: active after
/// the PR opened (give or take an hour) and started before it merged or closed. A session
/// placed in a worktree it doesn't run in (`session_seed`) goes by that worktree's branch. A session from
/// another lane records its own folder's branch, so it counts through a worktree on the PR's
/// branch that it made or worked in: the one that made it whenever it started before the PR
/// ended, since it wrote the branch even if the PR was opened later, and one that worked there
/// in the same window as the rest. A PR from the default branch (a fork's `main`) links no
/// sessions by branch: every session on `main` would match. Nor does a PR from a fork the
/// worktree's checkout has no remote for, to it or its sessions' pushes: its branch is another
/// of the same name (someone's `app3` beside yours), as `find_pr` has it.
///
/// So do sessions in the repo whose records show they pushed to the PR's branch while they
/// worked in the PR's time (`links.pushed`): one in a worktree on a branch of its own that
/// pushes it to the PR's (`git push origin local:<head>`) has none of the rest. A branch's name
/// says nothing of its repo, so only sessions in the PR's repo count.
///
/// Then come the sessions whose records show they made or changed the PR, or that were linked
/// to it by hand (`links.worked`), whatever their branch or time, each under the lane it's in.
/// A session unlinked by hand is left off. One of them is the PR's primary (`pr_primary`).
///
/// A repo whose PRs aren't read (`polled_slugs`) lists the PRs GitHub's search says wait on your
/// review (`requested`), and the search has the last word on reviews there: a PR it no longer
/// lists doesn't wait on you, though an earlier read of the repo said so. A repo whose PRs are
/// read keeps its read's word on the PRs it lists, and lists the search's it doesn't: past the
/// hundred most recently updated that a read holds, in a busy repo, or any while no read of it
/// has come (its first not yet, or every one failing).
///
/// Then the PRs sessions made or changed in repos whose PRs aren't read, read one by one
/// (`made`, `worked_prs`), so a PR an agent opened anywhere is listed with it: in its project's
/// repo where one covers it, else under the project of the session it's for, the primary's, or
/// the first that's on the board (`made_item`).
pub fn pr_items(seeds: &[LaneSeed], lanes: &[Lane], prs: &HashMap<String, Vec<PrRaw>>, requested: &HashMap<String, Vec<PrRaw>>, made: &HashMap<String, Vec<PrRaw>>, login: Option<&str>, links: &PrLinks) -> Vec<PrItem> {
    let repos = repos_of(seeds);
    let owners = owners_of(seeds);
    let polled = polled_slugs(seeds);
    let read_alone = read_alone(made);
    let mut out = Vec::new();
    for (slug, list) in prs {
        let Some(&repo) = repos.get(slug.as_str()) else { continue };
        let searched = (!polled.contains(slug)).then(|| requested.get(slug).map_or(&[][..], Vec::as_slice));
        for p in list {
            // The search's copy is the newer, and so is one read by its number: a repo no longer
            // read keeps its last read.
            if searched.is_some_and(|found| found.iter().any(|x| x.number == p.number) || read_alone.contains(&crate::prlinks::pr_key(&format!("{slug}#{}", p.number)))) {
                continue;
            }
            let mut item = pr_item(lanes, &owners, slug, repo, p, branch_freed(list, p), login, links);
            if searched.is_some() {
                item.review_requested = false;
                // Nor is its last read's word on a merge still good.
                item.waits_to_merge = false;
                item.requested_by = None;
                item.requested_at = None;
            }
            out.push(item);
        }
    }
    for (slug, list) in requested {
        let Some(&repo) = repos.get(slug.as_str()) else { continue };
        let read = prs.get(slug).filter(|_| polled.contains(slug));
        let unread = |p: &&PrRaw| !read.is_some_and(|read| read.iter().any(|x| x.number == p.number));
        // The search lists only open PRs; the repo's last read has the ones that ended.
        let history = || prs.get(slug).into_iter().flatten().chain(list);
        out.extend(list.iter().filter(unread).map(|p| pr_item(lanes, &owners, slug, repo, p, branch_freed(history(), p), login, links)));
    }
    let listed: HashSet<String> = out.iter().map(|p| crate::prlinks::pr_key(&p.key)).collect();
    for (slug, list) in made {
        for p in list.iter().filter(|p| !listed.contains(&crate::prlinks::pr_key(&format!("{slug}#{}", p.number)))) {
            out.extend(made_item(&repos, &owners, lanes, slug, p, login, links));
        }
    }
    out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    out
}

/// A PR sessions made in a repo whose PRs aren't read (`pr_items`): in the project whose repo it's
/// in, by its name in any case, else under the project of the session it's for. None when no
/// session it's for is on the board, which leaves it no project.
fn made_item(repos: &HashMap<&str, (&str, &str, Option<&str>)>, owners: &HashMap<&Path, &[String]>, lanes: &[Lane], slug: &str, p: &PrRaw, login: Option<&str>, links: &PrLinks) -> Option<PrItem> {
    if let Some((&local, &repo)) = repos.iter().find(|(s, _)| s.eq_ignore_ascii_case(slug)) {
        return Some(pr_item(lanes, owners, local, repo, p, None, login, links));
    }
    let mut item = pr_item(lanes, owners, slug, ("", slug.rsplit('/').next().unwrap_or(slug), None), p, None, login, links);
    let key = crate::prlinks::pr_key(&item.key);
    let worked = links.worked.get(&key).into_iter().flatten().filter(|k| !links.unlinked.contains(&(key.clone(), (*k).clone())));
    let project = item.primary.iter().chain(item.sessions.iter().map(|s| &s.key)).chain(worked).find_map(|k| home(lanes, k)).map(|(l, _)| l.project.clone())?;
    item.project = project;
    Some(item)
}

/// The PRs sessions on the board (`lanes`) made or changed by their records, or were linked to by
/// hand, as `owner/name#number`, the latest marked or linked first. Records outlast the sessions
/// the board lists (a background job removed, say), and a session unlinked from a PR counts for
/// none. The engine reads the ones in repos whose PRs aren't read by their numbers (ghpoll.rs
/// `made_to_read`), which are listed only with such a session (`made_item`). Links aren't put
/// first: every PR an automatic review was started on has one, hundreds of them a month in a busy
/// repo, and put first they kept the latest PRs from being read.
pub fn worked_prs(links: &PrLinks, lanes: &[Lane]) -> Vec<String> {
    let on_board: HashSet<&str> = lanes.iter().flat_map(|l| l.sessions.iter().map(|s| s.key.as_str())).collect();
    let mut prs: Vec<(Reverse<i64>, &String)> = links
        .worked
        .iter()
        .filter(|(pr, sessions)| sessions.iter().any(|s| on_board.contains(s.as_str()) && !links.unlinked.contains(&((*pr).clone(), s.clone()))))
        .map(|(pr, _)| {
            let marked = links.made.get(pr).into_iter().flatten().map(|(_, at)| *at).chain(links.linked_at.get(pr).copied()).max();
            (Reverse(marked.unwrap_or(0)), pr)
        })
        .collect();
    prs.sort();
    prs.into_iter().map(|(_, pr)| pr.clone()).collect()
}

/// The open PRs of yours that another machine of yours owns (its card says so:
/// autoreview/own.rs), in repos this machine has a checkout of, as `owner/name#number`: read by
/// their numbers here too (`made_to_read`), so this machine lists them (`made_item`) and can review
/// them for that one. A checkout on its repo's default branch alone has its PRs read no other way
/// (`polled_slugs`), and none of this machine's sessions worked on them (`worked_prs`).
pub fn owned_elsewhere(machines: &crate::shared::Machines, lanes: &[Lane]) -> Vec<String> {
    let here: HashSet<String> = lanes.iter().filter(|l| l.device.is_none()).flat_map(|l| [l.repo_slug.as_deref(), l.repo_slug_now.as_deref()]).flatten().map(str::to_ascii_lowercase).collect();
    let mut out: Vec<String> = machines
        .others
        .iter()
        .flat_map(|m| &m.prs)
        .filter(|c| c.state.is_open() && c.shared_review.as_ref().is_some_and(|s| s.owner))
        .filter(|c| c.key.rsplit_once('#').is_some_and(|(slug, _)| here.contains(&slug.to_ascii_lowercase())))
        .map(|c| crate::prlinks::pr_key(&c.key))
        .collect();
    out.sort();
    out.dedup();
    out
}

/// The PRs to read by their numbers where their repos' PRs aren't read (ghpoll.rs `made_to_read`,
/// which reads only so many, the first it's given): the ones another machine owns first
/// (`owned_elsewhere`), a few at most, and kept from this machine's reviews for that one by
/// however many PRs its sessions worked on; then those (`worked_prs`).
pub fn read_by_number(links: &PrLinks, lanes: &[Lane], machines: &crate::shared::Machines) -> Vec<String> {
    let mut keys = owned_elsewhere(machines, lanes);
    for key in worked_prs(links, lanes) {
        if !keys.contains(&key) {
            keys.push(key);
        }
    }
    keys
}

/// Whether GitHub would merge the PR now: open and not a draft, with nothing between it and a
/// merge (no conflict, no review or rule it waits on, every check passed), and not resting only on
/// approvals of an earlier commit (`stale_approval`). src/prStatus.ts `prReady` says the same.
fn merge_ready(p: &PrItem) -> bool {
    p.state.is_open()
        && p.state != PrState::Draft
        && p.merge == Merge::Clean
        && p.checks.failed == 0
        && p.checks.pending == 0
        && !(p.state == PrState::Approved && p.stale_approval.is_some())
}

/// Whether a session listed on a PR is actively working on it: working, or set aside for its side
/// conversation, leaving out visitors, archived sessions and unattended automatic reviews.
fn pr_session_at_work(s: &Session) -> bool {
    let reviewing = s.helper.as_ref().is_some_and(|h| h.pr.is_some()) && s.helps_an_agent();
    let at_work = s.status == Status::Working || (s.aside.is_some() && s.status == Status::Needs);
    s.visitor.is_none() && !s.archived && at_work && !reviewing
}

/// The PRs (lowercase `owner/name#number`) that a session on another of your online devices is
/// actively working on (`pr_session_at_work`): from the sessions each device's PR list tied to the
/// PR (`pr_sessions`), and from any online lane that holds the PR itself.
pub fn working_elsewhere(lanes: &[Lane], pr_sessions: &BTreeMap<String, Vec<PrSession>>) -> BTreeSet<String> {
    let online = |l: &&Lane| l.device.as_ref().is_some_and(|d| d.online);
    let mut out: BTreeSet<String> = BTreeSet::new();
    for (pr, sessions) in pr_sessions {
        let working = sessions.iter().any(|x| {
            lanes
                .iter()
                .filter(online)
                .find(|l| l.id == x.lane)
                .and_then(|l| l.sessions.iter().find(|s| s.key == x.key))
                .is_some_and(pr_session_at_work)
        });
        if working {
            out.insert(crate::prlinks::pr_key(pr));
        }
    }
    for l in lanes.iter().filter(online) {
        let Some(number) = l.pr.as_ref().map(|p| p.number) else { continue };
        if !l.sessions.iter().any(pr_session_at_work) {
            continue;
        }
        for slug in [&l.repo_slug, &l.repo_slug_now].into_iter().flatten() {
            out.insert(crate::prlinks::pr_key(&format!("{slug}#{number}")));
        }
    }
    out
}

/// Takes `PrItem::waits_to_merge` off the PRs `pr_item` said were yours to merge where something
/// else brings them to you or it isn't ready yet. Not while one of its sessions on this Mac (a
/// taken-over primary too) or on another online Mac (`working_elsewhere`) is at work on it
/// (`pr_session_at_work`): working, or set aside for its side conversation, but not a visitor,
/// whose work elsewhere is its own, nor Codealong's review of it while it works for that review.
/// Nor while its primary session waits on you, in the queue, with Merge and done for it on its
/// strip: that's the first PR in the board's order that the session is the primary of and GitHub
/// would merge (src/menus.ts `mergesWith`), never one it took over, so another of its PRs waits on
/// you by itself. A PR whose work is all done with (its sessions archived, or none known) waits on
/// you by itself, so a merge left for you isn't left unseen.
pub fn merge_waits(prs: &mut [PrItem], lanes: &[Lane], working_elsewhere: &BTreeSet<String>) {
    let session = |k: &str| home(lanes, k).map(|(_, s)| s).filter(|s| !s.archived);
    // A session as the PR lists it, in the worktree it's listed under.
    let listed = |x: &PrSession| lanes.iter().find(|l| l.id == x.lane).and_then(|l| l.sessions.iter().find(|s| s.key == x.key));
    // In the queue, as the floating bar lists what waits on you (barfeed.rs `feed`).
    let waits = |s: &Session| s.visitor.is_none() && !s.unattended() && matches!(s.status, Status::Needs | Status::Error | Status::Unread) && s.snoozed_until.is_none() && s.aside.is_none();
    // The sessions whose strip has Merge and done for a PR already.
    let mut on_strip: HashSet<String> = HashSet::new();
    for p in prs.iter_mut() {
        let strip = p.primary.as_deref().filter(|k| !p.taken_over && merge_ready(p) && session(k).is_some() && !on_strip.contains(*k)).map(String::from);
        let merged_from = strip.as_deref().and_then(session).is_some_and(waits);
        on_strip.extend(strip);
        let worked = || working_elsewhere.contains(&crate::prlinks::pr_key(&p.key)) || p.sessions.iter().filter_map(listed).any(pr_session_at_work);
        p.waits_to_merge = p.waits_to_merge && merge_ready(p) && !merged_from && !worked();
    }
}

/// Applies the snoozes you gave reviews asked of you, and PRs of yours that wait on you to merge
/// them. Until its time such a PR carries `snoozed_until`, and Review requested, the floating
/// bar's reviews and the mark leave it out; then it's back as GitHub has it. A PR that no longer
/// waits on you has none, and the snooze runs out in state.json unused.
pub fn apply_review_snoozes(prs: &mut [PrItem], snoozes: &[Snooze], now: i64) {
    if snoozes.is_empty() {
        return;
    }
    for p in prs.iter_mut().filter(|p| (p.review_requested || p.waits_to_merge) && p.state.is_open()) {
        p.snoozed_until = snoozes.iter().find(|z| z.key == p.key && z.until > now).map(|z| z.until);
    }
}

/// The repos whose PRs are read, one GitHub query each: those with a worktree on a branch other
/// than the default, which may have a PR of its own. The rest hear only of PRs that wait on your
/// review, from one search across GitHub.
pub fn polled_slugs(seeds: &[LaneSeed]) -> BTreeSet<String> {
    seeds.iter().filter(|s| s.branch.is_some() && s.branch != s.default_branch).filter_map(|s| s.repo_slug.clone()).collect()
}

/// The PRs read one by one by their numbers (`made`), as `pr_key`s.
fn read_alone(made: &HashMap<String, Vec<PrRaw>>) -> HashSet<String> {
    made.iter().flat_map(|(slug, list)| list.iter().map(move |p| crate::prlinks::pr_key(&format!("{slug}#{}", p.number)))).collect()
}

/// Whether the board has a PR as GitHub has it now, which is what says it waits on you again
/// though GitHub asks nobody (autoreview.rs `asked_again`): every PR of a repo whose PRs are read
/// (`polled_slugs`), and the PRs read one by one by their numbers (`made`), as one an automatic
/// review was started on is in a repo checked out only on its default branch. An author's answer
/// to that review there was left waiting on nobody (specbook-infra#404, 2026-11-19). The rest of a
/// repo no longer read are as it was last read, and GitHub's search has the last word on them
/// (`pr_items`).
pub fn read_now(seeds: &[LaneSeed], made: &HashMap<String, Vec<PrRaw>>) -> impl Fn(&PrItem) -> bool {
    let (polled, alone) = (polled_slugs(seeds), read_alone(made));
    move |p| polled.contains(&p.repo_slug) || alone.contains(&crate::prlinks::pr_key(&p.key))
}

/// One PR that `pr_items` doesn't list, for a link to it in a conversation: one older than the
/// PRs the board reads, or in a repo no project covers. It's matched to worktrees and sessions
/// as the listed ones are. `slug` is the repo as the link names it; a project's repo that
/// answers to it in another case, or by the name it had before a rename (`renamed`, old name to
/// new, lowercased), is the one it's matched in.
pub fn pr_card(seeds: &[LaneSeed], lanes: &[Lane], slug: &str, renamed: &HashMap<String, String>, p: &PrRaw, login: Option<&str>, links: &PrLinks, working_elsewhere: &BTreeSet<String>) -> PrItem {
    let repos = repos_of(seeds);
    let owners = owners_of(seeds);
    let found = repos.iter().find(|(s, _)| {
        s.eq_ignore_ascii_case(slug) || renamed.get(&s.to_ascii_lowercase()).is_some_and(|now| now.eq_ignore_ascii_case(slug))
    });
    let mut item = match found {
        Some((&local, &repo)) => pr_item(lanes, &owners, local, repo, p, None, login, links),
        None => pr_item(lanes, &owners, slug, ("", slug.rsplit('/').next().unwrap_or(slug), None), p, None, login, links),
    };
    merge_waits(std::slice::from_mut(&mut item), lanes, working_elsewhere);
    item
}

/// The GitHub owners each worktree's checkout has a remote for (`LaneSeed::head_owners`), by
/// its path, a lane's id: the forks whose branches it can be on.
fn owners_of(seeds: &[LaneSeed]) -> HashMap<&Path, &[String]> {
    seeds.iter().map(|s| (s.path.as_path(), s.head_owners.as_slice())).collect()
}

/// Each project's repo by its slug: the project, the repo's name and its default branch.
fn repos_of(seeds: &[LaneSeed]) -> HashMap<&str, (&str, &str, Option<&str>)> {
    let mut repos: HashMap<&str, (&str, &str, Option<&str>)> = HashMap::new();
    for s in seeds {
        if let Some(slug) = s.repo_slug.as_deref() {
            repos.entry(slug).or_insert((s.project.as_str(), s.repo.as_str(), s.default_branch.as_deref()));
        }
    }
    repos
}

/// When the last PR from `p`'s branch before it ended, of the repo's `list`: a branch such as
/// `yolo` serves one PR after another, and a push before then was for that one. A branch of the
/// same name in another fork (`head_owner`) is another branch.
fn branch_freed<'a>(list: impl IntoIterator<Item = &'a PrRaw>, p: &PrRaw) -> Option<i64> {
    let before = list.into_iter().filter(|q| q.number != p.number && q.head_ref == p.head_ref && q.head_owner == p.head_owner);
    before.filter_map(|q| q.merged_at.or(q.closed_at)).filter(|&end| end <= p.created_at).max()
}

/// A PR with its worktrees and sessions, as `pr_items` says. `freed` is when an earlier PR from
/// its branch ended (`branch_freed`), when that's known.
pub(super) fn pr_item(lanes: &[Lane], owners: &HashMap<&Path, &[String]>, slug: &str, (project, repo, default_branch): (&str, &str, Option<&str>), p: &PrRaw, freed: Option<i64>, login: Option<&str>, links: &PrLinks) -> PrItem {
    let here: Vec<&Lane> = lanes.iter().filter(|l| l.repo_slug.as_deref() == Some(slug)).collect();
    // Whether a lane's branch of the PR's name can be the PR's: not when the PR comes from a fork
    // its checkout has no remote for. A lane with no seed to say (a test's) can.
    let holds_head = |l: &Lane| p.head_owner.as_ref().map_or(true, |o| owners.get(Path::new(&l.id)).map_or(true, |have| has_owner(have, o)));
    let head = p.head_ref.as_str();
    let (opened, ended) = (p.created_at, p.merged_at.or(p.closed_at));
    let before_end = move |s: &Session| ended.map_or(true, |end| s.started_at.unwrap_or(s.last_activity) <= end);
    let during = move |s: &Session| s.last_activity >= opened - 3_600_000 && before_end(s);
    // How often a session listed under a lane of the repo pushed to the branch in the PR's time,
    // as `during` has it, and after an earlier PR from the branch ended: a branch used for one PR
    // after another (`yolo`) has the earlier ones' pushes too, and one of the same name in another
    // repo, or a fork the lane's checkout has no remote for, its own. A PR from the default
    // branch counts none, as it lists no sessions by branch.
    let by_branch = default_branch != Some(head);
    let since = (opened - 3_600_000).max(freed.map_or(i64::MIN, |t| t + 1));
    let pushes = move |l: &Lane, k: &str| -> u32 {
        let pushed = links.pushed.get(k).filter(|_| by_branch && l.repo_slug.as_deref() == Some(slug) && holds_head(l)).into_iter().flatten();
        pushed.filter(|(b, at)| b == head && *at >= since && ended.map_or(true, |end| *at <= end)).count() as u32
    };
    let mut sessions: Vec<PrSession> = Vec::new();
    if by_branch {
        // Sessions in their own lanes first, then ones that came to a worktree on the branch.
        let own = here.iter().filter(|l| holds_head(l)).flat_map(|&l| {
            // One that works here from another folder goes by this worktree's branch, and by its
            // folder's, where it may have worked on the PR before it came (`compose`).
            let on_branch = move |s: &Session| s.branch.as_deref().or(l.branch.as_deref()) == Some(head) || s.folder_branch.as_deref() == Some(head);
            l.sessions.iter().filter(move |s| s.visitor.is_none() && on_branch(s) && during(s)).map(move |s| (l, s))
        });
        let guests = here.iter().filter(|l| l.branch.as_deref() == Some(head) && holds_head(l)).flat_map(|&l| {
            let counts = move |s: &Session| match s.visitor.as_deref() {
                None => false,
                Some(MADE_IT) => before_end(s),
                Some(_) => during(s),
            };
            l.sessions.iter().filter(move |s| counts(s)).map(move |s| (l, s))
        });
        let pushers = here.iter().flat_map(|&l| l.sessions.iter().filter(move |s| s.visitor.is_none() && during(s) && pushes(l, &s.key) > 0).map(move |s| (l, s)));
        for (l, s) in own.chain(guests).chain(pushers) {
            if !sessions.iter().any(|x| x.key == s.key) {
                sessions.push(PrSession { key: s.key.clone(), lane: l.id.clone(), pushes: pushes(l, &s.key), primary: None });
            }
        }
    }
    let key = crate::prlinks::pr_key(&format!("{slug}#{}", p.number));
    // The sessions that did some of the PR's work: on its branch, or pushing to it, or that its
    // records mark or that were linked to it by hand (`links.worked`: `codealong pr link` is for
    // work Codealong can't see). Not the one picked as its primary, which a pick links too (in the
    // PR's menu, by a watch, or as the copy a primary was resumed as): it's there for the pick.
    let picked = links.picked.get(&key);
    let linked = links.worked.get(&key).into_iter().flatten().filter(|k| Some(*k) != picked);
    let did: HashSet<String> = sessions.iter().map(|x| x.key.clone()).chain(links.made.get(&key).into_iter().flatten().map(|(s, _)| s.clone())).chain(linked.cloned()).collect();
    for k in links.worked.get(&key).into_iter().flatten() {
        if sessions.iter().any(|x| &x.key == k) {
            continue;
        }
        // The lane it's in, else one it visited.
        if let Some((l, s)) = home(lanes, k) {
            sessions.push(PrSession { key: s.key.clone(), lane: l.id.clone(), pushes: pushes(l, &s.key), primary: None });
        }
    }
    sessions.retain(|x| !links.unlinked.contains(&(key.clone(), x.key.clone())));
    let (primary, primary_by) = pr_primary(lanes, &sessions, &key, links).unzip();
    // A helper's parent that took the PR over is listed on it, under its own worktree, so the PR
    // shows with it and its menu can pick it.
    if let (Some(k), Some(PrimaryBy::Helper)) = (&primary, primary_by) {
        if let Some((l, _)) = home(lanes, k).filter(|_| !sessions.iter().any(|x| &x.key == k)) {
            sessions.push(PrSession { key: k.clone(), lane: l.id.clone(), pushes: 0, primary: None });
        }
    }
    // Its primary did none of its work: it took the PR over from a helper it started, or was
    // picked (a watch picks the primary it begins on, and a primary resumed as a copy is picked
    // in its place, nudges.rs `follow`).
    let taken_over = primary.as_ref().is_some_and(|k| !did.contains(k));
    let mine = login.is_some() && p.author.as_deref() == login;
    let by_hand = links.nudged.contains(&key);
    let watched = links.watched.contains(&key) && primary_by == Some(PrimaryBy::Picked);
    let by_default = links.nudge_by_default_in.get(slug).copied().unwrap_or(links.nudge_by_default);
    let nudge = p.state.is_open() && !links.not_nudged.contains(&key) && (by_hand || (by_default && mine));
    // Assigned to you since your last review: it waits on your review as a request does, asked
    // by whoever assigned you, since then, whichever move its turn says.
    let assigned = p.assigned_review(login).filter(|_| !p.review_requested);
    let requested_by = p.requested_by.clone().filter(|_| p.review_requested).or_else(|| assigned.map(|a| a.by.clone()));
    let requested_at = p.requested_at.filter(|_| p.review_requested).or_else(|| assigned.map(|a| a.at));
    let review_requested = p.review_requested || assigned.is_some();
    PrItem {
        key: format!("{slug}#{}", p.number),
        repo_slug: slug.to_string(),
        repo: repo.to_string(),
        project: project.to_string(),
        number: p.number,
        title: p.title.clone(),
        url: p.url.clone(),
        state: p.state,
        author: p.author.clone(),
        mine,
        review_requested,
        requested_by,
        requested_at,
        reviewers_asked: p.reviewers_asked.clone(),
        snoozed_until: None,
        nudge,
        nudge_by_default: nudge && !by_hand,
        watched,
        // As its repo's settings say: the engine knows them (engine/publish.rs, where it sends the board).
        review_first: false,
        // Yours to merge, where GitHub says you may and doesn't merge it by itself: `merge_waits`
        // keeps it while GitHub would merge it now and nothing else brings it to you, once every
        // PR is listed.
        waits_to_merge: mine && p.writes == Some(true) && !p.merging,
        taken_over,
        primary,
        primary_by,
        head: p.head_ref.clone(),
        base: p.base_ref.clone(),
        checks: p.checks.clone(),
        merge: p.merge,
        additions: p.additions,
        deletions: p.deletions,
        changed_files: p.changed_files,
        created_at: p.created_at,
        updated_at: p.updated_at,
        merged_at: p.merged_at,
        closed_at: p.closed_at,
        lanes: here.iter().filter(|l| l.pr.as_ref().map(|x| x.number) == Some(p.number)).map(|l| l.id.clone()).collect(),
        sessions,
        head_oid: p.head_oid.clone(),
        authored_by: p.authored_by,
        labels: p.labels.clone(),
        auto_review: None,
        last_review: p.last_review.clone(),
        stale_approval: p.stale_approval.clone(),
        turn: turn_of(p, mine, login, &links.agent_posts),
        shared_review: Default::default(),
    }
}

/// Whose move an open PR is, where its review state doesn't say (`Turn`), from what was said on
/// it lately (`PrRaw::said`). A PR of yours that a reviewer asked changes of is their move while
/// they're asked again or you wrote since their last word; yours where commits came after it and
/// nobody said so (`ReplyNeeded`), or where they wrote again after asking (`ReviewerReplied`);
/// and with nothing since, the request for changes says it. Of several such reviewers, the move
/// that's yours goes first, naming each it holds for. Someone else's PR you wrote on or reviewed
/// is yours where its author wrote after you were on it (anything of yours, your agent's posted
/// reviews too) and nothing of yours answered them since: a review you submitted, or words of your
/// own, not your agent's post, which leaves the sign-off to you as a request on GitHub would; and
/// not once your latest review approves the commit it's at, when what they say is theirs to act
/// on. It's yours too where someone assigned you to it after your last review and GitHub doesn't
/// ask for your review (`Assigned`, `PrRaw::assigned_review`); of the two, the later says it. A
/// review just submitted here counts at once (`last_review`, ghpoll.rs `on_approved`), before
/// GitHub is read again. `agent_posts` are your agent's posts (`PrLinks::agent_posts`). A push is taken to be when its
/// head commit was made, the nearest GitHub says: one made long before it's pushed reads as earlier
/// than it was. Nobody said so only where what was said reaches back to the push
/// (`PrRaw::said_from`): a run of a bot's comments can hide an older word of yours.
pub fn turn_of(p: &PrRaw, mine: bool, login: Option<&str>, agent_posts: &HashSet<String>) -> Option<Turn> {
    if !p.state.is_open() || p.state == PrState::Draft {
        return None;
    }
    let same = |a: &str, b: &str| a.eq_ignore_ascii_case(b);
    let wrote = |who: &str| p.said.iter().filter(|s| s.asked.is_none() && same(&s.who, who)).map(|s| s.at).max();
    let author = p.author.as_deref()?;
    let turn = |kind, who: &str, since| Some(Turn { kind, who: who.to_string(), also: Vec::new(), since });
    if !mine {
        let assigned = p.assigned_review(login).filter(|_| !p.review_requested).and_then(|a| turn(TurnKind::Assigned, &a.by, Some(a.at)));
        let replied = || -> Option<Turn> {
            let (me, theirs) = (login?, wrote(author)?);
            let mine_said = || p.said.iter().filter(|s| s.asked.is_none() && same(&s.who, me));
            let on_it = mine_said().any(|s| s.at < theirs);
            let agents = |s: &&crate::github::Said| s.url.as_ref().is_some_and(|u| agent_posts.contains(u));
            let answered = mine_said().filter(|s| !agents(s)).map(|s| s.at).max().max(p.last_review.as_ref().and_then(|l| l.at));
            let approved = p.last_review.as_ref().is_some_and(|l| l.state == LastReviewState::Approved && l.commit.is_some() && l.commit == p.head_oid);
            if on_it && answered.is_none_or(|a| theirs > a) && !approved { turn(TurnKind::AuthorReplied, author, Some(theirs)) } else { None }
        };
        return assigned.into_iter().chain(replied()).max_by_key(|t| t.since);
    }
    let of = |c: &crate::github::ChangesAsked| -> Option<Turn> {
        let asked = p.said.iter().filter(|s| s.asked.as_deref().is_some_and(|r| same(r, &c.who))).map(|s| s.at).max();
        if p.reviewers_asked.iter().any(|r| same(r, &c.who)) {
            return turn(TurnKind::RereviewAsked, &c.who, asked);
        }
        let theirs = wrote(&c.who).map_or(c.at, |t| t.max(c.at));
        let yours = wrote(author).max(asked);
        // Commits came since their review, where GitHub says which commit that was of.
        let pushed = c.commit.is_some() && p.head_oid.is_some() && p.head_oid != c.commit;
        match p.head_at {
            Some(at) if pushed && at > theirs && yours.is_none_or(|y| at > y) && p.said_from.is_none_or(|from| from <= at) => turn(TurnKind::ReplyNeeded, &c.who, Some(at)),
            _ if yours.is_some_and(|y| y > theirs) => turn(TurnKind::RereviewAsked, &c.who, yours),
            _ if theirs > c.at => turn(TurnKind::ReviewerReplied, &c.who, Some(theirs)),
            _ => None,
        }
    };
    let urgency = |t: &Option<Turn>| match t.as_ref().map(|t| t.kind) {
        Some(TurnKind::ReplyNeeded) => 0,
        Some(TurnKind::ReviewerReplied) => 1,
        None => 2,
        Some(_) => 3,
    };
    let turns: Vec<Option<Turn>> = p.changes_asked.iter().filter(|c| !same(&c.who, author)).map(of).collect();
    let first = turns.iter().min_by_key(|t| urgency(t))?.clone()?;
    let also = turns.iter().flatten().filter(|t| t.kind == first.kind && t.who != first.who).map(|t| t.who.clone()).collect();
    Some(Turn { also, ..first })
}

/// The PR's primary session, of the ones listed on it, and why: the one picked by hand; else the
/// one that pushed to its branch most often (`PrSession::pushes`); else the one that started
/// first. The one that opened it, whose records show it first (a session that later pushes to
/// it, comments or edits it is marked after), counts as having pushed once at least, since its
/// push may have left no record (a quiet `git push -q`, a harness that records none), and wins a
/// tie: a session that pushes a fix or two to another's PR doesn't take it over, and one that
/// goes on with the work does. Without pushes that's the one that opened it. A copy of a
/// conversation (a fork, a move to another account) carries its marks and pushes with their
/// times, so the one that started first wins a tie. An archived session is listed but never the
/// primary, picked or not: the next takes over. Nor is an automatic review (`Helper::pr`), unless picked: it reads the work
/// and says what it found, and runs in the PR's worktree on its branch, so with the session
/// that opened it gone it took over as the one that started first, and was told of what
/// happened on the PR in place of the work's own session.
///
/// With none left, a PR whose work was mostly a helper's (by the same rule over the archived ones
/// too) goes to the session that started the helper (`helper_parent`), if you haven't archived
/// that one: an agent hands work to a helper, which opens the PR, reports
/// and is marked done (`codealong session done`), and the PR would otherwise have nobody to tell
/// of a review or a failing check, and no session to merge it from.
fn pr_primary(lanes: &[Lane], sessions: &[PrSession], key: &str, links: &PrLinks) -> Option<(String, PrimaryBy)> {
    let archived = |k: &str| home(lanes, k).is_some_and(|(_, s)| s.archived);
    let live: Vec<&PrSession> = sessions.iter().filter(|s| !archived(&s.key)).collect();
    if let Some(k) = links.picked.get(key).filter(|k| live.iter().any(|s| &s.key == *k)) {
        return Some((k.clone(), PrimaryBy::Picked));
    }
    let reviews = |k: &str| home(lanes, k).is_some_and(|(_, s)| s.helper.as_ref().is_some_and(|h| h.pr.is_some()));
    let started = |k: &str| home(lanes, k).map_or(i64::MAX, |(_, s)| s.started_at.unwrap_or(s.last_activity));
    // When its records first show it.
    let mark = |k: &str| links.made.get(key).into_iter().flatten().find(|(m, _)| m == k).map(|(_, at)| crate::prlinks::mark_order(*at));
    // The one of `sessions` that did the most of the PR's work, and how.
    let ranked = |sessions: Vec<&PrSession>| -> Option<(String, PrimaryBy)> {
        let opener = sessions.iter().filter_map(|s| Some((mark(&s.key)?, started(&s.key), s.key.as_str()))).min().map(|(_, _, k)| k);
        let pushes = |s: &PrSession| if Some(s.key.as_str()) == opener { s.pushes.max(1) } else { s.pushes };
        // A session without a mark after every one with one.
        let best = sessions.iter().min_by_key(|s| (Reverse(pushes(s)), mark(&s.key).unwrap_or((true, i64::MAX)), started(&s.key), s.key.as_str()))?;
        let by = if Some(best.key.as_str()) == opener {
            PrimaryBy::Opened
        } else if best.pushes > 0 {
            PrimaryBy::Pushed
        } else {
            PrimaryBy::Started
        };
        Some((best.key.clone(), by))
    };
    if let Some(found) = ranked(live.into_iter().filter(|s| !reviews(&s.key)).collect()) {
        return Some(found);
    }
    // The one of them all that did the most, as above, when it's a helper whose parent is still
    // about and wasn't taken off the PR by hand: not a helper that only looked in on the work of
    // a session archived since.
    let parent = |k: &str| helper_parent(lanes, k).filter(|p| !links.unlinked.contains(&(key.to_string(), p.clone())));
    let (best, _) = ranked(sessions.iter().filter(|s| !reviews(&s.key)).collect())?;
    Some((parent(&best)?, PrimaryBy::Helper))
}

/// The session that started the helper `key` (`Helper::parent`), where the board lists it and you
/// haven't archived it; none for a session that isn't a helper, or is an automatic review, which
/// reads a PR rather than making it.
fn helper_parent(lanes: &[Lane], key: &str) -> Option<String> {
    let (_, s) = home(lanes, key)?;
    let parent = s.helper.as_ref().filter(|h| h.pr.is_none())?.parent.as_deref()?;
    home(lanes, parent).filter(|(_, p)| !p.archived).map(|(_, p)| p.key.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::fixtures::{acct, pr, reg, seed, tr, NOW};

    fn wrote(who: &str, at: i64) -> crate::github::Said {
        crate::github::Said { who: who.into(), at, asked: None, url: None }
    }

    fn asked(who: &str, reviewer: &str, at: i64) -> crate::github::Said {
        crate::github::Said { who: who.into(), at, asked: Some(reviewer.into()), url: None }
    }

    /// A PR of gwen's that ken asked changes of at 100, at commit `aaa`: his findings at 99, the
    /// request at 100.
    fn changes_asked() -> PrRaw {
        PrRaw {
            head_oid: Some("aaa".into()),
            head_at: Some(90),
            said: vec![wrote("ken", 99), wrote("ken", 100)],
            changes_asked: vec![crate::github::ChangesAsked { who: "ken".into(), at: 100, commit: Some("aaa".into()) }],
            ..pr(PrState::Changes, 0)
        }
    }

    fn kind(p: &PrRaw, mine: bool) -> Option<(TurnKind, String, Option<i64>)> {
        turn_of(p, mine, Some(if mine { "gwen" } else { "me" }), &HashSet::from(["agents-post".to_string()])).map(|t| (t.kind, t.who, t.since))
    }

    #[test]
    fn a_push_after_a_request_for_changes_that_nobody_said_is_yours_to_say() {
        use TurnKind::*;
        let p = changes_asked();
        // Nothing since the request: it says itself.
        assert_eq!(kind(&p, true), None);
        // The fixes pushed and nothing said, as on a PR between 07:04 and 07:07 on 2026-10-22.
        let pushed = PrRaw { head_oid: Some("bbb".into()), head_at: Some(110), ..p.clone() };
        assert_eq!(kind(&pushed, true), Some((ReplyNeeded, "ken".into(), Some(110))));
        // Said before the push, not after it: still yours.
        let before = PrRaw { said: [p.said.clone(), vec![wrote("gwen", 105)]].concat(), ..pushed.clone() };
        assert_eq!(kind(&before, true), Some((ReplyNeeded, "ken".into(), Some(110))));
        // Asked again after it, or written to: his move.
        let again = PrRaw { said: [pushed.said.clone(), vec![asked("gwen", "ken", 112)]].concat(), ..pushed.clone() };
        assert_eq!(kind(&again, true), Some((RereviewAsked, "ken".into(), Some(112))));
        let told = PrRaw { said: [pushed.said.clone(), vec![wrote("gwen", 115)]].concat(), ..pushed.clone() };
        assert_eq!(kind(&told, true), Some((RereviewAsked, "ken".into(), Some(115))));
        // Still asked, however it stands: his Codealong has it waiting on him.
        let waiting = PrRaw { reviewers_asked: vec!["Ken".into()], ..pushed.clone() };
        assert_eq!(kind(&waiting, true), Some((RereviewAsked, "ken".into(), None)));
        // He wrote again after that ("all fixed"), so it's yours: ask for the approval.
        let replied = PrRaw { said: [told.said.clone(), vec![wrote("ken", 120)]].concat(), ..told.clone() };
        assert_eq!(kind(&replied, true), Some((ReviewerReplied, "ken".into(), Some(120))));
        // A review whose commit GitHub doesn't say can't be told a push came after.
        let unknown = PrRaw { changes_asked: vec![crate::github::ChangesAsked { commit: None, ..p.changes_asked[0].clone() }], ..pushed.clone() };
        assert_eq!(kind(&unknown, true), None);
        // A draft, or a PR that ended, is nobody's move.
        assert_eq!(kind(&PrRaw { state: PrState::Draft, ..pushed.clone() }, true), None);
        assert_eq!(kind(&PrRaw { state: PrState::Merged, ..pushed.clone() }, true), None);
    }

    #[test]
    fn of_several_reviewers_the_one_whose_move_is_yours_says_it() {
        use TurnKind::*;
        let two = |ana_said: Vec<crate::github::Said>| PrRaw {
            head_oid: Some("bbb".into()),
            head_at: Some(110),
            said: [vec![wrote("ken", 100), asked("gwen", "ken", 112)], ana_said].concat(),
            changes_asked: vec![
                crate::github::ChangesAsked { who: "ken".into(), at: 100, commit: Some("aaa".into()) },
                crate::github::ChangesAsked { who: "ana".into(), at: 111, commit: Some("bbb".into()) },
            ],
            ..pr(PrState::Changes, 0)
        };
        // Ken is asked again; ana asked for changes of the commit it's at, and nothing since.
        assert_eq!(kind(&two(vec![]), true), None);
        // Ana wrote again after that: yours.
        assert_eq!(kind(&two(vec![wrote("ana", 130)]), true), Some((ReviewerReplied, "ana".into(), Some(130))));
        // You wrote since, which both of them hear: their re-reviews are what's left, both named.
        assert_eq!(kind(&two(vec![wrote("gwen", 125)]), true), Some((RereviewAsked, "ken".into(), Some(125))));
        assert_eq!(turn_of(&two(vec![wrote("gwen", 125)]), true, Some("gwen"), &HashSet::new()).map(|t| t.also), Some(vec!["ana".to_string()]));
        // Both owed a word on the fixes: each is named, so each can be asked.
        let both = PrRaw {
            head_oid: Some("ccc".into()),
            head_at: Some(140),
            said: vec![wrote("ken", 100), wrote("ana", 111)],
            changes_asked: vec![
                crate::github::ChangesAsked { who: "ken".into(), at: 100, commit: Some("aaa".into()) },
                crate::github::ChangesAsked { who: "ana".into(), at: 111, commit: Some("bbb".into()) },
            ],
            ..pr(PrState::Changes, 0)
        };
        let t = turn_of(&both, true, Some("gwen"), &HashSet::new()).unwrap();
        assert_eq!((t.kind, t.who.as_str(), t.also), (ReplyNeeded, "ken", vec!["ana".to_string()]));
    }

    #[test]
    fn nobody_said_so_only_where_what_was_said_reaches_back_to_the_push() {
        // Pushed at 110; GitHub's last twenty comments begin at 120, a bot's, so a word of yours
        // between the push and then can't be told from none.
        let p = PrRaw { head_oid: Some("bbb".into()), head_at: Some(110), said_from: Some(120), ..changes_asked() };
        assert_eq!(kind(&p, true), None);
        // Reaching back to it, it's said.
        assert_eq!(kind(&PrRaw { said_from: Some(105), ..p.clone() }, true).map(|t| t.0), Some(TurnKind::ReplyNeeded));
        // What's known still says the rest: asked again after it is their move.
        let asked_again = PrRaw { said: [p.said.clone(), vec![asked("gwen", "ken", 130)]].concat(), ..p.clone() };
        assert_eq!(kind(&asked_again, true).map(|t| t.0), Some(TurnKind::RereviewAsked));
    }

    #[test]
    fn someone_elses_pr_is_yours_once_its_author_writes_after_you() {
        let p = |said| PrRaw { said, author: Some("gwen".into()), ..pr(PrState::Open, 0) };
        assert_eq!(kind(&p(vec![wrote("me", 100), wrote("gwen", 110)]), false), Some((TurnKind::AuthorReplied, "gwen".into(), Some(110))));
        assert_eq!(kind(&p(vec![wrote("gwen", 110), wrote("me", 120)]), false), None);
        // You haven't written on it: nothing of yours to answer.
        assert_eq!(kind(&p(vec![wrote("gwen", 110)]), false), None);
        // Asking you again isn't words to answer: Re-review says it.
        assert_eq!(kind(&p(vec![wrote("me", 100), asked("gwen", "me", 110)]), false), None);
        // Approved as it is, their thanks are theirs to act on; after new commits, yours again.
        let approved = |commit: &str| Some(LastReview { state: LastReviewState::Approved, commit: Some(commit.into()), at: Some(100) });
        let thanked = |commit: &str| PrRaw { last_review: approved(commit), ..p(vec![wrote("me", 100), wrote("gwen", 110)]) };
        assert_eq!(kind(&thanked("0ef4ed2"), false), None);
        assert_eq!(kind(&thanked("a11ce55"), false).map(|t| t.0), Some(TurnKind::AuthorReplied));
        // Your agent's review, posted as you after their reply, leaves it yours to sign off; it does
        // say you were on it, before their reply.
        let post = |at| crate::github::Said { url: Some("agents-post".into()), ..wrote("me", at) };
        assert_eq!(kind(&p(vec![wrote("me", 100), wrote("gwen", 110), post(120)]), false).map(|t| t.0), Some(TurnKind::AuthorReplied));
        assert_eq!(kind(&p(vec![post(100), wrote("gwen", 110)]), false).map(|t| t.0), Some(TurnKind::AuthorReplied));
        // A Mac that didn't post it takes it for your answer.
        let elsewhere = turn_of(&p(vec![wrote("me", 100), wrote("gwen", 110), post(120)]), false, Some("me"), &HashSet::new());
        assert_eq!(elsewhere, None);
        // A review you submitted after it answers them, as soon as it's submitted here, before
        // GitHub's next answer has it among what was said.
        let submitted = |state| PrRaw { last_review: Some(LastReview { state, commit: Some("a11ce55".into()), at: Some(120) }), ..p(vec![wrote("me", 100), wrote("gwen", 110)]) };
        assert_eq!(kind(&submitted(LastReviewState::Commented), false), None);
        assert_eq!(kind(&submitted(LastReviewState::Changes), false), None);
    }

    #[test]
    fn someone_elses_pr_assigned_to_you_since_your_last_review_waits_on_it() {
        let p = |said, at| PrRaw { said, author: Some("gwen".into()), assigned: Some(crate::github::Assigned { by: "ken".into(), at }), ..pr(PrState::Open, 0) };
        // Assigned, with no review of yours since: your move, from whoever assigned you.
        assert_eq!(kind(&p(vec![], 100), false), Some((TurnKind::Assigned, "ken".into(), Some(100))));
        // A review of yours after it answers it, as soon as it's submitted here; one before doesn't.
        let reviewed = |at| PrRaw { last_review: Some(LastReview { state: LastReviewState::Commented, commit: None, at: Some(at) }), ..p(vec![], 100) };
        assert_eq!(kind(&reviewed(120), false), None);
        assert_eq!(kind(&reviewed(90), false).map(|t| t.0), Some(TurnKind::Assigned));
        // GitHub asking for your review says it itself, and a PR of yours is never yours to review.
        assert_eq!(kind(&PrRaw { review_requested: true, ..p(vec![], 100) }, false), None);
        assert_eq!(kind(&p(vec![], 100), true), None);
        // Of an assignment and the author's words after yours, the later says it.
        assert_eq!(kind(&p(vec![wrote("me", 90), wrote("gwen", 110)], 100), false).map(|t| t.0), Some(TurnKind::AuthorReplied));
        assert_eq!(kind(&p(vec![wrote("me", 90), wrote("gwen", 95)], 100), false).map(|t| t.0), Some(TurnKind::Assigned));

        // On the board it waits on your review, asked by whoever assigned you, since then; a repo
        // whose PRs aren't read has only the search's word on it.
        let seeds = [seed("/r/spark-portal", "portal")];
        let items = |prs: &HashMap<String, Vec<PrRaw>>, requested: &HashMap<String, Vec<PrRaw>>, seeds: &[LaneSeed]| pr_items(seeds, &[], prs, requested, &HashMap::new(), Some("me"), &PrLinks::default());
        let prs = HashMap::from([("sprucehq/spark".to_string(), vec![p(vec![], 100)])]);
        let got = items(&prs, &HashMap::new(), &seeds).remove(0);
        assert_eq!((got.review_requested, got.requested_by.as_deref(), got.requested_at), (true, Some("ken"), Some(100)));
        let gone = [seed("/r/spark-portal", "main")];
        assert!(!items(&prs, &HashMap::new(), &gone)[0].review_requested);
        assert!(items(&HashMap::new(), &prs, &gone)[0].review_requested);
        let reviewed = HashMap::from([("sprucehq/spark".to_string(), vec![reviewed(120)])]);
        assert_eq!(items(&reviewed, &HashMap::new(), &seeds)[0].requested_by, None);
        // Its author writing after the assignment says the move, and it still waits on you, from
        // the search too, where Author replied alone wouldn't bring it back (autoreview.rs
        // `asked_again`).
        let replied = HashMap::from([("sprucehq/spark".to_string(), vec![p(vec![wrote("me", 90), wrote("gwen", 110)], 100)])]);
        let got = items(&HashMap::new(), &replied, &gone).remove(0);
        assert_eq!((got.review_requested, got.requested_by.as_deref(), got.turn.map(|t| t.kind)), (true, Some("ken"), Some(TurnKind::AuthorReplied)));
        // A draft assigned to you doesn't wait on you, its assignee often being whoever does the
        // work: only a request for your review has a draft wait on you, and reviewed
        // (autoreview.rs `wants`, `[reviews] drafts`).
        let draft = HashMap::from([("sprucehq/spark".to_string(), vec![PrRaw { state: PrState::Draft, ..p(vec![], 100) }])]);
        assert!(!items(&draft, &HashMap::new(), &seeds)[0].review_requested);
        let asked = HashMap::from([("sprucehq/spark".to_string(), vec![PrRaw { state: PrState::Draft, review_requested: true, ..p(vec![], 100) }])]);
        assert!(items(&asked, &HashMap::new(), &seeds)[0].review_requested);
    }

    #[test]
    fn lists_review_requests_in_repos_checked_out_only_on_their_default_branch() {
        let mut site = seed("/r/site", "main");
        site.repo = "site".into();
        site.repo_slug = Some("sprucehq/site".into());
        let seeds = [seed("/r/spark-portal", "portal"), site];
        assert_eq!(polled_slugs(&seeds), BTreeSet::from(["sprucehq/spark".to_string()]));
        let asked = |number: u64| PrRaw { number, review_requested: true, ..pr(PrState::Open, 0) };
        // The site's PRs were read before its last worktree left its branch: #5 has since been
        // reviewed, #7 still waits and is newer from the search. The repo read on its own keeps
        // its own word: the search's copy of #1037 doesn't count there. The search's #2000, past
        // the hundred PRs the repo's read holds, is listed from the search.
        let prs = HashMap::from([
            ("sprucehq/spark".to_string(), vec![pr(PrState::Open, 0)]),
            ("sprucehq/site".to_string(), vec![asked(5), PrRaw { title: "Old title".into(), ..asked(7) }]),
        ]);
        let requested = HashMap::from([
            ("sprucehq/site".to_string(), vec![asked(7), asked(9)]),
            ("sprucehq/spark".to_string(), vec![asked(1037), asked(2000)]),
            ("acme/elsewhere".to_string(), vec![asked(3)]),
        ]);
        let mut got: Vec<(String, bool, String)> = pr_items(&seeds, &[], &prs, &requested, &HashMap::new(), None, &PrLinks::default()).into_iter().map(|p| (p.key, p.review_requested, p.project)).collect();
        got.sort();
        let want = [("sprucehq/site#5", false), ("sprucehq/site#7", true), ("sprucehq/site#9", true), ("sprucehq/spark#1037", false), ("sprucehq/spark#2000", true)];
        assert_eq!(got, want.map(|(k, r)| (k.to_string(), r, "Spruce".to_string())));
        let items = pr_items(&seeds, &[], &prs, &requested, &HashMap::new(), None, &PrLinks::default());
        assert_eq!(items.iter().find(|p| p.number == 7).map(|p| p.title.as_str()), Some("Let the agent add and caption project photos"));
        // A PR of yours its last read had ready to merge doesn't wait on you on that word: GitHub
        // may have merged or closed it since.
        let ready = PrRaw { number: 11, merge: Merge::Clean, writes: Some(true), author: Some("gwen".into()), ..pr(PrState::Approved, 0) };
        let prs = HashMap::from([("sprucehq/site".to_string(), vec![ready.clone()]), ("sprucehq/spark".to_string(), vec![PrRaw { number: 12, ..ready }])]);
        let items = pr_items(&seeds, &[], &prs, &HashMap::new(), &HashMap::new(), Some("gwen"), &PrLinks::default());
        let waits = |n: u64| items.iter().find(|p| p.number == n).is_some_and(|p| p.waits_to_merge);
        assert!(!waits(11) && waits(12));
    }

    #[test]
    fn links_prs_to_the_worktrees_and_sessions_on_their_branch() {
        let apps = AppNames::default();
        let mut s = seed("/r/spark-portal", "portal");
        s.repo_slug = Some("sprucehq/spark".into());
        let seeds = vec![s];
        let a = acct("claude2");
        let mut on_branch = claude_session(&a, Some(&reg("idle", NOW)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        on_branch.branch = Some("portal".into());
        let mut elsewhere = on_branch.clone();
        elsewhere.key = "claude:claude2:other".into();
        elsewhere.branch = Some("app4".into());
        let prs = HashMap::from([("sprucehq/spark".to_string(), vec![pr(PrState::Open, 0)])]);
        let (git, visitors) = (HashMap::new(), HashMap::new());
        let order = vec!["Spruce".to_string()];
        let board = compose(Inputs {
            now: NOW,
            apps: &apps,
            seeds: &seeds,
            sessions: vec![on_branch.clone(), elsewhere],
            visitors: &visitors,
            git: &git,
            prs: &prs,
            requested: &HashMap::new(),
            made: &HashMap::new(),
            project_order: &order,
            notices: vec![],
            loading: false,
            github_at: None,
            pinned: &[],
            github_login: Some("gwen"),
            pr_links: &PrLinks::default(),
            previews: &Previews::default(),
            reusable: &HashMap::new(),
            review_snoozes: &[],
            session_settings: &Default::default(), auto_reviews: (&Default::default(), &Default::default(), &Default::default()), machines: &Default::default(), at_merged: &HashSet::new(), working_elsewhere: &BTreeSet::new(),
        });
        assert_eq!(board.prs.len(), 1);
        let p = &board.prs[0];
        assert_eq!(p.key, "sprucehq/spark#1037");
        assert!(p.mine);
        assert_eq!(p.lanes, vec!["/r/spark-portal".to_string()]);
        assert_eq!(p.sessions, vec![PrSession { key: on_branch.key, lane: "/r/spark-portal".into(), pushes: 0, primary: None }]);
        assert_eq!(board.github_login.as_deref(), Some("gwen"));
    }

    #[test]
    fn a_pr_from_a_fork_the_checkout_has_no_remote_for_is_from_another_branch() {
        let s = |key: &str, started: i64| Session { cwd: "/r/spark-portal".into(), branch: Some("portal".into()), started_at: Some(started), last_activity: NOW, ..crate::fixtures::session(key) };
        let opened = NOW - 86_400_000;
        let lane = Lane { repo_slug: Some("sprucehq/spark".into()), branch: Some("portal".into()), ..crate::fixtures::lane("/r/spark-portal", vec![s("ours", opened), s("reviewer", opened + 1_000)]) };
        let lanes = vec![lane];
        // The checkout's remotes are sprucehq's and gwen's.
        let seeds = vec![seed("/r/spark-portal", "portal")];
        let push = vec![("portal".to_string(), opened + 3_600_000)];
        let links = PrLinks {
            worked: HashMap::from([("sprucehq/spark#1037".to_string(), vec!["reviewer".to_string()])]),
            pushed: HashMap::from([("ours".to_string(), push.clone()), ("reviewer".to_string(), push)]),
            ..Default::default()
        };
        let listed = |owner: &str| {
            let p = PrRaw { head_owner: Some(owner.into()), author: Some(owner.into()), ..pr(PrState::Open, 0) };
            let item = pr_card(&seeds, &lanes, "sprucehq/spark", &HashMap::new(), &p, Some("gwen"), &links, &BTreeSet::new());
            item.sessions.into_iter().map(|x| (x.key, x.pushes)).collect::<Vec<_>>()
        };
        assert_eq!(listed("gwen"), vec![("ours".to_string(), 1), ("reviewer".to_string(), 1)]);
        // GitHub may name the owner in another case than the remote's URL.
        assert_eq!(listed("Gwen"), listed("gwen"));
        // Ana's `portal` isn't the one checked out here: only the session whose records show it
        // worked on her PR is listed, and its pushes to this `portal` weren't to hers.
        assert_eq!(listed("ana"), vec![("reviewer".to_string(), 0)]);
    }

    #[test]
    fn a_pr_the_board_doesnt_list_is_matched_in_the_project_repo_a_link_names() {
        let seeds = vec![seed("/r/spark-portal", "portal")];
        let a = acct("claude2");
        let mut maker = claude_session(&a, Some(&reg("idle", NOW)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        maker.branch = Some("app4".into());
        let (git, visitors, prs) = (HashMap::new(), HashMap::new(), HashMap::new());
        let order = vec!["Spruce".to_string()];
        let board = compose(Inputs { now: NOW, apps: &AppNames::default(), seeds: &seeds, sessions: vec![maker.clone()], visitors: &visitors, git: &git, prs: &prs, requested: &HashMap::new(), made: &HashMap::new(), project_order: &order, notices: vec![], loading: false, github_at: None, pinned: &[], github_login: None, pr_links: &PrLinks::default(), previews: &Previews::default(), reusable: &HashMap::new(), review_snoozes: &[], session_settings: &Default::default(), auto_reviews: (&Default::default(), &Default::default(), &Default::default()), machines: &Default::default(), at_merged: &HashSet::new(), working_elsewhere: &BTreeSet::new() });
        let old = pr(PrState::Merged, 0);
        let links = PrLinks { worked: HashMap::from([("sprucehq/spark#1037".to_string(), vec![maker.key.clone()]), ("acme/tool#1037".to_string(), vec![maker.key.clone()])]), ..Default::default() };
        let renamed = HashMap::from([("sprucehq/spark".to_string(), "Spruce/spark".to_string())]);
        // In another case, or by the repo's name now: the project's repo, and the session its
        // records say made it.
        for slug in ["SpruceHQ/Spark", "spruce/spark"] {
            let item = pr_card(&seeds, &board.lanes, slug, &renamed, &old, Some("gwen"), &links, &BTreeSet::new());
            assert_eq!((item.key.as_str(), item.project.as_str(), item.repo.as_str()), ("sprucehq/spark#1037", "Spruce", "spark"), "{slug}");
            assert_eq!(item.sessions.iter().map(|x| x.key.as_str()).collect::<Vec<_>>(), vec![maker.key.as_str()], "{slug}");
            assert!(item.mine);
        }
        // A repo no project covers still gets the sessions linked to it.
        let other = pr_card(&seeds, &board.lanes, "acme/tool", &renamed, &old, None, &links, &BTreeSet::new());
        assert_eq!((other.key.as_str(), other.project.as_str(), other.repo.as_str()), ("acme/tool#1037", "", "tool"));
        assert_eq!(other.sessions.len(), 1);
        assert!(other.lanes.is_empty());
    }

    #[test]
    fn someone_elses_pr_read_by_its_number_waits_on_you_once_its_author_writes_after_you() {
        // Spark's PRs are read; infra is checked out on its default branch alone, so its PR is
        // read by its number, as one an automatic review was started on is. Its author answered
        // the review the agent posted for gwen, and nothing asks her of it on GitHub any more.
        let infra = LaneSeed { repo: "infra".into(), repo_slug: Some("sprucehq/infra".into()), ..seed("/r/infra", "main") };
        let seeds = vec![seed("/r/spark-portal", "portal"), infra];
        let answered = PrRaw { number: 404, url: "https://github.com/sprucehq/infra/pull/404".into(), author: Some("ana".into()), head_owner: Some("ana".into()), said: vec![wrote("gwen", NOW - 200_000), wrote("ana", NOW - 100_000)], ..pr(PrState::Open, 0) };
        let infra = |slug: &str, p: &PrRaw| HashMap::from([(slug.to_string(), vec![p.clone()])]);
        let (git, visitors, order) = (HashMap::new(), HashMap::new(), vec!["Spruce".to_string()]);
        let asked = |prs: &HashMap<String, Vec<PrRaw>>, made: &HashMap<String, Vec<PrRaw>>| {
            let board = compose(Inputs { now: NOW, apps: &AppNames::default(), seeds: &seeds, sessions: vec![], visitors: &visitors, git: &git, prs, requested: &HashMap::new(), made, project_order: &order, notices: vec![], loading: false, github_at: None, pinned: &[], github_login: Some("gwen"), pr_links: &PrLinks::default(), previews: &Previews::default(), reusable: &HashMap::new(), review_snoozes: &[], session_settings: &Default::default(), auto_reviews: (&Default::default(), &Default::default(), &Default::default()), machines: &Default::default(), at_merged: &HashSet::new(), working_elsewhere: &BTreeSet::new() });
            board.prs.iter().find(|p| p.number == 404).map(|p| (p.turn.as_ref().map(|t| t.kind), p.review_requested, p.requested_at))
        };
        assert_eq!(asked(&HashMap::new(), &infra("SpruceHQ/infra", &answered)), Some((Some(TurnKind::AuthorReplied), true, Some(NOW - 100_000))));
        // Kept from when the repo was last read, it's as it was then: GitHub's search, which no
        // longer lists it, has the last word.
        assert_eq!(asked(&infra("sprucehq/infra", &answered), &HashMap::new()), Some((Some(TurnKind::AuthorReplied), false, None)));
    }

    #[test]
    fn a_pr_a_session_made_where_no_repo_is_read_is_listed_with_it() {
        let maker = Session { cwd: "/r/spark-portal".into(), branch: Some("portal".into()), ..crate::fixtures::session("maker") };
        let lanes = vec![Lane { project: "Spruce".into(), repo_slug: Some("sprucehq/spark".into()), branch: Some("portal".into()), ..crate::fixtures::lane("/r/spark-portal", vec![maker]) }];
        // Spark's PRs are read; its docs repo is checked out on its default branch alone.
        let docs = LaneSeed { repo: "docs".into(), repo_slug: Some("SpruceHQ/docs".into()), ..seed("/r/docs", "main") };
        let seeds = vec![seed("/r/spark-portal", "portal"), docs];
        let upstream = PrRaw { number: 8580, url: "https://github.com/Upstream/orm/pull/8580".into(), head_ref: "skip-scan".into(), updated_at: NOW - 1_000, ..pr(PrState::Open, 0) };
        let docs_pr = PrRaw { number: 12, url: "https://github.com/sprucehq/docs/pull/12".into(), head_ref: "photos".into(), ..pr(PrState::Merged, 0) };
        let made = HashMap::from([("Upstream/orm".to_string(), vec![upstream.clone()]), ("sprucehq/docs".to_string(), vec![docs_pr.clone()])]);
        let mut links = PrLinks { worked: HashMap::from([("upstream/orm#8580".to_string(), vec!["maker".to_string()])]), ..Default::default() };
        let listed = |requested: &HashMap<String, Vec<PrRaw>>, lanes: &[Lane], links: &PrLinks| {
            let items = pr_items(&seeds, lanes, &HashMap::new(), requested, &made, Some("gwen"), links);
            items.into_iter().map(|p| (p.key, p.project, p.repo, p.sessions.into_iter().map(|s| s.key).collect::<Vec<_>>())).collect::<Vec<_>>()
        };
        let none = HashMap::new();
        let upstream_item = ("Upstream/orm#8580".to_string(), "Spruce".to_string(), "orm".to_string(), vec!["maker".to_string()]);
        let docs_item = ("SpruceHQ/docs#12".to_string(), "Spruce".to_string(), "docs".to_string(), vec![]);
        // Upstream, under the project of the session that made it, with it; the docs PR in its
        // project's repo, whatever case its link has.
        assert_eq!(listed(&none, &lanes, &links), vec![upstream_item.clone(), docs_item.clone()]);

        // Waiting on your review there too, it's listed once.
        let asked = HashMap::from([("SpruceHQ/docs".to_string(), vec![PrRaw { review_requested: true, ..docs_pr.clone() }])]);
        assert_eq!(listed(&asked, &lanes, &links).len(), 2);

        // A repo no longer read keeps its last read, from before the docs PR merged: the one read
        // by its number is the newer.
        let stale = HashMap::from([("SpruceHQ/docs".to_string(), vec![PrRaw { state: PrState::Open, merged_at: None, ..docs_pr }])]);
        let docs: Vec<PrState> = pr_items(&seeds, &lanes, &stale, &none, &made, Some("gwen"), &links).into_iter().filter(|p| p.repo == "docs").map(|p| p.state).collect();
        assert_eq!(docs, [PrState::Merged]);

        // With no session it's for on the board, or the one there unlinked from it, nothing
        // gives the upstream PR a project.
        assert_eq!(listed(&none, &[], &links), vec![docs_item.clone()]);
        links.unlinked.insert(("upstream/orm#8580".into(), "maker".into()));
        assert_eq!(listed(&none, &lanes, &links), vec![docs_item]);
    }

    /// Another machine's card for open PR `key` of yours, saying whether it owns it.
    fn card(key: &str, state: &str, owner: bool) -> codealong_peer::pr::PrCard {
        serde_json::from_value(serde_json::json!({ "key": key, "repo": "r", "project": "P", "number": 7, "title": "Teams", "url": "", "state": state, "author": "gwen", "head": "teams", "mine": true, "sharedReview": { "owner": owner } })).expect("a card")
    }

    /// Your machines, the other one listing `cards`.
    fn studio(cards: Vec<codealong_peer::pr::PrCard>) -> crate::shared::Machines {
        let m = crate::shared::Machine { key: "aa11".into(), name: "Studio".into(), rank: codealong_peer::feed::Rank::First, online: true, last_seen: Some(NOW), fed_at: Some(NOW), prs: cards, schedules: Default::default() };
        crate::shared::Machines { me: None, others: vec![m] }
    }

    #[test]
    fn the_open_prs_another_machine_owns_are_read_where_this_one_has_a_checkout_of_their_repo() {
        let tag = |short: &str| codealong_peer::model::DeviceTag { name: "Mini".into(), short: short.into(), color: "blue".into(), online: true, last_seen: None };
        let lanes = |slug: Option<&str>, device: Option<&str>| vec![Lane { repo_slug: slug.map(String::from), device: device.map(tag), ..crate::fixtures::lane("/r", vec![]) }];
        let owned = studio(vec![card("O/R#7", "open", true), card("o/r#8", "open", false), card("o/r#9", "merged", true), card("o/elsewhere#1", "open", true)]);
        // Its repo's name in another case is the same repo; one it doesn't own, one that's merged
        // and one in a repo with no checkout here aren't read.
        assert_eq!(owned_elsewhere(&owned, &lanes(Some("o/R"), None)), ["o/r#7"]);
        assert!(owned_elsewhere(&owned, &lanes(None, None)).is_empty(), "no checkout here");
        assert!(owned_elsewhere(&owned, &lanes(Some("o/r"), Some("bb22"))).is_empty(), "another machine's checkout");
    }

    #[test]
    fn the_prs_another_machine_owns_are_read_before_the_ones_worked_on_here() {
        // More PRs worked on than are read by their numbers (ghpoll.rs `MADE_MAX`): the one
        // another machine owns still is, being first.
        let sessions = ["a"].map(crate::fixtures::session);
        let lanes = vec![Lane { repo_slug: Some("o/r".into()), ..crate::fixtures::lane("/w", sessions.to_vec()) }];
        let links = PrLinks { worked: (1..=80).map(|n| (format!("o/r#{n}"), vec!["a".to_string()])).collect(), ..PrLinks::default() };
        let keys = read_by_number(&links, &lanes, &studio(vec![card("o/r#500", "open", true), card("o/r#3", "open", true)]));
        assert_eq!(&keys[..2], ["o/r#3", "o/r#500"]);
        assert_eq!(keys.len(), 81, "#3 once");
    }

    #[test]
    fn the_prs_worked_on_come_latest_marked_first() {
        let worked = |keys: &[(&str, &str)]| keys.iter().map(|(pr, s)| (pr.to_string(), vec![s.to_string()])).collect();
        let on_board = ["a", "b", "c", "linked"].map(crate::fixtures::session);
        let lanes = vec![crate::fixtures::lane("/w", on_board.to_vec())];
        let mut links = PrLinks {
            // A session the board no longer lists made #5.
            worked: worked(&[("o/r#1", "a"), ("o/r#2", "b"), ("o/r#3", "c"), ("o/r#4", "linked"), ("o/r#5", "gone")]),
            made: HashMap::from([
                ("o/r#1".to_string(), vec![("a".to_string(), 100)]),
                ("o/r#2".to_string(), vec![("b".to_string(), 300), ("a".to_string(), 50)]),
                ("o/r#3".to_string(), vec![("c".to_string(), 200)]),
                ("o/r#5".to_string(), vec![("gone".to_string(), 900)]),
                ("o/r#6".to_string(), vec![("a".to_string(), 10)]),
            ]),
            ..Default::default()
        };
        // #6, marked long ago by a, was linked to a by hand today, and #4, which no record marks,
        // before that. #7 was linked long ago, as an automatic review links every PR it reviews.
        links.worked.insert("o/r#6".into(), vec!["a".into()]);
        links.worked.insert("o/r#7".into(), vec!["linked".into()]);
        links.linked_at.extend([("o/r#4".to_string(), 400), ("o/r#6".to_string(), 500), ("o/r#7".to_string(), 1)]);
        // By the latest mark or link: an old link no longer goes before a newer mark.
        assert_eq!(worked_prs(&links, &lanes), ["o/r#6", "o/r#4", "o/r#2", "o/r#3", "o/r#1", "o/r#7"]);
        // Every session it had unlinked by hand: it's no one's.
        links.unlinked.insert(("o/r#3".into(), "c".into()));
        assert_eq!(worked_prs(&links, &lanes), ["o/r#6", "o/r#4", "o/r#2", "o/r#1", "o/r#7"]);
    }

    #[test]
    fn a_prs_primary_is_the_one_picked_else_the_one_that_opened_it_else_the_first_started() {
        let s = |key: &str, started: i64| Session { started_at: Some(started), ..crate::fixtures::session(key) };
        // An automatic review of the PR, in its worktree on its branch, started first of all.
        let review = Session { helper: Some(Helper { pr: Some("o/r#1".into()), ..Default::default() }), ..s("review", 5) };
        let lanes = vec![crate::fixtures::lane("/w", vec![s("a", 30), s("b", 10), s("c", 20), s("copy", 40), review])];
        let listed = |keys: &[&str]| keys.iter().map(|k| PrSession { key: (*k).into(), lane: "/w".into(), pushes: 0, primary: None }).collect::<Vec<_>>();
        let primary = |keys: &[&str], links: &PrLinks| pr_primary(&lanes, &listed(keys), "o/r#1", links).map(|(k, _)| k);
        let mut links = PrLinks::default();
        // Nothing in the records: the one that started first.
        assert_eq!(primary(&["a", "b", "c"], &links).as_deref(), Some("b"));
        assert_eq!(primary(&[], &links), None);
        // The first mark is the one that opened it; a mark without a time comes last, and a copy
        // of its conversation, with the same marks, loses to the one that started first.
        links.made.insert("o/r#1".into(), vec![("a".into(), 0), ("copy".into(), 200), ("c".into(), 200), ("b".into(), 300)]);
        assert_eq!(primary(&["a", "b", "c", "copy"], &links).as_deref(), Some("c"));
        // One that isn't listed (archived, unlinked) leaves it to the next.
        assert_eq!(primary(&["a", "b"], &links).as_deref(), Some("b"));
        // Picked by hand wins, while it's listed.
        links.picked.insert("o/r#1".into(), "a".into());
        assert_eq!(primary(&["a", "b", "c"], &links).as_deref(), Some("a"));
        assert_eq!(primary(&["b", "c"], &links).as_deref(), Some("c"));
        // The review is never it, though it started first, unless it's picked: with nothing else
        // listed, the PR has none.
        let links = PrLinks::default();
        assert_eq!(primary(&["review", "c", "b"], &links).as_deref(), Some("b"));
        assert_eq!(primary(&["review"], &links), None);
        let picked = PrLinks { picked: HashMap::from([("o/r#1".to_string(), "review".to_string())]), ..Default::default() };
        assert_eq!(primary(&["review", "b"], &picked).as_deref(), Some("review"));
    }

    /// From a real PR: one session opened a small PR from its worktree, with a quiet
    /// push that left no record, and another, in a worktree on a branch of its own, took the work
    /// over, pushing it to the PR's branch again and again (`git push origin tiers:portal`).
    #[test]
    fn a_session_that_pushes_to_a_pr_more_than_its_opener_becomes_its_primary() {
        let s = |key: &str, cwd: &str, branch: &str, started: i64| Session { cwd: cwd.into(), branch: Some(branch.into()), started_at: Some(started), last_activity: NOW, ..crate::fixtures::session(key) };
        let lane = |id: &str, slug: &str, branch: &str, sessions: Vec<Session>| Lane { repo_slug: Some(slug.into()), branch: Some(branch.into()), ..crate::fixtures::lane(id, sessions) };
        let opened = NOW - 86_400_000;
        let lanes = vec![
            lane("/r/spark-portal", "sprucehq/spark", "portal", vec![s("opener", "/r/spark-portal", "portal", opened - 3_600_000)]),
            // Another clone on the PR's branch too.
            lane("/r/spark-portal2", "sprucehq/spark", "portal", vec![s("fixer", "/r/spark-portal2", "portal", opened + 1_000)]),
            lane("/r/spark-tiers", "sprucehq/spark", "tiers", vec![s("router", "/r/spark-tiers", "tiers", opened + 2_000)]),
            // A branch of the same name in another repo.
            lane("/r/tool", "acme/tool", "portal", vec![s("elsewhere", "/r/tool", "portal", opened)]),
        ];
        let seeds = vec![seed("/r/spark-portal", "portal")];
        let pushed = |n: usize| (0..n).map(|i| ("portal".to_string(), opened + 3_600_000 * (i as i64 + 1))).collect::<Vec<_>>();
        let card = |links: &PrLinks| {
            let p = pr_card(&seeds, &lanes, "sprucehq/spark", &HashMap::new(), &pr(PrState::Open, 0), Some("gwen"), links, &BTreeSet::new());
            let listed: Vec<(String, u32)> = p.sessions.iter().map(|x| (x.key.clone(), x.pushes)).collect();
            (p.primary.unwrap_or_default(), p.primary_by, listed)
        };
        let key = "sprucehq/spark#1037".to_string();
        // The opener's records show it made the PR; the router's that it edited it later.
        let mut links = PrLinks {
            worked: HashMap::from([(key.clone(), vec!["opener".to_string(), "router".to_string()])]),
            made: HashMap::from([(key.clone(), vec![("opener".to_string(), opened), ("router".to_string(), opened + 3_600_000)])]),
            pushed: HashMap::from([("router".to_string(), pushed(1)), ("elsewhere".to_string(), pushed(5))]),
            ..Default::default()
        };
        // One push each, the opener's left no record: it stays the opener's.
        let (primary, by, listed) = card(&links);
        assert_eq!((primary.as_str(), by), ("opener", Some(PrimaryBy::Opened)));
        assert_eq!(listed, vec![("opener".to_string(), 0), ("fixer".to_string(), 0), ("router".to_string(), 1)], "not the other repo's");
        // A second push takes it over.
        links.pushed.insert("router".into(), pushed(3));
        assert_eq!(card(&links).0, "router");
        assert_eq!(card(&links).1, Some(PrimaryBy::Pushed));
        // As it does without a mark of its own, listed by its pushes alone.
        links.worked.get_mut(&key).unwrap().retain(|k| k != "router");
        links.made.get_mut(&key).unwrap().retain(|(k, _)| k != "router");
        assert_eq!(card(&links).0, "router");
        // An opener that pushed as often keeps it, and one that pushed a fix or two to another's
        // PR doesn't take it.
        links.pushed.insert("opener".into(), pushed(3));
        links.pushed.insert("fixer".into(), pushed(2));
        assert_eq!((card(&links).0.as_str(), card(&links).1), ("opener", Some(PrimaryBy::Opened)));
        // Nor do pushes to the branch for an earlier PR from it.
        links.pushed.get_mut("fixer").unwrap().extend((0..4).map(|i| ("portal".to_string(), opened - 86_400_000 + i)));
        assert_eq!(card(&links).0, "opener");
        // One in another repo, listed by its records, gets nothing for pushing to a branch of the
        // same name there.
        links.worked.get_mut(&key).unwrap().push("elsewhere".into());
        links.made.get_mut(&key).unwrap().push(("elsewhere".into(), opened + 7_200_000));
        links.pushed.insert("elsewhere".into(), pushed(9));
        let (primary, _, listed) = card(&links);
        assert_eq!(primary, "opener");
        assert!(listed.contains(&("elsewhere".to_string(), 0)), "{listed:?}");
        // Pushes after it merged don't count.
        let mut merged = pr(PrState::Merged, 0);
        merged.merged_at = Some(opened + 3_600_000 * 2 + 1);
        links.pushed.insert("opener".into(), vec![]);
        let p = pr_card(&seeds, &lanes, "sprucehq/spark", &HashMap::new(), &merged, Some("gwen"), &links, &BTreeSet::new());
        assert_eq!(p.sessions.iter().find(|x| x.key == "router").map(|x| x.pushes), Some(2));
        // Picked by hand wins.
        links.picked.insert(key.clone(), "fixer".into());
        assert_eq!((card(&links).0.as_str(), card(&links).1), ("fixer", Some(PrimaryBy::Picked)));
        // Without marks or pushes: the one that started first.
        let none = PrLinks::default();
        assert_eq!((card(&none).0.as_str(), card(&none).1), ("opener", Some(PrimaryBy::Started)));
        // A branch an earlier PR freed half an hour before this one opened: the pushes before
        // that were the earlier one's, though they're in the hour before this one.
        let mut earlier = pr(PrState::Merged, 0);
        earlier.number = 1036;
        earlier.created_at = opened - 7_200_000;
        earlier.merged_at = Some(opened - 1_800_000);
        let mut router = (0..3).map(|i| ("portal".to_string(), opened - 2_400_000 + i)).collect::<Vec<_>>();
        router.push(("portal".to_string(), opened - 1_200_000));
        let reused = PrLinks { pushed: HashMap::from([("router".to_string(), router)]), ..Default::default() };
        let router_pushes = |seeds: &[LaneSeed], prs: Vec<PrRaw>, requested: Vec<PrRaw>| {
            let (prs, requested) = (HashMap::from([("sprucehq/spark".to_string(), prs)]), HashMap::from([("sprucehq/spark".to_string(), requested)]));
            let items = pr_items(seeds, &lanes, &prs, &requested, &HashMap::new(), Some("gwen"), &reused);
            items.iter().find(|p| p.number == 1037).and_then(|p| p.sessions.iter().find(|x| x.key == "router")).map(|x| x.pushes)
        };
        assert_eq!(router_pushes(&seeds, vec![earlier.clone(), pr(PrState::Open, 0)], vec![]), Some(1));
        // A branch of the same name in another fork is another branch.
        let forked = PrRaw { head_owner: Some("ken".into()), ..earlier.clone() };
        assert_eq!(router_pushes(&seeds, vec![forked, pr(PrState::Open, 0)], vec![]), Some(4));
        // A PR only the search for reviews lists, in a repo whose PRs aren't read now (no worktree
        // off its default branch), has the earlier one from the repo's last read.
        let unpolled = vec![seed("/r/spark-portal", "main")];
        assert_eq!(router_pushes(&unpolled, vec![earlier], vec![pr(PrState::Open, 0)]), Some(1));
    }

    #[test]
    fn a_pr_of_yours_nudges_by_default_unless_turned_off_for_it() {
        let seeds = vec![seed("/r/spark-portal", "portal")];
        let card = |p: &PrRaw, login: Option<&str>, links: &PrLinks| {
            let item = pr_card(&seeds, &[], "sprucehq/spark", &HashMap::new(), p, login, links, &BTreeSet::new());
            (item.nudge, item.nudge_by_default)
        };
        let open = pr(PrState::Open, 0);
        let mut links = PrLinks::default();
        assert_eq!(card(&open, Some("gwen"), &links), (false, false));
        links.nudge_by_default = true;
        assert_eq!(card(&open, Some("gwen"), &links), (true, true));
        // Someone else's isn't, nor one that ended.
        assert_eq!(card(&open, Some("ken"), &links), (false, false));
        assert_eq!(card(&pr(PrState::Merged, 0), Some("gwen"), &links), (false, false));
        // Turned on for it: on whatever the setting says; off for it: off.
        links.nudged.insert("sprucehq/spark#1037".into());
        assert_eq!(card(&open, Some("ken"), &links), (true, false));
        links.nudged.clear();
        links.not_nudged.insert("sprucehq/spark#1037".into());
        assert_eq!(card(&open, Some("gwen"), &links), (false, false));
    }

    /// A watch is the session's picked as the PR's primary: archived, the next takes over the PR
    /// without it, and it's back with the session. It stays said as the PR ends, unlike the nudge,
    /// so that the ending is told in full.
    #[test]
    fn a_watch_goes_with_the_session_picked_for_it() {
        let s = |key: &str, archived: bool| Session { cwd: "/r/spark-portal".into(), branch: Some("portal".into()), started_at: Some(NOW - 1_000), last_activity: NOW, archived, ..crate::fixtures::session(key) };
        let lanes = |archived: bool| vec![Lane { repo_slug: Some("sprucehq/spark".into()), branch: Some("portal".into()), ..crate::fixtures::lane("/r/spark-portal", vec![s("watcher", archived), s("other", false)]) }];
        let seeds = vec![seed("/r/spark-portal", "portal")];
        let key = "sprucehq/spark#1037".to_string();
        let card = |p: PrRaw, archived: bool, links: &PrLinks| {
            let item = pr_card(&seeds, &lanes(archived), "sprucehq/spark", &HashMap::new(), &p, Some("gwen"), links, &BTreeSet::new());
            (item.watched, item.primary.unwrap_or_default())
        };
        let mut links = PrLinks { watched: HashSet::from([key.clone()]), picked: HashMap::from([(key.clone(), "watcher".to_string())]), worked: HashMap::from([(key.clone(), vec!["watcher".to_string()])]), ..Default::default() };
        assert_eq!(card(pr(PrState::Open, 0), false, &links), (true, "watcher".to_string()));
        assert_eq!(card(pr(PrState::Merged, 0), false, &links), (true, "watcher".to_string()));
        assert_eq!(card(pr(PrState::Open, 0), true, &links), (false, "other".to_string()));
        // Nobody picked, as with a file written before a watch picked one: nobody is watching.
        links.picked.clear();
        assert!(!card(pr(PrState::Open, 0), false, &links).0);
        // Picked, and no watch.
        links.picked.insert(key, "watcher".into());
        links.watched.clear();
        assert!(!card(pr(PrState::Open, 0), false, &links).0);
    }

    /// A session archived once its PR is done with (Mark as done, say) is still listed on it, by
    /// its branch or by its records, as one that made the PR from another repo's checkout is; but
    /// it's never the PR's primary, even picked: the next takes over, or none does.
    #[test]
    fn an_archived_session_stays_listed_on_its_pr_but_isnt_its_primary() {
        let s = |key: &str, cwd: &str, branch: &str, archived: bool| Session { cwd: cwd.into(), branch: Some(branch.into()), started_at: Some(NOW - 3 * 3_600_000), last_activity: NOW, archived, ..crate::fixtures::session(key) };
        // The maker opened the PR from the app's checkout, as its records say; the fixer worked
        // on the PR's branch.
        let lanes = |maker: bool, fixer: bool| {
            vec![
                Lane { repo_slug: Some("sprucehq/spark".into()), branch: Some("portal".into()), ..crate::fixtures::lane("/r/spark-portal", vec![s("fixer", "/r/spark-portal", "portal", fixer)]) },
                Lane { repo_slug: Some("sprucehq/app".into()), branch: Some("main".into()), ..crate::fixtures::lane("/r/app", vec![s("maker", "/r/app", "main", maker)]) },
            ]
        };
        let seeds = vec![seed("/r/spark-portal", "portal")];
        let key = "sprucehq/spark#1037".to_string();
        let mut links = PrLinks { worked: HashMap::from([(key.clone(), vec!["maker".to_string()])]), made: HashMap::from([(key.clone(), vec![("maker".to_string(), NOW - 2 * 86_400_000)])]), ..Default::default() };
        let card = |lanes: &[Lane], links: &PrLinks| {
            let item = pr_card(&seeds, lanes, "sprucehq/spark", &HashMap::new(), &pr(PrState::Merged, 0), Some("gwen"), links, &BTreeSet::new());
            (item.sessions.into_iter().map(|x| (x.key, x.lane)).collect::<Vec<_>>(), item.primary)
        };
        let both = vec![("fixer".to_string(), "/r/spark-portal".to_string()), ("maker".to_string(), "/r/app".to_string())];
        assert_eq!(card(&lanes(false, false), &links), (both.clone(), Some("maker".to_string())));
        assert_eq!(card(&lanes(true, false), &links), (both.clone(), Some("fixer".to_string())));
        assert_eq!(card(&lanes(true, true), &links), (both.clone(), None));
        links.picked.insert(key.clone(), "maker".into());
        assert_eq!(card(&lanes(true, false), &links), (both.clone(), Some("fixer".to_string())));
        // Unlinked by hand, it goes as an unarchived one does.
        links.unlinked.insert((key, "maker".into()));
        assert_eq!(card(&lanes(true, false), &links).0, both[..1]);
    }

    /// An agent in another repo's checkout handed the work to a helper, which opened the PR,
    /// reported and was marked done: the agent takes the PR over, listed on it under its own
    /// worktree, while you haven't archived it, it wasn't taken off by hand, and no other session
    /// that worked on it is left.
    #[test]
    fn a_done_helpers_pr_goes_to_the_session_that_started_it() {
        let s = |key: &str, cwd: &str, branch: &str, archived: bool| Session { cwd: cwd.into(), branch: Some(branch.into()), started_at: Some(NOW - 3 * 3_600_000), last_activity: NOW, archived, ..crate::fixtures::session(key) };
        let helper = |archived: bool| Session { helper: Some(Helper { parent: Some("lead".into()), report: true, ..Default::default() }), ..s("helper", "/r/spark-portal", "portal", archived) };
        // An automatic review the lead asked for, done too: it reads the PR rather than making it.
        let review = Session { helper: Some(Helper { parent: Some("lead".into()), pr: Some("sprucehq/spark#1037".into()), ..Default::default() }), ..s("review", "/r/spark-portal", "portal", true) };
        let lanes = |helper_done: bool, lead_done: bool, fixer: Option<bool>| {
            let mut portal = vec![helper(helper_done), review.clone()];
            portal.extend(fixer.map(|done| s("fixer", "/r/spark-portal", "portal", done)));
            vec![
                Lane { repo_slug: Some("sprucehq/spark".into()), branch: Some("portal".into()), ..crate::fixtures::lane("/r/spark-portal", portal) },
                Lane { repo_slug: Some("sprucehq/infra".into()), branch: Some("main".into()), ..crate::fixtures::lane("/r/infra", vec![s("lead", "/r/infra", "main", lead_done)]) },
            ]
        };
        let seeds = vec![seed("/r/spark-portal", "portal")];
        let key = "sprucehq/spark#1037".to_string();
        let mut links = PrLinks { made: HashMap::from([(key.clone(), vec![("helper".to_string(), NOW - 2 * 3_600_000)])]), ..Default::default() };
        let card = |lanes: &[Lane], links: &PrLinks| {
            let item = pr_card(&seeds, lanes, "sprucehq/spark", &HashMap::new(), &pr(PrState::Open, 0), Some("gwen"), links, &BTreeSet::new());
            let listed: Vec<String> = item.sessions.iter().map(|x| format!("{} {}", x.key, x.lane)).collect();
            (listed, item.primary, item.primary_by, item.taken_over)
        };
        let on_branch = ["helper /r/spark-portal".to_string(), "review /r/spark-portal".to_string()];
        let with_lead = [&on_branch[..], &["lead /r/infra".to_string()]].concat();
        // While the helper isn't done, it's the PR's own.
        assert_eq!(card(&lanes(false, false, None), &links), (on_branch.to_vec(), Some("helper".into()), Some(PrimaryBy::Opened), false));
        // Done, its PR goes to the lead, which didn't do its work.
        assert_eq!(card(&lanes(true, false, None), &links), (with_lead.clone(), Some("lead".into()), Some(PrimaryBy::Helper), true));
        // Not to a lead you archived too, nor past another session at work on the PR's branch.
        assert_eq!(card(&lanes(true, true, None), &links), (on_branch.to_vec(), None, None, false));
        assert_eq!(card(&lanes(true, false, Some(false)), &links).1.as_deref(), Some("fixer"));
        assert_eq!(card(&lanes(true, false, Some(true)), &links).1.as_deref(), Some("lead"));
        // Nor when the most of its work was an archived session's that isn't a helper, which
        // opened it: the helper only looked in on its branch.
        let opened = PrLinks { made: HashMap::from([(key.clone(), vec![("fixer".to_string(), NOW - 2 * 3_600_000)])]), ..Default::default() };
        assert_eq!(card(&lanes(true, false, Some(true)), &opened).1, None);
        // Picked in the PR's menu, which links it, or by a watch, it's the lead's as any
        // session's is, and still not its work.
        links.picked.insert(key.clone(), "lead".into());
        links.worked.insert(key.clone(), vec!["lead".into()]);
        assert_eq!(card(&lanes(true, false, None), &links), (with_lead, Some("lead".into()), Some(PrimaryBy::Picked), true));
        // Linked to it by hand (`codealong pr link`, for a PR opened another way), a session did
        // its work, though nothing else shows it.
        let mut api = lanes(true, true, None);
        api[1].sessions.push(s("api", "/r/infra", "main", false));
        let linked = PrLinks { worked: HashMap::from([(key.clone(), vec!["api".to_string()])]), ..Default::default() };
        assert_eq!(card(&api, &linked).1.as_deref(), Some("api"));
        assert!(!card(&api, &linked).3);
        // Resumed as a copy (nudges.rs `follow` picks it), the copy did none of it either.
        let mut copied = lanes(true, false, None);
        copied[1].sessions.push(s("lead-copy", "/r/infra", "main", false));
        let follow = PrLinks { picked: HashMap::from([(key.clone(), "lead-copy".to_string())]), worked: HashMap::from([(key.clone(), vec!["lead-copy".to_string()])]), made: links.made.clone(), ..Default::default() };
        assert_eq!(card(&copied, &follow).1.as_deref(), Some("lead-copy"));
        assert!(card(&copied, &follow).3);
        // A lead that worked on the PR's branch itself took nothing over.
        let mut there = lanes(true, false, None);
        there[0].sessions.push(Session { cwd: "/r/spark-portal".into(), ..s("lead2", "/r/spark-portal", "portal", false) });
        let picked = PrLinks { picked: HashMap::from([(key.clone(), "lead2".to_string())]), made: links.made.clone(), ..Default::default() };
        let led = |lanes: &mut Vec<Lane>| lanes[0].sessions[0].helper.as_mut().unwrap().parent = Some("lead2".into());
        led(&mut there);
        assert_eq!(card(&there, &picked).2, Some(PrimaryBy::Picked));
        assert!(!card(&there, &picked).3);
        // Its work isn't the lead's: merged, it doesn't make the lead look finished.
        let merged = pr_card(&seeds, &lanes(true, false, None), "sprucehq/spark", &HashMap::new(), &pr(PrState::Merged, 0), Some("gwen"), &links, &BTreeSet::new());
        assert!(merged.taken_over);
        assert!(works(&lanes(true, false, None), &[merged], &[]).get("lead").is_some_and(|w| w.prs.is_empty()));
        // Taken off by hand, it stays off.
        links = PrLinks { made: links.made, unlinked: HashSet::from([(key, "lead".to_string())]), ..Default::default() };
        assert_eq!(card(&lanes(true, false, None), &links), (on_branch.to_vec(), None, None, false));
    }

    /// A PR of yours that GitHub would merge now waits on you to merge it, unless an agent is at
    /// work on it or its primary session waits on you with Merge and done on its strip.
    #[test]
    fn a_ready_pr_of_yours_waits_on_you_to_merge_it_when_nothing_else_brings_it() {
        let s = |key: &str, status: Status| Session { cwd: "/r/spark-portal".into(), branch: Some("portal".into()), started_at: Some(NOW - 3 * 3_600_000), last_activity: NOW, status, ..crate::fixtures::session(key) };
        let lanes = |sessions: Vec<Session>| vec![Lane { repo_slug: Some("sprucehq/spark".into()), branch: Some("portal".into()), ..crate::fixtures::lane("/r/spark-portal", sessions) }];
        let seeds = vec![seed("/r/spark-portal", "portal")];
        let ready = PrRaw { merge: Merge::Clean, writes: Some(true), ..pr(PrState::Approved, 0) };
        let waits_with = |lanes: &[Lane], raw: &PrRaw, login: &str, elsewhere: &BTreeSet<String>| {
            pr_card(&seeds, lanes, "sprucehq/spark", &HashMap::new(), raw, Some(login), &PrLinks::default(), elsewhere).waits_to_merge
        };
        let waits = |lanes: &[Lane], raw: &PrRaw, login: &str| waits_with(lanes, raw, login, &BTreeSet::new());
        let idle = lanes(vec![s("maker", Status::Idle)]);
        assert!(waits(&idle, &ready, "gwen"));
        // Someone else's, one GitHub wouldn't merge yet, a stale approval, and one that's ended don't.
        assert!(!waits(&idle, &ready, "ana"));
        assert!(!waits(&idle, &PrRaw { merge: Merge::Blocked, ..ready.clone() }, "gwen"));
        assert!(!waits(&idle, &PrRaw { checks: Checks { pending: 1, ..ready.checks.clone() }, ..ready.clone() }, "gwen"));
        assert!(!waits(&idle, &PrRaw { stale_approval: Some(StaleApproval { by: vec!["ana".into()], commit: "a11ce55".into() }), ..ready.clone() }, "gwen"));
        assert!(!waits(&idle, &PrRaw { state: PrState::Draft, ..ready.clone() }, "gwen"));
        assert!(!waits(&idle, &PrRaw { state: PrState::Merged, ..ready.clone() }, "gwen"));
        // Its session at work, or waiting on you with Merge and done, brings it to you instead.
        assert!(!waits(&lanes(vec![s("maker", Status::Working)]), &ready, "gwen"));
        assert!(!waits(&lanes(vec![s("maker", Status::Unread)]), &ready, "gwen"));
        assert!(!waits(&lanes(vec![s("maker", Status::Needs)]), &ready, "gwen"));
        // Snoozed, or archived, it doesn't; nor does an automatic review at work keep it.
        assert!(waits(&lanes(vec![Session { snoozed_until: Some(NOW + 60_000), ..s("maker", Status::Unread) }]), &ready, "gwen"));
        assert!(waits(&lanes(vec![Session { archived: true, ..s("maker", Status::Unread) }]), &ready, "gwen"));
        let review = Session { helper: Some(Helper { pr: Some("sprucehq/spark#1037".into()), ..Default::default() }), ..s("review", Status::Working) };
        assert!(waits(&lanes(vec![s("maker", Status::Idle), review]), &ready, "gwen"));
        // Nor with no session known; but not where you may only read its repo, or GitHub didn't
        // say, nor where GitHub merges it by itself.
        assert!(waits(&lanes(vec![]), &ready, "gwen"));
        assert!(!waits(&idle, &PrRaw { writes: Some(false), ..ready.clone() }, "gwen"));
        assert!(!waits(&idle, &PrRaw { writes: None, ..ready.clone() }, "gwen"));
        assert!(!waits(&idle, &PrRaw { merging: true, ..ready.clone() }, "gwen"));
        // Set aside for its side conversation, its session is still at work on it; so is
        // Codealong's review of it once you talk to it yourself.
        assert!(!waits(&lanes(vec![Session { aside: Some(NOW - 60_000), ..s("maker", Status::Needs) }]), &ready, "gwen"));
        let yours = Session { helper: Some(Helper { pr: Some("sprucehq/spark#1037".into()), yours: true, ..Default::default() }), ..s("review", Status::Working) };
        assert!(!waits(&lanes(vec![s("maker", Status::Idle), yours]), &ready, "gwen"));
        // Nor is one the board lists only where it visited.
        assert!(waits(&lanes(vec![Session { visitor: Some("/r/elsewhere".into()), ..s("maker", Status::Unread) }]), &ready, "gwen"));
        // A primary that's a helper whose turn ended isn't in the queue to merge it from: its agent
        // waits on it, not you.
        let helping = Session { helper: Some(Helper { parent: Some("lead".into()), ..Default::default() }), ..s("maker", Status::Unread) };
        assert!(waits(&lanes(vec![helping]), &ready, "gwen"));
        // A session that came to its worktree works elsewhere: that's not the PR's work.
        let visiting = Session { visitor: Some("/r/elsewhere".into()), ..s("guest", Status::Working) };
        assert!(waits(&lanes(vec![s("maker", Status::Idle), visiting]), &ready, "gwen"));
        // Two ready PRs its waiting primary is the primary of: its strip merges the first, and
        // the other waits on you by itself.
        // (Each read as one card while its session was idle, as the board lists them, then
        // weighed together once it waits on you.)
        let unread = lanes(vec![s("maker", Status::Unread)]);
        let card = |raw: &PrRaw| pr_card(&seeds, &idle, "sprucehq/spark", &HashMap::new(), raw, Some("gwen"), &PrLinks::default(), &BTreeSet::new());
        let mut two = vec![card(&ready), card(&PrRaw { number: 1038, ..ready.clone() })];
        merge_waits(&mut two, &unread, &BTreeSet::new());
        assert_eq!(two.iter().map(|p| p.waits_to_merge).collect::<Vec<_>>(), [false, true]);
        // Its primary session took it over from a helper it started, and waits on you for its own
        // work: its strip has no Merge and done for the PR, which waits on you by itself.
        let helper = Session { archived: true, helper: Some(Helper { parent: Some("lead".into()), ..Default::default() }), ..s("helper", Status::Idle) };
        let mut taken = lanes(vec![helper]);
        let infra = |status: Status| Lane { repo_slug: Some("sprucehq/infra".into()), branch: Some("main".into()), ..crate::fixtures::lane("/r/infra", vec![Session { cwd: "/r/infra".into(), branch: Some("main".into()), ..s("lead", status) }]) };
        taken.push(infra(Status::Needs));
        assert!(waits(&taken, &ready, "gwen"));
        // While its taken-over primary is working, it holds the PR out of the merge queue.
        taken[1] = infra(Status::Working);
        assert!(!waits(&taken, &ready, "gwen"));
        // A session working on the PR on another online Mac holds it out too.
        assert!(!waits_with(&idle, &ready, "gwen", &BTreeSet::from(["sprucehq/spark#1037".to_string()])));
    }

    #[test]
    fn working_elsewhere_tracks_active_sessions_on_online_peers() {
        let tag = |online: bool| Some(DeviceTag { name: "Mac mini".into(), short: "abc".into(), color: "teal".into(), online, last_seen: None });
        let sess = |key: &str, status: Status| Session { status, ..crate::fixtures::session(key) };
        let peer_lane = |id: &str, online: bool, slug: Option<&str>, pr_num: Option<u64>, sessions: Vec<Session>| Lane {
            device: tag(online),
            repo_slug: slug.map(String::from),
            pr: pr_num.map(|number| Pr { number, title: String::new(), url: String::new(), state: PrState::Approved, checks: Checks::default(), merge: Merge::Clean, updated_at: 0, merged_at: None, closed_at: None, stale_approval: None }),
            ..crate::fixtures::lane(id, sessions)
        };
        let lanes = vec![
            // Online lane with a working session tied via pr_sessions (different lane than the PR's).
            peer_lane("@abc:/w/lead", true, Some("SpruceHQ/infra"), None, vec![sess("@abc:lead", Status::Working)]),
            // Online lane holding a PR directly with a set-aside session, under a renamed repo too.
            Lane { repo_slug_now: Some("SpruceHQ/spark-new".into()), ..peer_lane("@abc:/w/hold", true, Some("SpruceHQ/spark"), Some(42), vec![Session { aside: Some(NOW - 10), ..sess("@abc:aside", Status::Needs) }]) },
            // Offline lane with a working session: ignored.
            peer_lane("@def:/w/off", false, Some("SpruceHQ/spark"), Some(99), vec![sess("@def:off", Status::Working)]),
            // Online lane with an idle session, a visitor, an archived session, and an unattended auto-review: ignored.
            peer_lane(
                "@abc:/w/skip",
                true,
                Some("SpruceHQ/spark"),
                Some(77),
                vec![
                    sess("@abc:idle", Status::Idle),
                    Session { visitor: Some("/elsewhere".into()), ..sess("@abc:vis", Status::Working) },
                    Session { archived: true, ..sess("@abc:arch", Status::Working) },
                    Session { helper: Some(Helper { pr: Some("sprucehq/spark#77".into()), ..Default::default() }), ..sess("@abc:rev", Status::Working) },
                ],
            ),
        ];
        let pr_sessions = BTreeMap::from([
            ("SpruceHQ/Spark#10".to_string(), vec![PrSession { key: "@abc:lead".into(), lane: "@abc:/w/lead".into(), pushes: 0, primary: None }]),
            ("sprucehq/spark#99".to_string(), vec![PrSession { key: "@def:off".into(), lane: "@def:/w/off".into(), pushes: 0, primary: None }]),
            ("sprucehq/spark#77".to_string(), vec![PrSession { key: "@abc:rev".into(), lane: "@abc:/w/skip".into(), pushes: 0, primary: None }]),
        ]);
        assert_eq!(working_elsewhere(&lanes, &pr_sessions), BTreeSet::from(["sprucehq/spark#10".to_string(), "sprucehq/spark#42".to_string(), "sprucehq/spark-new#42".to_string()]));
    }

    #[test]
    fn a_session_working_in_a_worktree_from_the_checkout_goes_by_the_worktrees_branch() {
        let apps = AppNames::default();
        let mut wt = seed("/r/spark-portal", "portal");
        wt.repo_slug = Some("sprucehq/spark".into());
        let mut main = seed("/r/spark", "main");
        main.repo_slug = Some("sprucehq/spark".into());
        main.default_branch = Some("main".into());
        let seeds = vec![main, wt];
        let a = acct("claude2");
        // Started in the checkout on main, then made the worktree and did all its work there.
        let mut s = claude_session(&a, Some(&reg("idle", NOW)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        s.cwd = "/r/spark".into();
        s.branch = Some("main".into());
        s.work = vec!["/r/spark-portal".into(); 5];
        // Started in the worktree itself, on the branch it had before.
        let mut earlier = s.clone();
        (earlier.id, earlier.key, earlier.cwd, earlier.branch) = ("e".into(), Session::key_for(Harness::Claude, "claude2", "e"), "/r/spark-portal".into(), Some("photos".into()));
        let prs = HashMap::from([("sprucehq/spark".to_string(), vec![pr(PrState::Open, 0)])]);
        let (git, visitors) = (HashMap::new(), HashMap::new());
        let order = vec!["Spruce".to_string()];
        let board = compose(Inputs {
            now: NOW,
            apps: &apps,
            seeds: &seeds,
            sessions: vec![s.clone(), earlier],
            visitors: &visitors,
            git: &git,
            prs: &prs,
            requested: &HashMap::new(),
            made: &HashMap::new(),
            project_order: &order,
            notices: vec![],
            loading: false,
            github_at: None,
            pinned: &[],
            github_login: None,
            pr_links: &PrLinks::default(),
            previews: &Previews::default(),
            reusable: &HashMap::new(),
            review_snoozes: &[],
            session_settings: &Default::default(), auto_reviews: (&Default::default(), &Default::default(), &Default::default()), machines: &Default::default(), at_merged: &HashSet::new(), working_elsewhere: &BTreeSet::new(),
        });
        assert_eq!(board.prs[0].sessions, vec![PrSession { key: s.key.clone(), lane: "/r/spark-portal".into(), pushes: 0, primary: None }]);
        // Main was the checkout's branch, not one it worked on: the windows show the worktree's,
        // and its PR in the session's header.
        let branch = |key: &str| board.lanes.iter().flat_map(|l| &l.sessions).find(|x| x.key == key).unwrap().branch.clone();
        assert_eq!(branch(&s.key), None);
        assert_eq!(branch(&Session::key_for(Harness::Claude, "claude2", "e")).as_deref(), Some("photos"));
    }

    #[test]
    fn a_session_that_moved_to_a_worktree_keeps_the_pr_of_its_folders_branch() {
        let apps = AppNames::default();
        let mut wt = seed("/r/spark-portal", "portal");
        wt.repo_slug = Some("sprucehq/spark".into());
        let mut checkout = seed("/r/spark", "photos");
        checkout.repo_slug = Some("sprucehq/spark".into());
        checkout.default_branch = Some("main".into());
        let seeds = vec![checkout, wt];
        // It worked on the photos PR in the checkout, then went on in the portal worktree.
        let mut s = claude_session(&acct("claude2"), Some(&reg("idle", NOW)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        s.cwd = "/r/spark".into();
        s.branch = Some("photos".into());
        s.work = vec!["/r/spark-portal".into(); 5];
        let mut photos = pr(PrState::Open, 0);
        (photos.number, photos.url, photos.head_ref) = (1038, "https://github.com/sprucehq/spark/pull/1038".into(), "photos".into());
        let prs = HashMap::from([("sprucehq/spark".to_string(), vec![pr(PrState::Open, 0), photos])]);
        let (git, visitors) = (HashMap::new(), HashMap::new());
        let order = vec!["Spruce".to_string()];
        let board = compose(Inputs { now: NOW, apps: &apps, seeds: &seeds, sessions: vec![s.clone()], visitors: &visitors, git: &git, prs: &prs, requested: &HashMap::new(), made: &HashMap::new(), project_order: &order, notices: vec![], loading: false, github_at: None, pinned: &[], github_login: None, pr_links: &PrLinks::default(), previews: &Previews::default(), reusable: &HashMap::new(), review_snoozes: &[], session_settings: &Default::default(), auto_reviews: (&Default::default(), &Default::default(), &Default::default()), machines: &Default::default(), at_merged: &HashSet::new(), working_elsewhere: &BTreeSet::new() });
        let linked = |number: u64| board.prs.iter().find(|p| p.number == number).unwrap().sessions.iter().map(|x| x.key.clone()).collect::<Vec<_>>();
        assert_eq!((linked(1037), linked(1038)), (vec![s.key.clone()], vec![s.key.clone()]));
        let listed = board.lanes.iter().find(|l| l.path == "/r/spark-portal").unwrap();
        assert_eq!(listed.sessions[0].branch, None);
    }

    #[test]
    fn a_long_lived_branch_holds_only_its_latest_pr() {
        let apps = AppNames::default();
        let seeds = vec![seed("/r/spark-portal", "portal")];
        let a = acct("claude2");
        let mut earlier = claude_session(&a, None, Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        earlier.key = "claude:claude2:earlier".into();
        earlier.branch = Some("portal".into());
        earlier.started_at = Some(NOW - 8 * DAY_MS);
        earlier.last_activity = NOW - 6 * DAY_MS;
        let mut latest = earlier.clone();
        latest.key = "claude:claude2:latest".into();
        latest.started_at = Some(NOW - 7_200_000);
        latest.last_activity = NOW - 60_000;
        // The branch's first PR merged five days ago, and the worktree has moved on to the next.
        let mut first = pr(PrState::Merged, 0);
        first.number = 1015;
        first.head_oid = Some("1111111".into());
        first.created_at = NOW - 10 * DAY_MS;
        first.merged_at = Some(NOW - 5 * DAY_MS);
        let prs = HashMap::from([("sprucehq/spark".to_string(), vec![first, pr(PrState::Open, 0)])]);
        let (git, visitors) = (HashMap::new(), HashMap::new());
        let order = vec!["Spruce".to_string()];
        let board = compose(Inputs {
            now: NOW,
            apps: &apps,
            seeds: &seeds,
            sessions: vec![earlier.clone(), latest.clone()],
            visitors: &visitors,
            git: &git,
            prs: &prs,
            requested: &HashMap::new(),
            made: &HashMap::new(),
            project_order: &order,
            notices: vec![],
            loading: false,
            github_at: None,
            pinned: &[],
            github_login: None,
            pr_links: &PrLinks::default(),
            previews: &Previews::default(),
            reusable: &HashMap::new(),
            review_snoozes: &[],
            session_settings: &Default::default(), auto_reviews: (&Default::default(), &Default::default(), &Default::default()), machines: &Default::default(), at_merged: &HashSet::new(), working_elsewhere: &BTreeSet::new(),
        });
        let get = |n: u64| board.prs.iter().find(|p| p.number == n).unwrap();
        let listed = |key: &str| PrSession { key: key.into(), lane: "/r/spark-portal".into(), pushes: 0, primary: None };
        assert_eq!(get(1037).lanes, vec!["/r/spark-portal".to_string()]);
        assert_eq!(get(1037).sessions, vec![listed(&latest.key)]);
        assert!(get(1015).lanes.is_empty(), "the worktree holds the newer PR, as its heading in Sessions shows");
        assert_eq!(get(1015).sessions, vec![listed(&earlier.key)], "its sessions still show, under the worktree they ran in");
    }

    #[test]
    fn prs_link_the_sessions_that_made_or_worked_in_their_worktree() {
        let apps = AppNames::default();
        let scratch = "/private/tmp/claude-501/-r-vault/abc12345-0000-0000-0000-000000000000/scratchpad/spark-nx-cache";
        let seeds = vec![seed("/r/vault", "main"), seed(scratch, "nx-cache")];
        let a = acct("claude");
        let mut maker = claude_session(&a, Some(&reg("idle", NOW)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        maker.id = "abc12345-0000-0000-0000-000000000000".into();
        maker.key = Session::key_for(Harness::Claude, "claude", &maker.id);
        maker.cwd = "/r/vault".into();
        // A session records the branch of the folder it runs in, not the one it made.
        maker.branch = Some("main".into());
        let mut helper = maker.clone();
        helper.id = "helper".into();
        helper.key = "claude:claude:helper".into();
        let mut earlier = helper.clone();
        earlier.id = "earlier".into();
        earlier.key = "claude:claude:earlier".into();
        earlier.started_at = Some(NOW - 3 * DAY_MS);
        earlier.last_activity = NOW - 2 * DAY_MS;
        let visitors = HashMap::from([(PathBuf::from(scratch), vec![helper.key.clone(), earlier.key.clone()])]);
        let (git, order) = (HashMap::new(), vec!["Spruce".to_string()]);
        let board_with = |p: PrRaw| {
            let prs = HashMap::from([("sprucehq/spark".to_string(), vec![PrRaw { head_ref: "nx-cache".into(), ..p }])]);
            compose(Inputs {
                now: NOW,
                apps: &apps,
                seeds: &seeds,
                sessions: vec![maker.clone(), helper.clone(), earlier.clone()],
                visitors: &visitors,
                git: &git,
                prs: &prs,
                requested: &HashMap::new(),
                made: &HashMap::new(),
                project_order: &order,
                notices: vec![],
                loading: false,
                github_at: None,
                pinned: &[],
                github_login: None,
                pr_links: &PrLinks::default(),
                previews: &Previews::default(),
                reusable: &HashMap::new(),
                review_snoozes: &[],
                session_settings: &Default::default(), auto_reviews: (&Default::default(), &Default::default(), &Default::default()), machines: &Default::default(), at_merged: &HashSet::new(), working_elsewhere: &BTreeSet::new(),
            })
        };
        let listed = |key: &str| PrSession { key: key.into(), lane: scratch.into(), pushes: 0, primary: None };
        let board = board_with(pr(PrState::Open, 0));
        assert_eq!(board.prs[0].lanes, vec![scratch.to_string()]);
        // The maker, and the session that worked there while the PR was open, listed under the
        // worktree; not the one that worked there two days before the PR opened.
        assert_eq!(board.prs[0].sessions, vec![listed(&maker.key), listed(&helper.key)]);
        // Sessions that started after the PR merged didn't work on it, even the maker.
        let mut merged = pr(PrState::Merged, 0);
        merged.merged_at = Some(NOW - 2 * 3_600_000);
        assert!(board_with(merged).prs[0].sessions.is_empty());
    }

    #[test]
    fn finds_prs_by_branch_and_owner() {
        let s = seed("/r/spark-portal", "portal");
        let mut prs = HashMap::new();
        prs.insert("sprucehq/spark".to_string(), vec![pr(PrState::Approved, 0)]);
        assert_eq!(find_pr(&s, &prs, NOW).map(|p| p.number), Some(1037));
        prs.insert("sprucehq/spark".to_string(), vec![PrRaw { head_owner: Some("Gwen".into()), ..pr(PrState::Open, 0) }]);
        assert_eq!(find_pr(&s, &prs, NOW).map(|p| p.number), Some(1037), "an owner in another case than the remote's");
        let mut other_owner = pr(PrState::Open, 0);
        other_owner.head_owner = Some("stranger".into());
        prs.insert("sprucehq/spark".to_string(), vec![other_owner]);
        assert!(find_pr(&s, &prs, NOW).is_none());
        let mut main = s.clone();
        main.branch = Some("main".into());
        assert!(find_pr(&main, &prs, NOW).is_none());
        let mut old = pr(PrState::Merged, 0);
        old.merged_at = Some(NOW - 8 * DAY_MS);
        prs.insert("sprucehq/spark".to_string(), vec![old]);
        assert!(find_pr(&s, &prs, NOW).is_none());
        let mut moved_on = pr(PrState::Merged, 0);
        moved_on.head_oid = Some("ffffff0".into());
        prs.insert("sprucehq/spark".to_string(), vec![moved_on]);
        assert!(find_pr(&s, &prs, NOW).is_none(), "a reused branch drops its merged PR");
    }
}
