// The PRs view: every PR in the project's repos, under tabs for all of them, yours and everyone
// else's (`PR_TABS`), each tab counting the PRs in it that need you. The ones you pinned come first
// in every tab, in your order; then, sorted by status, what needs you, flagged: the reviews asked of
// you (Approve is in the PR's panel and its menu), then your own PRs ready to merge, then your own
// that need work; then the reviews Codealong's agent is still on, which are yours once it's done;
// then the PRs an agent of yours is at work on, Fixing what came of their review and Working on
// the ones not ready for it (`prAgentWork`); then the rest under headings of the tab's own
// (`prSections`), the reviews you snoozed among them, the soonest back first.
// Sorted by when opened (`PR_SORTS`, in the bar over the list), the open ones go under the days
// they were opened. Merged and closed ones fold at the end. Each row names its repo (a chip whose
// double-click shows only that repo's PRs), the local worktree and the sessions that worked on
// the PR. Drag a PR up among the pinned ones to pin it there, a pinned one down to the list to
// unpin it, and any out of the window to pin it to the desktop (useRowDrag.tsx). ⌘-click and
// ⇧-click (or ⇧↑ ⇧↓, ⌘A) pick several, to open, approve, snooze, pin or copy together (picks.ts,
// PrPickPanel.tsx). Rows, in the bar over the list, puts each PR on one line (`PR_ROWS`), and Group
// by puts the PRs under the worktrees that hold them or under their repos (groups.ts, Groups.tsx;
// `prPlace`), the ones no worktree here holds under a heading of their own, and Merged & closed
// grouped among themselves.

import { byDay, newestFirst, type DayId } from '../days';
import { absolute, ago, count } from '../format';
import { groupOptions, groupsOf, placeIn, type GroupBy, type Grouped, type ListGroupBy, type Place } from '../groups';
import { selectHow, type SelectHow } from '../picks';
import { helpsAnAgent, listedStatus } from '../lanes';
import { heldForAgent, prFlag, type PrPrimary } from '../prStatus';
import type { GitInfo, Lane, PrItem, Session } from '../types';
import { rowsOptions, type RowLines } from './ListBar';
import { PinnedSection } from './PinnedSection';
import { PrLabels } from './PrLabels';
import { PrPills } from './PrPills';
import { ProjectSaidHere } from './ProjectSaid';
import { cx, HarnessChip, Icon, MoreChip, PR_ICON, PR_WORD, RepoChip, Row, SectionHeader, WorktreeChip } from './primitives';
import { GroupBlock, PinnedGroups, type GroupActs } from './Groups';
import { SnoozeChip } from './LaneRow';
import { useRowDrag, type DragProps } from './useRowDrag';
import { cmd } from '../platform';

export interface PrRowItem {
  kind: 'pr';
  key: string;
  pr: PrItem;
  /** The first worktree that holds the PR, if one does. */
  lane: Lane | null;
  /** The sessions that worked on the PR, with the worktrees they're listed under. */
  sessions: { lane: Lane; session: Session }[];
  /** The same, as the PR panel shows them: each worktree with its sessions. */
  places: PrPlace[];
  /** Its primary session, the one a nudge tells: this Mac's, else another machine's
   *  (boardLists.ts `prRowOf`). */
  primary: PrPrimary | null;
}

/** A worktree on this Mac that holds a PR, or one on any of your Macs that the PR's sessions
 *  worked in. */
export interface PrPlace {
  lane: Lane;
  /** The worktree holds the PR: the Worktrees view shows the PR on it. */
  holds: boolean;
  sessions: Session[];
}

/** Puts each session under its worktree. Worktrees that hold the PR come first. */
export function prPlaces(holding: Lane[], sessions: { lane: Lane; session: Session }[]): PrPlace[] {
  const places: PrPlace[] = holding.map((lane) => ({ lane, holds: true, sessions: [] }));
  for (const { lane, session } of sessions) {
    let place = places.find((p) => p.lane.id === lane.id);
    if (!place) {
      place = { lane, holds: false, sessions: [] };
      places.push(place);
    }
    place.sessions.push(session);
  }
  return places;
}

