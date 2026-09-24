// A PR in the right sidebar: where it stands, where it lives on this Mac (worktrees and the
// sessions that worked on it, drawn like the sidebar's tree), its description as GitHub renders
// it, and its changed files with their diffs.
//
// The description is GitHub's own rendering (`bodyHTML`), which GitHub sanitizes; the page's
// content security policy blocks scripts on top of that. Links in it open in the browser.

import { useEffect, useState, type MouseEvent as ReactMouseEvent } from 'react';
import { openUrl, prDetail, reviewInCodex } from '../api';
import { absolute, ago, middleTruncate } from '../format';
import type { Lane, PrDetail, PrFile, Session } from '../types';
import { PrPills } from './PrPills';
import { Button, Icon, LaneGlyph, StatusGlyph } from './primitives';
import { ChecksSummary, PrGlyph, prIsOpen, syncWords, type PrPlace, type PrRowItem } from './PrsView';

/** Details already fetched, by PR and last update, so going back to a PR is instant. */
const cache = new Map<string, PrDetail>();

const PATCH_LINES = 400;

function Patch({ text }: { text: string }) {
  const [all, setAll] = useState(false);
  const lines = text.split('\n');
  const shown = all ? lines : lines.slice(0, PATCH_LINES);
  return (
    <>
      <pre className="ca-patch">
        {shown.map((l, i) => (
          <span key={i} className={l.startsWith('@@') ? 'is-hunk' : l.startsWith('+') ? 'is-add' : l.startsWith('-') ? 'is-del' : undefined}>
            {l || ' '}
            {'\n'}
          </span>
        ))}
      </pre>
      {!all && lines.length > PATCH_LINES && (
        <button type="button" className="ca-more-btn" onClick={() => setAll(true)}>
          Show all {lines.length.toLocaleString('en-US')} lines
        </button>
      )}
    </>
  );
}

const STATUS_LETTER: Record<string, string> = { added: 'A', removed: 'D', modified: 'M', renamed: 'R', copied: 'C', changed: 'M' };

