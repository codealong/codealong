// The PRs view: every PR in the project's repos. Reviews asked of you come first, flagged, with
// one-click Approve; then yours, then everyone else's; merged and closed ones fold at the end.
// Each row names the local worktree and the sessions that worked on the PR.

import { absolute, ago } from '../format';
import type { Checks, Lane, PrItem, Session } from '../types';
import { HarnessChip, Icon, PR_ICON, PR_WORD } from './primitives';

export interface PrRowItem {
  kind: 'pr';
  key: string;
  pr: PrItem;
  /** The worktree with the PR's branch checked out, if there is one. */
  lane: Lane | null;
  /** The sessions that worked on the PR, with their worktrees. */
  sessions: { lane: Lane; session: Session }[];
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

/** Where a PR stands with its reviewers, in a word or two. */
export function reviewWord(pr: PrItem): string {
  switch (pr.state) {
    case 'approved':
      return 'Approved';
    case 'changes':
      return 'Changes requested';
    case 'draft':
      return 'Draft';
    case 'open':
      return 'Awaiting review';
    default:
      return PR_WORD[pr.state];
  }
}

export function ChecksSummary({ checks: c }: { checks: Checks }) {
  if (c.failed > 0) {
    return (
      <span className="ca-checks is-fail" title={c.failing.join(', ')}>
        <Icon name="x" size={11} stroke={2.25} />
        {c.failed} failing
      </span>
    );
  }
  if (c.pending > 0) {
    return (
      <span className="ca-checks is-pending" title={`${c.pending} running`}>
        <Icon name="circle-dashed" size={11} stroke={2} />
        {c.pending} running
      </span>
    );
  }
  if (c.passed > 0) {
    return (
      <span className="ca-checks is-pass" title={`${c.passed} passed`}>
        <Icon name="check" size={11} stroke={2.25} />
        {c.passed}
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
}

function PrRow({ item, domId, now, selected, approving, onSelect, onOpen, onApprove, onOpenSession, onContextMenu }: PrRowProps) {
  const { pr } = item;
  const requested = pr.reviewRequested && prIsOpen(pr);
  const shown = item.sessions.slice(0, 2);
  return (
    <div
      id={domId}
      role="option"
      aria-selected={selected}
      className={['ca-lane', 'ca-prrow', requested && 'is-requested', selected && 'is-selected'].filter(Boolean).join(' ')}
      onMouseDown={onSelect}
      onDoubleClick={onOpen}
      onContextMenu={(e) => {
        e.preventDefault();
        onContextMenu();
      }}
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
        </div>
        <div className="ca-lane-line2">
          <span className="ca-prrow-meta" title={`${pr.repoSlug} · ${pr.head}${pr.base ? ` → ${pr.base}` : ''}`}>
            {pr.repo} · {pr.head} · {pr.mine ? 'you' : (pr.author ?? 'unknown')}
          </span>
          <span className={`ca-prrow-review is-${pr.state}`}>{reviewWord(pr)}</span>
          <ChecksSummary checks={pr.checks} />
          {shown.map(({ lane, session }) => (
            <HarnessChip key={session.key} session={session} onOpen={() => onOpenSession(lane, session)} />
          ))}
          {item.sessions.length > shown.length && <span className="ca-prrow-more">+{item.sessions.length - shown.length}</span>}
          {item.lane && (
            <span className="ca-tag" title={item.lane.path}>
              <Icon name="folder" size={11} />
              {item.lane.callsign}
            </span>
          )}
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
}

const SECTION_ICON: Record<PrSection['id'], string> = { requested: 'eye', mine: 'user', others: 'pr', done: 'merge' };

export function PrsView(props: PrsViewProps) {
  const { sections, now, selectedKey, domId, showDone, onToggleDone, approving, onSelect, onOpen, onApprove, onOpenSession, onContextMenu } = props;
  return (
    <>
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
            <div className="ca-bay-lanes">
              {s.rows.map((r) => (
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
                />
              ))}
            </div>
          )}
        </section>
      ))}
    </>
  );
}