/** How a worktree that holds a PR stands against its branch on GitHub. */
export function syncWords(g: GitInfo | null): string | null {
  if (!g) return null;
  const parts = [
    g.ahead > 0 ? count(g.ahead, 'unpushed commit') : null,
    g.behind > 0 ? `${count(g.behind, 'commit')} behind` : null,
    g.dirty > 0 ? count(g.dirty, 'uncommitted file') : null,
  ].filter((p): p is string => p !== null);
  if (parts.length > 0) return parts.join(' · ');
  // Without an upstream, git can't say whether the branch is pushed.
  return g.upstream ? 'pushed · clean' : 'clean';
}

/** How much each PR's row says: its repo, branch, author, sessions and worktree on a second line,
 *  or one line with its standing and size at the end. */
export const PR_ROWS = rowsOptions('under each PR, its standing, repo, branch, author, sessions, worktree and size', 'each PR with its standing and size at the end');

/** How the PRs that aren't pinned are ordered. */
export type PrSort = 'status' | 'opened';

/** Whose PRs the view lists: every one, yours, or everyone else's. */
export type PrTab = 'all' | 'mine' | 'others';

export const PR_TABS: { id: PrTab; label: string; title: string }[] = [
  { id: 'all', label: 'All', title: 'Every PR, yours and everyone else’s: what needs you first' },
  { id: 'mine', label: 'Yours', title: 'The PRs you opened: the ones ready to merge or needing work first, then the ones waiting on others, then your drafts' },
  { id: 'others', label: 'Others', title: 'Everyone else’s PRs: the reviews asked of you first, then the ones Codealong’s agent is reviewing, the ones you snoozed, and the ones that don’t ask for you' },
];

/** Whether a PR is listed under a tab. */
export function inPrTab(pr: Pick<PrItem, 'mine'>, tab: PrTab): boolean {
  return tab === 'all' || (tab === 'mine') === pr.mine;
}

/** How many PRs under each tab need you (`prMove`), as the tab counts them. */
export function prTabCounts(rows: PrRowItem[]): Record<PrTab, number> {
  const move = rows.filter((r) => prMove(r) !== null);
  return { all: move.length, mine: move.filter((r) => r.pr.mine).length, others: move.filter((r) => !r.pr.mine).length };
}

export const PR_SORTS: { id: PrSort; label: string; icon: string; title: string }[] = [
  { id: 'status', label: 'Status', icon: 'sort-descending-shapes', title: 'Sort by status: what needs you first, then the reviews Codealong’s agent is on, then the rest' },
  { id: 'opened', label: 'Opened', icon: 'sort-descending-numbers', title: 'Sort by when opened: the newest PR first, under the day it was opened' },
];

/** Whether PRs are grouped, and by what, as the other lists are. */
export type PrGroupBy = ListGroupBy;

/** Group by's buttons over PRs (groups.ts). */
export const PR_GROUPS: { id: PrGroupBy; label: string; icon: string; title: string }[] = groupOptions('PRs');

/** What the heading over PRs that no worktree here holds says. */
export const PR_NONE = 'No worktree here';

/** The sessions listed on a PR whose status it takes on (the status filters, Fixing and Working):
 *  as on the board, not one from another worktree, since one agent that made many worktrees would
 *  otherwise put all of their PRs in Working, nor an archived one. */
export function prSessionsNow(r: Pick<PrRowItem, 'pr' | 'sessions'>): Session[] {
  // Nor a primary that took the PR over and did none of its work (`PrItem.takenOver`), whose work
  // is its own things.
  const taker = (s: Session) => !!r.pr.takenOver && s.key === r.pr.primary;
  return r.sessions.flatMap(({ session }) => (session.visitor || session.archived || taker(session) ? [] : [session]));
}