function FileRow({ file }: { file: PrFile }) {
  const [open, setOpen] = useState(false);
  return (
    <div className={['ca-prfile', open && 'is-open'].filter(Boolean).join(' ')}>
      <button type="button" className="ca-prfile-head" aria-expanded={open} onClick={() => setOpen(!open)} title={file.path}>
        <Icon name="chevron-right" size={12} stroke={2} />
        <span className={`ca-prfile-status is-${file.status}`}>{STATUS_LETTER[file.status] ?? '·'}</span>
        <span className="ca-prfile-path">{middleTruncate(file.path, 60)}</span>
        <span className="ca-diff">
          +{file.additions} −{file.deletions}
        </span>
      </button>
      {open && (file.patch ? <Patch text={file.patch} /> : <p className="ca-empty">GitHub doesn't show a diff for this file here. Open it on GitHub.</p>)}
    </div>
  );
}

const REVIEW_WORD: Record<string, string> = { APPROVED: 'Approved', CHANGES_REQUESTED: 'Requested changes', COMMENTED: 'Commented', DISMISSED: 'Dismissed' };

interface PlaceProps {
  place: PrPlace;
  prNumber: number;
  head: string;
  now: number;
  onShowLane: (lane: Lane) => void;
  onLaneMenu: (lane: Lane) => void;
  onOpenSession: (lane: Lane, session: Session) => void;
  onSessionMenu: (lane: Lane, session: Session) => void;
}

/** What the worktree says next to its name: how it stands against GitHub when it holds the PR,
 *  otherwise what it has moved on to, when that's known. */
function PlaceNote({ place: { lane, holds }, prNumber, head }: { place: PrPlace; prNumber: number; head: string }) {
  if (holds) {
    const words = syncWords(lane.git);
    return words ? <span className="ca-tree-note">{words}</span> : null;
  }
  if (lane.pr && lane.pr.number !== prNumber) return <span className="ca-tree-note">now on #{lane.pr.number}</span>;
  if (lane.branch && lane.branch !== head) {
    return (
      <span className="ca-tree-note">
        now on <code className="ca-ic">{lane.branch}</code>
      </span>
    );
  }
  return null;
}

/** A worktree and the PR's sessions in it, as the sidebar's tree draws them. */
function Place({ place, prNumber, head, now, onShowLane, onLaneMenu, onOpenSession, onSessionMenu }: PlaceProps) {
  const { lane, sessions } = place;
  return (
    <div role="treeitem" aria-expanded={sessions.length > 0 ? true : undefined}>
      <div
        className="ca-tree-node"
        onContextMenu={(e) => {
          e.preventDefault();
          onLaneMenu(lane);
        }}
      >
        <button type="button" className="ca-tree-label" title={`${lane.path}. Click to show it in Worktrees.`} onClick={() => onShowLane(lane)}>
          <LaneGlyph lane={lane} />
          <span className="ca-nav-label">{lane.callsign}</span>
          <PlaceNote place={place} prNumber={prNumber} head={head} />
        </button>
      </div>
      {sessions.length > 0 && (
        <div className="ca-tree-children" role="group">
          {sessions.map((s) => (
            <button
              key={s.key}
              type="button"
              role="treeitem"
              className="ca-tree-session"
              title={`${s.account} · ${s.title}${s.visitor ? ` (${s.visitor})` : ''}. Click to open it where it lives.`}
              onClick={() => onOpenSession(lane, s)}
              onContextMenu={(e) => {
                e.preventDefault();
                onSessionMenu(lane, s);
              }}
            >
              <StatusGlyph status={s.status} size={10} />
              <span className="ca-tree-acct">{s.account}</span>
              <span className="ca-tree-title">{s.title}</span>
              {s.visitor && <span className="ca-tree-age">{s.visitor}</span>}
              <span className="ca-tree-age" title={absolute(s.lastActivity)}>
                {ago(s.lastActivity, now)}
              </span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

interface PrInspectorProps {
  item: PrRowItem;
  now: number;
  approving: boolean;
  onApprove: () => void;
  onOpenSession: (lane: Lane, session: Session) => void;
  onShowLane: (lane: Lane) => void;
  /** Right-click on a worktree. */
  onLaneMenu: (lane: Lane) => void;
  /** Right-click on a session. */
  onSessionMenu: (lane: Lane, session: Session) => void;
}

export function PrInspector({ item, now, approving, onApprove, onOpenSession, onShowLane, onLaneMenu, onSessionMenu }: PrInspectorProps) {
  const { pr } = item;
  const cacheKey = `${pr.key}@${pr.updatedAt}`;
  const [state, setState] = useState<{ key: string; detail?: PrDetail; error?: string }>({ key: '' });

  useEffect(() => {
    const hit = cache.get(cacheKey);
    if (hit) {
      setState({ key: cacheKey, detail: hit });
      return;
    }
    let live = true;
    setState({ key: cacheKey });
    prDetail(pr.repoSlug, pr.number)
      .then((d) => {
        cache.set(cacheKey, d);
        if (live) setState({ key: cacheKey, detail: d });
      })
      .catch((e: unknown) => {
        if (live) setState({ key: cacheKey, error: String(e).replace(/^Error: /, '') });
      });
    return () => {
      live = false;
    };
  }, [cacheKey, pr.repoSlug, pr.number]);

  const detail = state.key === cacheKey ? state.detail : undefined;
  const error = state.key === cacheKey ? state.error : undefined;
  const requested = pr.reviewRequested && prIsOpen(pr);

  // Links in the description open in the browser rather than inside the app.
  const onBodyClick = (e: ReactMouseEvent<HTMLDivElement>) => {
    const a = (e.target as Element).closest('a');
    if (!a) return;
    e.preventDefault();
    const href = a.getAttribute('href') ?? '';
    if (/^https?:\/\//.test(href)) void openUrl(href);
  };

  return (
    <aside className="ca-insp ca-prinsp" aria-label={`#${pr.number} details`}>
      <header>
        <div className="ca-insp-title">
          <PrGlyph pr={pr} />
          <span className="ca-insp-call">#{pr.number}</span>
          <span className="ca-prinsp-status">
            {requested && <span className="ca-tag is-attn">Review requested</span>}
            <PrPills pr={pr} />
          </span>
        </div>
        <h3 className="ca-prinsp-title">{pr.title}</h3>
        <div className="ca-prinsp-meta">
          <span>{pr.repoSlug}</span>
          <span>
            <code className="ca-ic">{pr.head}</code>
            {pr.base && (
              <>
                {' → '}
                <code className="ca-ic">{pr.base}</code>
              </>
            )}
          </span>
          <span>
            by {pr.mine ? 'you' : (pr.author ?? 'unknown')} · opened <span title={absolute(pr.createdAt)}>{ago(pr.createdAt, now)}</span> ago · updated{' '}
            <span title={absolute(pr.updatedAt)}>{ago(pr.updatedAt, now)}</span> ago
          </span>
        </div>
      </header>
      <div className="ca-insp-actions">
        {requested && (
          <Button variant="primary" icon="check" disabled={approving} onClick={onApprove}>
            {approving ? 'Approving…' : 'Approve'}
          </Button>
        )}
        <Button icon="external" onClick={() => void openUrl(pr.url)}>
          GitHub
        </Button>
        <Button icon="eye" onClick={() => void reviewInCodex(pr.url)}>
          Review in Codex
        </Button>
      </div>

      <section className="ca-insp-sec">
        <h4 className="ca-label">Checks and reviews</h4>
        <div className="ca-prinsp-checks">
          <ChecksSummary checks={pr.checks} />
          {pr.checks.pending > 0 && pr.checks.failed > 0 && <span className="ca-meta">{pr.checks.pending} running</span>}
          <span className="ca-meta">
            +{pr.additions.toLocaleString('en-US')} −{pr.deletions.toLocaleString('en-US')} in {pr.changedFiles} {pr.changedFiles === 1 ? 'file' : 'files'}
            {detail ? ` · ${detail.commits} ${detail.commits === 1 ? 'commit' : 'commits'}` : ''}
          </span>
        </div>
        {pr.checks.failing.length > 0 && (
          <div className="ca-prcard-failing">
            {pr.checks.failing.slice(0, 8).map((name) => (
              <code key={name} className="ca-ic">
                {name}
              </code>
            ))}
          </div>
        )}
        {detail && (detail.reviews.length > 0 || detail.requested.length > 0) && (
          <ul className="ca-prinsp-reviews">
            {detail.reviews.map((r) => (
              <li key={r.author} className={`is-${r.state.toLowerCase()}`}>
                <span className="ca-prinsp-reviewer">{r.author}</span>
                <span>{REVIEW_WORD[r.state] ?? r.state}</span>
                {r.at && <span className="ca-meta">{ago(r.at, now)}</span>}
              </li>
            ))}
            {detail.requested.map((who) => (
              <li key={`req-${who}`} className="is-requested">
                <span className="ca-prinsp-reviewer">{who}</span>
                <span>Review requested</span>
              </li>
            ))}
          </ul>
        )}
      </section>

      {item.places.length > 0 && (
        <section className="ca-insp-sec">
          <h4 className="ca-label">On this Mac</h4>
          <div className="ca-tree" role="tree" aria-label="On this Mac">
            {item.places.map((place) => (
              <Place
                key={place.lane.id}
                place={place}
                prNumber={pr.number}
                head={pr.head}
                now={now}
                onShowLane={onShowLane}
                onLaneMenu={onLaneMenu}
                onOpenSession={onOpenSession}
                onSessionMenu={onSessionMenu}
              />
            ))}
          </div>
        </section>
      )}

      <section className="ca-insp-sec">
        <h4 className="ca-label">Description</h4>
        {error && <p className="ca-form-error">{error}</p>}
        {!detail && !error && <p className="ca-empty">Loading…</p>}
        {detail &&
          (detail.bodyHtml.trim() ? (
            <div className="ca-prbody" onClick={onBodyClick} dangerouslySetInnerHTML={{ __html: detail.bodyHtml }} />
          ) : (
            <p className="ca-empty">No description.</p>
          ))}
      </section>

      {detail && (
        <section className="ca-insp-sec">
          <h4 className="ca-label">
            Files changed · {pr.changedFiles}
            {detail.moreFiles ? ` (first ${detail.files.length} shown)` : ''}
          </h4>
          <div className="ca-prfiles">
            {detail.files.map((f) => (
              <FileRow key={f.path} file={f} />
            ))}
          </div>
        </section>
      )}
    </aside>
  );
}
