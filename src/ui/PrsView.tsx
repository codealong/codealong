// The PRs view: every PR in the project's repos. The ones you pinned come first, in your order;
// then reviews asked of you, flagged, with one-click Approve; then yours, then everyone else's;
// merged and closed ones fold at the end. Each row names the local worktree and the sessions that
// worked on the PR. Drag a PR up among the pinned ones to pin it there, a pinned one down to the
// list to unpin it, and any out of the window to pin it to the desktop (useRowDrag.tsx).

import { absolute, ago, count } from '../format';
import { prFlag } from '../prStatus';
import type { Checks, GitInfo, Lane, PrItem, Session } from '../types';
import { PinnedSection } from './PinnedSection';
import { PrPills } from './PrPills';
import { HarnessChip, Icon, MoreChip, PR_ICON, PR_WORD, selects, WorktreeChip } from './primitives';
import { useRowDrag, type DragProps } from './useRowDrag';

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

export interface PrSection {
  id: 'requested' | 'mine' | 'others' | 'done';
  label: string;
  rows: PrRowItem[];
}

export function prIsOpen(pr: PrItem): boolean {
  return pr.state !== 'merged' && pr.state !== 'closed';
}

/** Review requested, Yours, Others, then Merged & closed (newest first). */
export function prSections(rows: PrRowItem[]): PrSection[] {
  const open = rows.filter((r) => prIsOpen(r.pr));
  const done = rows.filter((r) => !prIsOpen(r.pr)).sort((a, b) => (b.pr.mergedAt ?? b.pr.closedAt ?? 0) - (a.pr.mergedAt ?? a.pr.closedAt ?? 0));
  const sections: PrSection[] = [
    { id: 'requested', label: 'Review requested', rows: open.filter((r) => r.pr.reviewRequested) },
    { id: 'mine', label: 'Yours', rows: open.filter((r) => !r.pr.reviewRequested && r.pr.mine) },
    { id: 'others', label: 'Others', rows: open.filter((r) => !r.pr.reviewRequested && !r.pr.mine) },
    { id: 'done', label: 'Merged & closed', rows: done },
  ];
  return sections.filter((s) => s.rows.length > 0);
}

export function ChecksSummary({ checks: c }: { checks: Checks }) {
  if (c.failed > 0) {
    return (
      <span className="ca-checks is-fail" title={c.failing.join(', ')}>
        <Icon name="x" size={11} stroke={2.25} />
        <span>{c.failed} failing</span>
      </span>
    );
  }
  if (c.pending > 0) {
    return (
      <span className="ca-checks is-pending" title={`${c.pending} running`}>
        <Icon name="circle-dashed" size={11} stroke={2} />
        <span>{c.pending} running</span>
      </span>
    );
  }
  if (c.passed > 0) {
    return (
      <span className="ca-checks is-pass" title={`${c.passed} passed`}>
        <Icon name="check" size={11} stroke={2.25} />
        <span>{c.passed}</span>
      </span>
    );
  }
  return null;
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
  selected: boolean;
  approving: boolean;
  onSelect: () => void;
  onOpen: () => void;
  onApprove: () => void;
  onOpenSession: (lane: Lane, session: Session) => void;
  onContextMenu: () => void;
  /** Starts a drag of the row; with a `pinIndex`, it's one of the pinned ones. */
  drag?: DragProps;
}