/** What an agent of yours is doing on an open PR, while one of its sessions, here or on another
 *  machine (`prSessionsNow`), is working: Working on it while it isn't ready for review (a draft),
 *  or Fixing it once it's out for review, where what an agent does is answer what came of that
 *  (changes asked for, comments, failing checks, a conflict or a branch behind its base). Not a
 *  session of Codealong's own review of it (`Helper.pr`) while it works for that review, which is
 *  Agent reviewing's; one you're talking to works for you (lanes.ts `helpsAnAgent`). None with no
 *  session working. */
export function prAgentWork(r: Pick<PrRowItem, 'pr' | 'sessions'>): 'fixing' | 'working' | null {
  const reviewing = (s: Session) => !!s.helper?.pr && helpsAnAgent(s);
  if (!prIsOpen(r.pr) || !prSessionsNow(r).some((s) => !reviewing(s) && listedStatus(s) === 'working')) return null;
  return r.pr.state === 'draft' ? 'working' : 'fixing';
}

/** Why a PR is your move in the list (`prFlag`): a review asked of you whatever else is going on,
 *  but not a merge or work of yours while an agent of yours is at work on it (`prAgentWork`),
 *  which that waits on. Its section, its tab's count, its row's mark and whether its group folds it
 *  away all go by this. */
export function prMove(r: Pick<PrRowItem, 'pr' | 'sessions'>): ReturnType<typeof prFlag> {
  const flag = prFlag(r.pr);
  return flag && (r.pr.reviewRequested || !prAgentWork(r)) ? flag : null;
}

/** Where a PR goes, grouped: the worktree that holds it, or none here; or its repo, by its slug on
 *  GitHub, whether or not a worktree here has it. */
export function prPlace(r: PrRowItem, by: GroupBy): Place {
  if (by === 'worktree') return placeIn(r.lane, by);
  const slug = r.pr.repoSlug;
  return { key: slug, lane: null, repo: { name: r.pr.repo, slug, project: r.pr.project } };
}

export interface PrSection {
  id: 'requested' | 'ready' | 'work' | 'agent' | 'fixing' | 'working' | 'mine' | 'others' | 'waiting' | 'drafts' | 'unasked' | 'snoozed' | 'done' | DayId;
  label: string;
  rows: PrRowItem[];
}

export function prIsOpen(pr: PrItem): boolean {
  return pr.state !== 'merged' && pr.state !== 'closed';
}

/** Your review is asked for, you haven't snoozed it, and Codealong's agent isn't still reviewing
 *  it: it waits on you now. */
export function waitsOnReview(pr: PrItem): boolean {
  return pr.reviewRequested && !pr.snoozedUntil && !heldForAgent(pr) && prIsOpen(pr);
}

/** Your review is asked for, or it's yours and waits on you to merge it (the engine snoozes no
 *  other of yours: board.rs `apply_review_snoozes`), and you snoozed it until a time still to come. */
export function reviewSnoozed(pr: PrItem): pr is PrItem & { snoozedUntil: number } {
  return (pr.reviewRequested || pr.mine) && !!pr.snoozedUntil && prIsOpen(pr);
}

/** The section a snoozed PR is out of until its time: a review's, or a merge's. */
export function snoozedFrom(pr: Pick<PrItem, 'reviewRequested'>): string {
  return pr.reviewRequested ? 'Review requested' : 'Ready to merge';
}

type Standing = 'requested' | 'ready' | 'work' | 'agent' | 'fixing' | 'working' | 'rest';

/** Where an open PR whose review isn't snoozed stands, sorted by status: by what it needs of you
 *  (`prMove`), a review, a merge or work; asked of you but with Codealong's agent's review to come
 *  first; an agent of yours at work on it (`prAgentWork`); or nothing yet. */
function standing(r: PrRowItem): Standing {
  const { pr } = r;
  const flag = prMove(r);
  if (flag && pr.reviewRequested) return 'requested';
  if (flag === 'ready') return 'ready';
  if (flag) return 'work';
  if (pr.reviewRequested && heldForAgent(pr)) return 'agent';
  return prAgentWork(r) ?? 'rest';
}

