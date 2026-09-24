// The selected lane: where it is, how to open it, its sessions, its PR and its git state.

import { useEffect, useRef, useState } from 'react';
import { absolute, ago, count, middleTruncate, tildePath } from '../format';
import { HARNESS_APP } from '../harness';
import type { Board, Lane, Origin, Session, Target } from '../types';
import { BranchLabel, Button, HarnessChip, Icon, Kbd, LaneGlyph, PrChip, Rich, StatusGlyph } from './primitives';

const ORIGIN_WORD: Record<Origin, string> = {
  main: 'Main checkout',
  script: 'Worktree',
  codex: 'Made by Codex',
  'claude-app': 'Made by Claude app',
  scratchpad: 'Made by an agent',
  temp: 'Temporary',
  folder: 'Folder',
  other: 'Worktree',
};

const SESSIONS_SHOWN = 8;

export function OpenInMenu({ targets, defaultId, onOpen }: { targets: Target[]; defaultId: string | null; onOpen: (id: string) => void }) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (!ref.current?.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setOpen(false);
    };
    document.addEventListener('mousedown', onDown);
    document.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('mousedown', onDown);
      document.removeEventListener('keydown', onKey);
    };
  }, [open]);
  const def = targets.find((t) => t.id === defaultId) ?? targets[0];
  if (!def) return null;
  let lastGroup: string | undefined;
  return (
    <div className="ca-openin" ref={ref}>
      <div className="ca-split">
        <button type="button" className="ca-btn ca-btn--primary ca-btn--md ca-split-main" onClick={() => onOpen(def.id)} title={def.detail ?? undefined}>
          <Icon name={def.icon} size={16} />
          <span>{def.label}</span>
        </button>
        <button
          type="button"
          className="ca-btn ca-btn--primary ca-btn--md ca-split-caret"
          aria-haspopup="menu"
          aria-expanded={open}
          aria-label="Other ways to open"
          onClick={() => setOpen(!open)}
        >
          <Icon name="chevron-down" size={14} />
        </button>
      </div>
      {open && (
        <div className="ca-menu" role="menu">
          {targets.map((t) => {
            const sep = lastGroup !== undefined && t.group !== lastGroup;
            lastGroup = t.group;
            return (
              <div key={t.id} role="none">
                {sep && <div className="ca-menu-sep" role="separator" />}
                <button
                  type="button"
                  role="menuitem"
                  className={['ca-menu-item', t.id === def.id && 'is-on'].filter(Boolean).join(' ')}
                  disabled={t.disabled}
                  onClick={() => {
                    setOpen(false);
                    onOpen(t.id);
                  }}
                >
                  <Icon name={t.icon} size={16} />
                  <span className="ca-menu-label">
                    <span>{t.label}</span>
                    {t.detail && <span className="ca-menu-detail">{t.detail}</span>}
                  </span>
                  {t.kbd && <Kbd keys={t.kbd} />}
                </button>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}

function SessionRow({ session: s, now, focused, onOpen }: { session: Session; now: number; focused: boolean; onOpen: (id: string) => void }) {
  const verb = s.stalledSince ? 'Open' : s.status === 'needs' ? 'Respond' : s.status === 'stopped' ? 'Resume' : 'Open';
  const where = s.surface === 'cli' ? 'Ghostty' : s.harness === 'claude' ? 'the Claude app' : HARNESS_APP[s.harness];
  const open = () => onOpen(s.openTarget);
  return (
    <div
      className={['ca-srow', 'ca-srow--button', `is-${s.status}`, focused && 'is-focus'].filter(Boolean).join(' ')}
      role="button"
      tabIndex={0}
      title={`${verb} in ${where}`}
      onClick={open}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          e.stopPropagation();
          open();
        }
      }}
    >
      <StatusGlyph status={s.status} />
      <div className="ca-srow-main">
        <div className="ca-srow-title">{s.title}</div>
        <div className="ca-srow-meta">
          <HarnessChip session={s} withStatus={false} />
          <span className="ca-code" title={s.id}>
            {s.shortId}
          </span>
          <span title={absolute(s.lastActivity)}>{ago(s.lastActivity, now)}</span>
          {s.live && s.pid ? <span>pid {s.pid}</span> : null}
          {s.visitor ? <span>{s.visitor}</span> : null}
        </div>
        {s.excerpt && (
          <div className="ca-srow-excerpt">
            <Rich text={s.excerpt} now={now} />
          </div>
        )}
      </div>
      <span className={`ca-btn ca-btn--sm ${s.status === 'needs' ? 'ca-btn--secondary' : 'ca-btn--ghost'} ca-srow-action`} aria-hidden="true">
        <Icon name="arrow-up-right" size={14} />
        <span>{verb}</span>
      </span>
    </div>
  );
}

interface InspectorProps {
  lane: Lane | null;
  /** The session the selected board row is about, highlighted in the list. */
  focusKey?: string | null;
  apps: Board['apps'];
  now: number;
  onOpen: (targetId: string) => void;
  /** Opens the new-session sheet at this worktree. */
  onNewSession: () => void;
}

export function Inspector({ lane, focusKey, apps, now, onOpen, onNewSession }: InspectorProps) {
  const [showAll, setShowAll] = useState(false);
  useEffect(() => setShowAll(false), [lane?.id]);
  if (!lane) {
    return (
      <aside className="ca-insp ca-insp--empty">
        <p>Select a lane to see its sessions, PR and git state.</p>
      </aside>
    );
  }
  const g = lane.git;
  const pr = lane.pr;
  const branch = lane.branch ?? (lane.detached ? `detached @ ${lane.detached}` : null);
  const sessions = showAll ? lane.sessions : lane.sessions.slice(0, SESSIONS_SHOWN);
  return (
    <aside className="ca-insp" aria-label={`${lane.callsign} details`}>
      <header>
        <div className="ca-insp-title">
          <LaneGlyph lane={lane} />
          <span className="ca-insp-call" title={lane.callsign}>
            {lane.callsign}
          </span>
          <span className="ca-tag">{ORIGIN_WORD[lane.origin]}</span>
        </div>
        <button type="button" className="ca-insp-path" title={`Reveal ${lane.path} in Finder`} onClick={() => onOpen('l|finder')}>
          <Icon name="folder" size={12} />
          <span>{middleTruncate(tildePath(lane.path), 48)}</span>
        </button>
        {branch && (
          <div className="ca-insp-branch">
            <BranchLabel branch={branch} ahead={g?.ahead} behind={g?.behind} dirty={g?.dirty} />
          </div>
        )}
      </header>
      <div className="ca-insp-actions">
        <OpenInMenu targets={lane.targets} defaultId={lane.defaultTarget} onOpen={onOpen} />
        <Button icon="plus" title="New session here (⌘N)" aria-label="New session here" onClick={onNewSession} />
        <Button icon="terminal" title={`New ${apps.terminal} window here`} aria-label={`New ${apps.terminal} window here`} onClick={() => onOpen('l|terminal')} />
        <Button icon="code" title={`Open in ${apps.editor}`} aria-label={`Open in ${apps.editor}`} onClick={() => onOpen('l|editor')} />
        {pr && <Button icon="external" title={`Open #${pr.number} on GitHub`} aria-label={`Open #${pr.number} on GitHub`} onClick={() => onOpen('p|github')} />}
      </div>
      <section className="ca-insp-sec">
        <h4 className="ca-label">Sessions · {lane.sessions.length}</h4>
        {lane.sessions.length === 0 ? (
          <p className="ca-empty">No agent has worked here in the last 14 days.</p>
        ) : (
          <div className="ca-srows">
            {sessions.map((s) => (
              <SessionRow key={s.key} session={s} now={now} focused={s.key === focusKey} onOpen={onOpen} />
            ))}
          </div>
        )}
        {!showAll && lane.sessions.length > SESSIONS_SHOWN && (
          <button type="button" className="ca-more-btn" onClick={() => setShowAll(true)}>
            Show all {lane.sessions.length}
          </button>
        )}
      </section>
      {pr && (
        <section className="ca-insp-sec">
          <h4 className="ca-label">Pull request</h4>
          <div className="ca-prcard">
            <div className="ca-prcard-top">
              <PrChip pr={pr} />
              <span className="ca-meta" title={absolute(pr.updatedAt)}>
                updated {ago(pr.updatedAt, now)}
              </span>
            </div>
            <div className="ca-prcard-title">{pr.title}</div>
            <div className="ca-prcard-checks">
              {pr.checks.failed > 0 && (
                <span className="ca-checks is-fail" title={pr.checks.failing.join(', ')}>
                  <Icon name="x" size={12} stroke={2.25} />
                  {pr.checks.failed} failing
                </span>
              )}
              {pr.checks.pending > 0 && (
                <span className="ca-checks is-pending">
                  <Icon name="circle-dashed" size={12} stroke={2} />
                  {pr.checks.pending} running
                </span>
              )}
              <span className="ca-checks is-pass">
                <Icon name="check" size={12} stroke={2.25} />
                {pr.checks.passed} passed
              </span>
              {pr.checks.skipped > 0 && <span className="ca-meta">{pr.checks.skipped} skipped</span>}
            </div>
            {pr.checks.failing.length > 0 && (
              <div className="ca-prcard-failing">
                {pr.checks.failing.slice(0, 6).map((name) => (
                  <code key={name} className="ca-ic">
                    {name}
                  </code>
                ))}
              </div>
            )}
            <div className="ca-prcard-actions">
              <Button variant="ghost" size="sm" icon="external" onClick={() => onOpen('p|github')}>
                Open in GitHub
              </Button>
              <Button variant="ghost" size="sm" icon="eye" onClick={() => onOpen('p|codex-review')}>
                Review in Codex
              </Button>
            </div>
          </div>
        </section>
      )}
      {g && (
        <section className="ca-insp-sec">
          <h4 className="ca-label">Git</h4>
          <dl className="ca-kv">
            <dt>Upstream</dt>
            <dd className="ca-code">{g.upstream ?? 'None'}</dd>
            <dt>Uncommitted</dt>
            <dd>{g.dirty ? count(g.dirty, 'file') : 'Clean'}</dd>
            {g.added != null && (
              <>
                <dt>Against base</dt>
                <dd>
                  +{g.added.toLocaleString('en-US')} −{(g.removed ?? 0).toLocaleString('en-US')}
                </dd>
              </>
            )}
            <dt>Last commit</dt>
            <dd title={g.lastCommit ?? undefined}>
              {g.lastCommit ? `${g.lastCommit}${g.lastCommitAt ? ` · ${ago(g.lastCommitAt, now)}` : ''}` : 'None'}
            </dd>
          </dl>
        </section>
      )}
    </aside>
  );
}
