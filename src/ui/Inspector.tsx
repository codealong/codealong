// The selected lane: where it is, how to open it, its sessions, its PR and its git state.

import { Fragment, useEffect, useRef, useState } from 'react';
import { useEditorIcons, withOtherEditors } from '../editors';
import { absolute, ago, count, middleTruncate, tildePath } from '../format';
import { HARNESS_APP } from '../harness';
import { branchLabel } from '../lanes';
import { REUSE_WORDS } from '../reusable';
import type { Board, Lane, Origin, Session, Target, Todo } from '../types';
import { InsightChips } from './InsightBits';
import { BranchLabel, Button, cx, driftWords, HarnessChip, Icon, Kbd, LaneGlyph, PrChip, Rich, StatusGlyph } from './primitives';
import { useDismiss } from './useDismiss';
import { PreviewLink, PreviewSection } from './PreviewBits';
import { TodoChips, TodoGlyph } from './TodoBits';
import { STATUS_WORD } from '../todos';

const ORIGIN_WORD: Record<Origin, string> = {
  main: 'Main worktree',
  script: 'Worktree',
  codex: 'Made by Codex',
  'claude-app': 'Made by Claude',
  scratchpad: 'Made by an agent',
  temp: 'Temporary',
  folder: 'Folder',
  other: 'Worktree',
};

const SESSIONS_SHOWN = 8;

/** A target's app icon, where it has one (an editor's), else its own. */
function TargetIcon({ target }: { target: Target }) {
  return target.image ? <img className="ca-appicon" src={target.image} alt="" width={16} height={16} /> : <Icon name={target.icon} size={16} />;
}