function PrRow({ item, domId, now, selected, approving, onSelect, onOpen, onApprove, onOpenSession, onContextMenu, drag }: PrRowProps) {
  const { pr } = item;
  const requested = pr.reviewRequested && prIsOpen(pr);
  const flag = prFlag(pr);
  const shown = item.sessions.slice(0, 2);
  return (
    <div
      id={domId}
      role="option"
      aria-selected={selected}
      className={['ca-lane', 'ca-prrow', requested && 'is-requested', flag && `is-flag-${flag}`, selected && 'is-selected', drag?.pinIndex != null && 'is-pinned', drag?.extraClass].filter(Boolean).join(' ')}
      data-pin-index={drag?.pinIndex}
      onPointerDown={drag?.onPointerDown}
      onMouseDown={(e) => selects(e) && onSelect()}
      onDoubleClick={onOpen}
      onContextMenu={(e) => {
        e.preventDefault();
        onContextMenu();
      }}
    >
      {drag && (
        <span className="ca-grip" aria-hidden="true">
          <Icon name="grip-vertical" size={10} />
        </span>
      )}
      <div className="ca-seg ca-seg-glyph">
        <PrGlyph pr={pr} />
      </div>
      <div className="ca-seg ca-seg-main">
        <div className="ca-lane-line1">
          <span className="ca-prrow-num">#{pr.number}</span>
          <span className="ca-lane-headline" title={pr.title}>
            {pr.title}
          </span>
        </div>
        <div className="ca-lane-line2">
          {/* Shows what fits: the agents and the worktree tag drop out whole when there's no
              room (see .ca-prrow-fit), and the PR panel lists them all. */}
          <div className="ca-prrow-fit">
            {/* The repo · branch · author line comes after the pills, so it's the one that
                drops out of the lead. */}
            <span className="ca-prrow-lead">
              <PrPills pr={pr} />
              <span className="ca-prrow-meta" title={`${pr.repoSlug} · ${pr.head}${pr.base ? ` → ${pr.base}` : ''}`}>
                {pr.repo} · {pr.head} · {pr.mine ? 'you' : (pr.author ?? 'unknown')}
              </span>
            </span>
            {shown.length > 0 && (
              <span className="ca-prrow-agents">
                {shown.map(({ lane, session }) => (
                  <HarnessChip key={session.key} session={session} onOpen={() => onOpenSession(lane, session)} inRow />
                ))}
                <MoreChip names={item.sessions.slice(shown.length).map(({ session }) => `${session.account} · ${session.title}`)} />
              </span>
            )}
            {item.lane && <WorktreeChip lane={item.lane} short />}
          </div>
          <span className="ca-lane-right ca-diff">
            +{pr.additions.toLocaleString('en-US')} −{pr.deletions.toLocaleString('en-US')}
          </span>
        </div>
      </div>
      <div className="ca-seg ca-prrow-act">
        {requested && (
          <button
            type="button"
            className="ca-btn ca-btn--primary ca-btn--sm"
            disabled={approving}
            title={`Approve #${pr.number} as you`}
            onMouseDown={(e) => e.stopPropagation()}
            onDoubleClick={(e) => e.stopPropagation()}
            onClick={(e) => {
              e.stopPropagation();
              onApprove();
            }}
          >
            <Icon name="check" size={14} stroke={2} />
            <span>{approving ? 'Approving…' : 'Approve'}</span>
          </button>
        )}
      </div>
      <div className="ca-seg ca-seg-age" title={`Updated ${absolute(pr.updatedAt)}`}>
        {ago(pr.updatedAt, now)}
      </div>
    </div>
  );
}

interface PrsViewProps {
  /** The PRs you pinned, in your order; they're left out of the sections. */
  pinned: PrRowItem[];
  sections: PrSection[];
  now: number;
  selectedKey: string | null;
  domId: (row: PrRowItem) => string;
  showDone: boolean;
  onToggleDone: () => void;
  approving: Set<string>;
  onSelect: (row: PrRowItem) => void;
  onOpen: (row: PrRowItem) => void;
  onApprove: (row: PrRowItem) => void;
  onOpenSession: (lane: Lane, session: Session) => void;
  onContextMenu: (row: PrRowItem) => void;
  /** Puts a PR at insertion point `index` among the pinned ones showing: a pinned one moves
   *  there, another is pinned there. */
  onPlacePinned: (key: string, index: number) => void;
  /** Unpins a PR: its row dragged from the pinned ones down to the list, or out of the window. */
  onUnpin: (key: string) => void;
}

const SECTION_ICON: Record<PrSection['id'], string> = { requested: 'eye', mine: 'user', others: 'pr', done: 'merge' };

export function PrsView(props: PrsViewProps) {
  const { pinned, sections, now, selectedKey, domId, showDone, onToggleDone, approving, onSelect, onOpen, onApprove, onOpenSession, onContextMenu, onPlacePinned, onUnpin } = props;
  const byKey = (key: string) => pinned.find((r) => r.pr.key === key) ?? sections.flatMap((s) => s.rows).find((r) => r.pr.key === key);
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
  /** A PR's row: `index` is its place among the pinned ones. */
  const row = (r: PrRowItem, index: number | null) => (
    <PrRow
      key={r.key}
      item={r}
      domId={domId(r)}
      now={now}
      selected={r.key === selectedKey}
      approving={approving.has(r.pr.key)}
      onSelect={() => onSelect(r)}
      onOpen={() => onOpen(r)}
      onApprove={() => onApprove(r)}
      onOpenSession={onOpenSession}
      onContextMenu={() => onContextMenu(r)}
      drag={drag.rowProps(r.pr.key, index, pinned.length)}
    />
  );
  return (
    <>
      {/* The first pin lands at the top of the list. */}
      {pinned.length === 0 && drag.dropIndex === 0 && <div className="ca-dropline" />}
      {pinned.length > 0 && <PinnedSection items={pinned} row={row} />}
      {drag.ghost}
      {sections.map((s) => (
        <section key={s.id} className={['ca-bayblock', s.id === 'requested' && 'ca-prs-requested'].filter(Boolean).join(' ')}>
          <div className="ca-bay ca-bay--section" role="heading" aria-level={3}>
            <Icon name={SECTION_ICON[s.id]} size={12} />
            <span>{s.label}</span>
            <span className="ca-bay-count">{s.rows.length}</span>
            {s.id === 'done' && (
              <span className="ca-bay-action">
                <button type="button" className="ca-linkbtn" aria-expanded={showDone} onClick={onToggleDone}>
                  {showDone ? 'Hide' : 'Show'}
                </button>
              </span>
            )}
          </div>
          {(s.id !== 'done' || showDone) && (
            <div className="ca-bay-lanes">{s.rows.map((r) => row(r, null))}</div>
          )}
        </section>
      ))}
    </>
  );
}