/** The PRs of a tab (`PR_TABS`). By status, what needs you comes first, under what it needs:
 *  Review requested (the reviews asked of you), Ready to merge and Needs work (your own PRs that
 *  could be merged now, or whose checks fail, with changes asked for, conflicts or an out-of-date
 *  branch); then Agent reviewing (asked of you, but the agent's review comes first:
 *  `heldForAgent`); then Fixing and Working, the PRs an agent of yours is at work on
 *  (`prAgentWork`), out of the sections above but Review requested, since what they'd ask of you
 *  waits on the agent; then the rest under headings of the tab's own: All under Yours and Others,
 *  Yours under Waiting on others and Drafts, Others under Not asking you, after Snoozed (the
 *  soonest back first) there. By when opened: the open ones under the day each was opened, the
 *  newest first, snoozed ones too. Either way Merged & closed (newest first) come last. */
export function prSections(rows: PrRowItem[], sort: PrSort = 'status', now = Date.now(), tab: PrTab = 'all'): PrSection[] {
  const listed = rows.filter((r) => inPrTab(r.pr, tab));
  const done = listed.filter((r) => !prIsOpen(r.pr)).sort((a, b) => (b.pr.mergedAt ?? b.pr.closedAt ?? 0) - (a.pr.mergedAt ?? a.pr.closedAt ?? 0));
  const doneSection: PrSection = { id: 'done', label: 'Merged & closed', rows: done };
  if (sort === 'opened') {
    const newest = newestFirst(listed.filter((r) => prIsOpen(r.pr)), (r) => r.pr.createdAt, (r) => r.key);
    return [...byDay(newest, (r) => r.pr.createdAt, now), doneSection].filter((s) => s.rows.length > 0);
  }
  // Each row's standing once: it reads the PR's sessions.
  const stood = listed.filter((r) => prIsOpen(r.pr) && !reviewSnoozed(r.pr)).map((r) => ({ r, at: standing(r) }));
  const under = (id: Standing) => stood.filter((x) => x.at === id).map((x) => x.r);
  const rest = under('rest');
  const needs: PrSection[] = [
    { id: 'requested', label: 'Review requested', rows: under('requested') },
    { id: 'ready', label: 'Ready to merge', rows: under('ready') },
    { id: 'work', label: 'Needs work', rows: under('work') },
    { id: 'agent', label: 'Agent reviewing', rows: under('agent') },
    { id: 'fixing', label: 'Fixing', rows: under('fixing') },
    { id: 'working', label: 'Working', rows: under('working') },
  ];
  const snoozed: PrSection = {
    id: 'snoozed',
    label: 'Snoozed',
    rows: listed.filter((r) => reviewSnoozed(r.pr)).sort((a, b) => (a.pr.snoozedUntil ?? 0) - (b.pr.snoozedUntil ?? 0) || a.key.localeCompare(b.key)),
  };
  const sections: PrSection[] =
    tab === 'mine'
      ? [...needs, { id: 'waiting', label: 'Waiting on others', rows: rest.filter((r) => r.pr.state !== 'draft') }, { id: 'drafts', label: 'Drafts', rows: rest.filter((r) => r.pr.state === 'draft') }, snoozed, doneSection]
      : tab === 'others'
        ? [...needs, snoozed, { id: 'unasked', label: 'Not asking you', rows: rest }, doneSection]
        : [...needs, { id: 'mine', label: 'Yours', rows: rest.filter((r) => r.pr.mine) }, { id: 'others', label: 'Others', rows: rest.filter((r) => !r.pr.mine) }, snoozed, doneSection];
  return sections.filter((s) => s.rows.length > 0);
}

export function PrGlyph({ pr }: { pr: PrItem }) {
  return (
    <span className={`ca-lane-pricon ca-pr--${pr.state}`}>
      <Icon name={PR_ICON[pr.state]} size={14} stroke={1.75} title={PR_WORD[pr.state]} />
    </span>
  );
}