export function OpenInMenu({ targets, defaultId, onOpen, size = 'md' }: { targets: Target[]; defaultId: string | null; onOpen: (id: string) => void; size?: 'sm' | 'md' }) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  useDismiss(ref, () => setOpen(false), { when: open });
  const def = targets.find((t) => t.id === defaultId) ?? targets[0];
  if (!def) return null;
  let lastGroup: string | undefined;
  return (
    <div className="ca-openin" ref={ref}>
      <div className="ca-split">
        <button type="button" className={`ca-btn ca-btn--primary ca-btn--${size} ca-split-main`} onClick={() => onOpen(def.id)} title={def.detail ?? undefined}>
          <TargetIcon target={def} />
          <span>{def.label}</span>
        </button>
        <button
          type="button"
          className={`ca-btn ca-btn--primary ca-btn--${size} ca-split-caret`}
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
                  className={cx('ca-menu-item', t.id === def.id && 'is-on')}
                  disabled={t.disabled}
                  onClick={() => {
                    setOpen(false);
                    onOpen(t.id);
                  }}
                >
                  <TargetIcon target={t} />
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

function SessionRow({ session: s, terminal, now, focused, onOpen, todos, onShowTodo }: { session: Session; terminal: string; now: number; focused: boolean; onOpen: (id: string) => void; todos?: Todo[]; onShowTodo?: (todo: Todo) => void }) {
  const verb = s.stalledSince ? 'Open' : s.status === 'needs' ? 'Respond' : s.status === 'stopped' ? 'Resume' : 'Open';
  const where = s.surface === 'cli' ? terminal : s.harness === 'claude' ? 'the Claude app' : HARNESS_APP[s.harness];
  const open = () => onOpen(s.openTarget);
  return (
    <div
      className={cx('ca-srow', 'ca-srow--button', `is-${s.status}`, focused && 'is-focus')}
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
          <TodoChips todos={todos} onShow={onShowTodo} />
          <InsightChips session={s} />
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
  /** Todos the worktree or its sessions are linked to. */
  todos?: Todo[];
  /** Todos a session is linked to. */
  todosFor?: (sessionKey: string) => Todo[] | undefined;
  onShowTodo?: (todo: Todo) => void;
  /** Right-click on its PR: the PR's menu (open it, copy its link…). */
  onPrMenu?: () => void;
  /** Drops Review with… for its PR from the button. */
  onReviewPr?: (button: Element) => void;
  /** Opens the sheet that sets where its app opens; this device's worktrees only. */
  onSetPreview?: () => void;
}

export function Inspector({ lane, focusKey, apps, now, onOpen, onNewSession, todos, todosFor, onShowTodo, onPrMenu, onReviewPr, onSetPreview }: InspectorProps) {
  const [showAll, setShowAll] = useState(false);
  useEffect(() => setShowAll(false), [lane?.id]);
  const icons = useEditorIcons(apps);
  if (!lane) {
    return (
      <aside className="ca-insp ca-insp--empty">
        <p>Select a lane to see its sessions, PR and git state.</p>
      </aside>
    );
  }
  const g = lane.git;
  const pr = lane.pr;
  const branch = branchLabel(lane);
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
          {lane.reuse && (
            <span className="ca-tag" title={REUSE_WORDS[lane.reuse]}>
              {lane.reuse === 'free' ? 'Reusable · free' : 'Reusable'}
            </span>
          )}
        </div>
        <button type="button" className="ca-insp-path" title={`Reveal ${lane.path} in Finder`} onClick={() => onOpen('l|finder')}>
          <Icon name="folder" size={12} />
          <span>{middleTruncate(tildePath(lane.path), 48)}</span>
        </button>
        <PreviewLink preview={lane.preview} now={now} onOpen={() => onOpen('l|preview')} />
        {branch && (
          <div className="ca-insp-branch">
            <BranchLabel branch={branch} ahead={g?.ahead} behind={g?.behind} dirty={g?.dirty} />
          </div>
        )}
      </header>
      <div className="ca-insp-actions">
        <OpenInMenu targets={withOtherEditors(lane.targets, apps, icons)} defaultId={lane.defaultTarget} onOpen={onOpen} />
        <Button icon="plus" title="New session here (⌘N)" aria-label="New session here" onClick={onNewSession} />
        <Button icon="terminal" title={`New ${apps.terminal} terminal here`} aria-label={`New ${apps.terminal} terminal here`} onClick={() => onOpen('l|terminal')} />
        <Button icon="code" title={`Open in ${apps.editor}`} aria-label={`Open in ${apps.editor}`} onClick={() => onOpen('l|editor')} />
        {pr && <Button icon="external" title={`Open #${pr.number} on GitHub`} aria-label={`Open #${pr.number} on GitHub`} onClick={() => onOpen('p|github')} />}
        {!lane.preview && onSetPreview && !lane.device && <Button icon="world" title="Set preview URL…" aria-label="Set preview URL" onClick={onSetPreview} />}
      </div>
      {todos && todos.length > 0 && (
        <section className="ca-insp-sec">
          <h4 className="ca-label">Todos · {todos.length}</h4>
          <div className="ca-insp-todos">
            {todos.map((t) => (
              <button key={t.id} type="button" className="ca-insp-todo" title="Show it in Todo" onClick={() => onShowTodo?.(t)}>
                <TodoGlyph status={t.status} />
                <span className="ca-insp-todo-title">{t.title || 'Untitled'}</span>
                <span className="ca-meta">{STATUS_WORD[t.status]}</span>
              </button>
            ))}
          </div>
        </section>
      )}
      <PreviewSection lane={lane} now={now} onOpen={() => onOpen('l|preview')} onEdit={() => onSetPreview?.()} />
      <section className="ca-insp-sec">
        <h4 className="ca-label">Sessions · {lane.sessions.length}</h4>
        {lane.sessions.length === 0 ? (
          <p className="ca-empty">No agent has worked here in the last 14 days.</p>
        ) : (
          <div className="ca-srows">
            {sessions.map((s, i) => (
              <Fragment key={s.key}>
                {/* A reusable worktree's sessions from before its latest work come last, under a heading. */}
                {s.earlier && !sessions[i - 1]?.earlier && (
                  <div className="ca-srows-divider" title="Sessions from before this worktree last took new work: another branch checked out, or its branch reset">
                    Earlier work
                  </div>
                )}
                <SessionRow session={s} terminal={apps.terminal} now={now} focused={s.key === focusKey} onOpen={onOpen} todos={todosFor?.(s.key)} onShowTodo={onShowTodo} />
              </Fragment>
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
          <div
            className="ca-prcard"
            onContextMenu={
              onPrMenu
                ? (e) => {
                    e.preventDefault();
                    onPrMenu();
                  }
                : undefined
            }
          >
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
              {onReviewPr && (
                <Button variant="ghost" size="sm" icon="eye" onClick={(e) => onReviewPr(e.currentTarget)}>
                  Review with…
                </Button>
              )}
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
            {g.base && g.baseAhead != null && g.baseBehind != null && (
              <>
                <dt>Base</dt>
                <dd title={driftWords(g) ?? `Has the same commits as ${g.base}`}>
                  <span className="ca-code">{g.base}</span> · {g.baseAhead === 0 && g.baseBehind === 0 ? 'same commits' : `${g.baseAhead.toLocaleString('en-US')} ahead, ${g.baseBehind.toLocaleString('en-US')} behind`}
                </dd>
              </>
            )}
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
