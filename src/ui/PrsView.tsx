// The PRs view: every PR in the project's repos. The ones you pinned come first, in your order;
// then, sorted by status, reviews asked of you, flagged (Approve is in the PR's panel and its
// menu); then yours, then everyone else's; then the reviews you snoozed, the soonest back first.
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
import { groupOptions, groupsOf, placeIn, type GroupBy, type Grouped, type Place } from '../groups';
import { selectHow, type SelectHow } from '../picks';
import { heldForAgent, prFlag } from '../prStatus';
import type { GitInfo, Lane, PrItem, Session } from '../types';
import { rowsOptions, type RowLines } from './ListBar';
import { PinnedSection } from './PinnedSection';
import { PrLabels } from './PrLabels';
import { PrPills } from './PrPills';
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
}

/** A worktree on this Mac that holds a PR or that the PR's sessions worked in. */
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

export const PR_SORTS: { id: PrSort; label: string; icon: string; title: string }[] = [
  { id: 'status', label: 'Status', icon: 'sort-descending-shapes', title: 'Sort by status: reviews asked of you first, then yours, then everyone else’s, then the ones you snoozed' },
  { id: 'opened', label: 'Opened', icon: 'sort-descending-numbers', title: 'Sort by when opened: the newest PR first, under the day it was opened' },
];

/** Group by's buttons over PRs (groups.ts). */
export const PR_GROUPS = groupOptions('PRs');

/** What the heading over PRs that no worktree here holds says. */
export const PR_NONE = 'No worktree here';

/** Where a PR goes, grouped: the worktree that holds it, or none here; its repo, by its slug on
 *  GitHub, whether or not a worktree here has it. */
export function prPlace(r: PrRowItem, by: GroupBy): Place {
  if (by === 'worktree') return placeIn(r.lane, by);
  const slug = r.pr.repoSlug;
  return { key: slug, lane: null, repo: { name: r.pr.repo, slug } };
}

export interface PrSection {
  id: 'requested' | 'mine' | 'others' | 'snoozed' | 'done' | DayId;
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

/** Your review is asked for and you snoozed it until a time still to come. */
export function reviewSnoozed(pr: PrItem): pr is PrItem & { snoozedUntil: number } {
  return pr.reviewRequested && !!pr.snoozedUntil && prIsOpen(pr);
}

/** By status: Review requested, Yours, Others, Snoozed (the soonest back first). By when opened:
 *  the open ones under the day each was opened, the newest first, snoozed ones too. Either way
 *  Merged & closed (newest first) come last. */
export function prSections(rows: PrRowItem[], sort: PrSort = 'status', now = Date.now()): PrSection[] {
  const done = rows.filter((r) => !prIsOpen(r.pr)).sort((a, b) => (b.pr.mergedAt ?? b.pr.closedAt ?? 0) - (a.pr.mergedAt ?? a.pr.closedAt ?? 0));
  const doneSection: PrSection = { id: 'done', label: 'Merged & closed', rows: done };
  if (sort === 'opened') {
    const newest = newestFirst(rows.filter((r) => prIsOpen(r.pr)), (r) => r.pr.createdAt, (r) => r.key);
    return [...byDay(newest, (r) => r.pr.createdAt, now), doneSection].filter((s) => s.rows.length > 0);
  }
  const open = rows.filter((r) => prIsOpen(r.pr) && !reviewSnoozed(r.pr));
  const snoozed = rows.filter((r) => reviewSnoozed(r.pr)).sort((a, b) => (a.pr.snoozedUntil ?? 0) - (b.pr.snoozedUntil ?? 0) || a.key.localeCompare(b.key));
  const sections: PrSection[] = [
    { id: 'requested', label: 'Review requested', rows: open.filter((r) => r.pr.reviewRequested) },
    { id: 'mine', label: 'Yours', rows: open.filter((r) => !r.pr.reviewRequested && r.pr.mine) },
    { id: 'others', label: 'Others', rows: open.filter((r) => !r.pr.reviewRequested && !r.pr.mine) },
    { id: 'snoozed', label: 'Snoozed', rows: snoozed },
    doneSection,
  ];
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
}

/** A PR in the list: its number, title and labels, then its repo's chip, its standing (pills), its
 *  branch and author, a session that worked on it and its worktree, and its size. On one line
 *  (`slim`): its number and title, then its labels as they fit, and its repo, standing and size at
 *  the line's end. */
function PrRow({ item, domId, now, slim, opened, onShowRepo, selected, onSelect, onOpen, onOpenSession, onContextMenu, drag, under }: PrRowProps) {
  const { pr } = item;
  const requested = waitsOnReview(pr);
  const flag = prFlag(pr);
  // One session, named by its title: the PR's primary one, else the first. Two titles side by side
  // leave neither readable on a row's line; the +n names the rest, and the PR panel lists them all.
  const sessions = [...item.sessions].sort((a, b) => Number(b.session.key === pr.primary) - Number(a.session.key === pr.primary));
  const shown = sessions.slice(0, 1);
  const repo = under !== 'repo' && <RepoChip repo={pr.repo} slug={pr.repoSlug} onShowOnly={onShowRepo && (() => onShowRepo(pr.repoSlug))} />;
  const snooze = reviewSnoozed(pr) && <SnoozeChip until={pr.snoozedUntil} now={now} away="out of Review requested and off the floating bar" />;
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
                <PrPills pr={pr} />
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
                <PrPills pr={pr} />
                <span className="ca-prrow-meta" title={`${pr.head}${pr.base ? ` → ${pr.base}` : ''}`}>
                  {pr.head} · {pr.mine ? 'you' : (pr.author ?? 'unknown')}
                </span>
              </span>
              {shown.length > 0 && (
                <span className="ca-prrow-agents">
                  {shown.map(({ lane, session }) => (
                    <HarnessChip key={session.key} session={session} named onOpen={() => onOpenSession(lane, session)} inRow />
                  ))}
                  <MoreChip names={sessions.slice(shown.length).map(({ session }) => `${session.title} · ${session.account}`)} />
                </span>
              )}
              {item.lane && under !== 'worktree' && <WorktreeChip lane={item.lane} short />}
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
const SECTION_ICON: Partial<Record<PrSection['id'], string>> = { requested: 'eye', mine: 'user', others: 'pr', snoozed: 'zzz', done: 'merge' };

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
  /** A PR's row: `index` is its place among the pinned ones; `under`, the heading it's under. */
  const row = (r: PrRowItem, index: number | null, under?: GroupBy) => (
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
  const group = (g: Grouped<PrSection, PrRowItem>['pinned'][number], acts: GroupActs, pin: boolean) => (
    <GroupBlock key={`group:${g.key}`} g={g} acts={acts} now={now} rows={false} pinned={pin} none={PR_NONE}>
      {g.rows.map((r) => row(r, null, g.key ? g.by : undefined))}
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
            <section key={s.id} className={cx('ca-bayblock', s.id === 'requested' && 'ca-prs-requested')}>
              {header(s, s.count)}
              {(s.id !== 'done' || showDone) && <div className="ca-bay-lanes">{s.groups.map((g) => group(g, groupActs, false))}</div>}
            </section>
          ))}
        </>
      ) : (
        sections.map((s) => (
          <section key={s.id} className={cx('ca-bayblock', s.id === 'requested' && 'ca-prs-requested')}>
            {header(s, s.rows.length)}
            {(s.id !== 'done' || showDone) && <div className="ca-bay-lanes">{s.rows.map((r) => row(r, null))}</div>}
          </section>
        ))
      )}
    </>
  );
}