interface PrRowProps {
  item: PrRowItem;
  domId: string;
  now: number;
  /** On one line: its number, title and labels, then its standing and size at the end. */
  slim: boolean;
  /** Sorted by when opened, its age says when it was opened, not when it was last updated. */
  opened: boolean;
  /** A double-click on its repo's chip: only that repo's PRs, by its slug. */
  onShowRepo?: (slug: string) => void;
  selected: boolean;
  /** A press: alone, or with ⌘ or ⇧ held, picking it with others (picks.ts). */
  onSelect: (how: SelectHow) => void;
  onOpen: () => void;
  onOpenSession: (lane: Lane, session: Session) => void;
  onContextMenu: () => void;
  /** Starts a drag of the row; with a `pinIndex`, it's one of the pinned ones. */
  drag?: DragProps;
  /** Under its worktree's or its repo's heading, which says that for it. */
  under?: GroupBy;
  /** The status section it's listed in, whose heading may say what one of its pills would. */
  section?: PrSection['id'];
}

/** A PR in the list: its number, title and labels, then its repo's chip, its standing (pills), its
 *  branch and author, a session that worked on it and its worktree, and its size. On one line
 *  (`slim`): its number and title, then its labels as they fit, and its repo, standing and size at
 *  the line's end. */
function PrRow({ item, domId, now, slim, opened, onShowRepo, selected, onSelect, onOpen, onOpenSession, onContextMenu, drag, under, section }: PrRowProps) {
  const { pr } = item;
  // Under Ready to merge, its Ready to merge pill only said the heading again.
  const omit = section === 'ready' ? 'ready' : undefined;
  const requested = waitsOnReview(pr);
  const flag = prMove(item);
  // One session, named by its title: the PR's primary one, else the first. Two titles side by side
  // leave neither readable on a row's line; the +n names the rest, and the PR panel lists them all.
  const primary = item.primary?.key;
  const sessions = [...item.sessions].sort((a, b) => Number(b.session.key === primary) - Number(a.session.key === primary));
  const shown = sessions.slice(0, 1);
  const repo = under !== 'repo' && <RepoChip repo={pr.repo} slug={pr.repoSlug} project={pr.project} onShowOnly={onShowRepo && (() => onShowRepo(pr.repoSlug))} />;
  const snooze = reviewSnoozed(pr) && <SnoozeChip until={pr.snoozedUntil} now={now} away={`out of ${snoozedFrom(pr)} and off the floating bar`} />;
  const diff = (
    <span className="ca-diff">
      +{pr.additions.toLocaleString('en-US')} −{pr.deletions.toLocaleString('en-US')}
    </span>
  );
  return (
    <Row
      id={domId}
      selected={selected}
      className={cx('ca-lane', 'is-item', 'ca-prrow', slim && 'is-slim', requested && 'is-requested', flag && `is-flag-${flag}`, drag?.pinIndex != null && 'is-pinned')}
      drag={drag}
      // ⌘ adds it to the ones picked or leaves it out, ⇧ picks the rows up to it.
      onSelect={(e) => onSelect(selectHow(e))}
      onOpen={(e) => {
        if (!cmd(e) && !e.shiftKey) onOpen();
      }}
      onMenu={onContextMenu}
    >
      <div className="ca-seg ca-seg-glyph">
        <PrGlyph pr={pr} />
      </div>
      <div className="ca-seg ca-seg-main">
        <div className="ca-lane-line1">
          <span className="ca-prrow-num">#{pr.number}</span>
          <span className="ca-lane-headline" title={pr.title}>
            {pr.title}
          </span>
          {slim ? (
            <>
              <span className="ca-chips ca-lane-tags">
                {snooze}
                <PrLabels labels={pr.labels} />
              </span>
              <span className="ca-prrow-end">
                {repo}
                <PrPills pr={pr} omit={omit} />
                {diff}
              </span>
            </>
          ) : (
            <PrLabels labels={pr.labels} />
          )}
        </div>
        {!slim && (
          <div className="ca-lane-line2">
            {/* Shows what fits: the agents and the worktree tag drop out whole when there's no
                room (see .ca-prrow-fit), and the PR panel lists them all. */}
            <div className="ca-prrow-fit">
              {/* The branch · author line comes after the repo and the pills, so it's the one
                  that drops out of the lead. */}
              <span className="ca-prrow-lead">
                {repo}
                {snooze}
                <PrPills pr={pr} omit={omit} />
                <span className="ca-prrow-meta" title={`${pr.head}${pr.base ? ` → ${pr.base}` : ''}`}>
                  {pr.head} · {pr.mine ? 'you' : (pr.author ?? 'unknown')}
                </span>
              </span>
              {shown.length > 0 && (
                <span className="ca-prrow-agents">
                  {/* One on another of your Macs opens nowhere from here: its row shows it in the panel. */}
                  {shown.map(({ lane, session }) => (
                    <HarnessChip key={session.key} session={session} named onOpen={session.openTarget ? () => onOpenSession(lane, session) : undefined} inRow />
                  ))}
                  <MoreChip names={sessions.slice(shown.length).map(({ session }) => `${session.title} · ${session.account}`)} />
                </span>
              )}
              {item.lane && under !== 'worktree' && (
                <ProjectSaidHere project={repo ? pr.project : null}>
                  <WorktreeChip lane={item.lane} short />
                </ProjectSaidHere>
              )}
            </div>
            <span className="ca-lane-right">{diff}</span>
          </div>
        )}
      </div>
      <div className="ca-seg ca-seg-age" title={opened ? `Opened ${absolute(pr.createdAt)}, which sets its place in the list. Updated ${absolute(pr.updatedAt)}.` : `Updated ${absolute(pr.updatedAt)}`}>
        {ago(opened ? pr.createdAt : pr.updatedAt, now)}
      </div>
    </Row>
  );
}

interface PrsViewProps {
  /** The PRs you pinned, in your order; they're left out of the sections. */
  pinned: PrRowItem[];
  /** The rest under their headings, in `sort`'s order. */
  sections: PrSection[];
  /** The rest grouped instead, when the list is grouped (groups.ts), and what the groups' headings
   *  do. */
  groups?: Grouped<PrSection, PrRowItem> | null;
  groupActs?: GroupActs;
  /** Sorted by when opened, the rows show when each was opened. */
  sort: PrSort;
  /** A double-click on a row's repo chip: only that repo's PRs, by its slug. */
  onShowRepo?: (slug: string) => void;
  /** Two lines a PR, or one. */
  lines: RowLines;
  now: number;
  selectedKey: string | null;
  /** The rows picked together with the selected one, when there are several. */
  picked: ReadonlySet<string>;
  domId: (row: PrRowItem) => string;
  showDone: boolean;
  onToggleDone: () => void;
  /** A row pressed with the primary button, alone or with ⌘ or ⇧. */
  onSelect: (row: PrRowItem, how: SelectHow) => void;
  onOpen: (row: PrRowItem) => void;
  onOpenSession: (lane: Lane, session: Session) => void;
  onContextMenu: (row: PrRowItem) => void;
  /** Puts a PR at insertion point `index` among the pinned ones showing: a pinned one moves
   *  there, another is pinned there. */
  onPlacePinned: (key: string, index: number) => void;
  /** Unpins a PR: its row dragged from the pinned ones down to the list, or out of the window. */
  onUnpin: (key: string) => void;
}

/** The status headings' icons; the days' have none, as in Sessions. */
const SECTION_ICON: Partial<Record<PrSection['id'], string>> = { requested: 'eye', ready: 'circle-check', work: 'tool', agent: 'hourglass', fixing: 'refresh', working: 'code', mine: 'user', others: 'pr', waiting: 'clock', drafts: 'pr-draft', unasked: 'pr', snoozed: 'zzz', done: 'merge' };

export function PrsView(props: PrsViewProps) {
  const { pinned, sections, groups, groupActs, sort, onShowRepo, lines, now, selectedKey, picked, domId, showDone, onToggleDone, onSelect, onOpen, onOpenSession, onContextMenu, onPlacePinned, onUnpin } = props;
  const byKey = (key: string) => pinned.find((r) => r.pr.key === key) ?? (groups ? groupsOf(groups).flatMap((g) => g.rows) : sections.flatMap((s) => s.rows)).find((r) => r.pr.key === key);
  const drag = useRowDrag({
    onPlace: onPlacePinned,
    joins: true,
    onLeave: onUnpin,
    desktop: (key) => ({ ref: 'pr', key }),
    label: (key) => {
      const r = byKey(key);
      return r ? `#${r.pr.number} ${r.pr.title}` : '';
    },
  });
  /** A PR's row: `index` is its place among the pinned ones; `under`, the heading it's under, and
   *  `section` the status section, whose heading says what a pill of the row's would again. */
  const row = (r: PrRowItem, index: number | null, under?: GroupBy, section?: PrSection['id']) => (
    <PrRow
      key={r.key}
      item={r}
      domId={domId(r)}
      now={now}
      slim={lines === 'one'}
      opened={sort === 'opened'}
      onShowRepo={onShowRepo}
      selected={r.key === selectedKey || picked.has(r.key)}
      onSelect={(how) => onSelect(r, how)}
      onOpen={() => onOpen(r)}
      onOpenSession={onOpenSession}
      onContextMenu={() => onContextMenu(r)}
      drag={drag.rowProps(r.pr.key, index, pinned.length)}
      under={under}
      section={section}
    />
  );
  /** A section's heading, with Merged & closed's way to show or hide its PRs. */
  const header = (s: { id: PrSection['id']; label: string }, count: number) => (
    <SectionHeader
      icon={SECTION_ICON[s.id]}
      label={s.label}
      count={count}
      hint={
        s.id === 'done' && (
          <button type="button" className="ca-linkbtn" aria-expanded={showDone} onClick={onToggleDone}>
            {showDone ? 'Hide' : 'Show'}
          </button>
        )
      }
    />
  );
  /** A group: its heading and its PRs. A press on a worktree's heading shows the worktree. */
  const group = (g: Grouped<PrSection, PrRowItem>['pinned'][number], acts: GroupActs, pin: boolean, section?: PrSection['id']) => (
    <GroupBlock key={`group:${g.key}`} g={g} acts={acts} now={now} rows={false} pinned={pin} none={PR_NONE}>
      {g.rows.map((r) => row(r, null, g.key ? g.by : undefined, section))}
    </GroupBlock>
  );
  return (
    <>
      {/* The first pin lands at the top of the list. */}
      {pinned.length === 0 && drag.dropIndex === 0 && <div className="ca-dropline" />}
      {pinned.length > 0 && <PinnedSection items={pinned} row={row} />}
      {drag.ghost}
      {groups && groupActs ? (
        <>
          {groups.pinned.length > 0 && (
            <PinnedGroups by={groups.by} count={groups.pinned.length} reorder={false}>
              {groups.pinned.map((g) => group(g, groupActs, true))}
            </PinnedGroups>
          )}
          {groups.sections.map((s) => (
            <section key={s.id} className="ca-bayblock">
              {header(s, s.count)}
              {(s.id !== 'done' || showDone) && <div className="ca-bay-lanes">{s.groups.map((g) => group(g, groupActs, false, s.id))}</div>}
            </section>
          ))}
        </>
      ) : (
        sections.map((s) => (
          <section key={s.id} className="ca-bayblock">
            {header(s, s.rows.length)}
            {(s.id !== 'done' || showDone) && <div className="ca-bay-lanes">{s.rows.map((r) => row(r, null, undefined, s.id))}</div>}
          </section>
        ))
      )}
    </>
  );
}
