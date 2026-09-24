// The Sessions view: every session in the project, pinned ones first in your order, then the
// rest by when they were last active. Drag a pinned session, or press ⌥↑ ⌥↓, to reorder.

import { useRef, useState, type PointerEvent as ReactPointerEvent, type ReactNode } from 'react';
import type { Lane, Session } from '../types';
import { AgentRow } from './LaneRow';
import { Icon, Kbd } from './primitives';

export interface SessionItem {
  kind: 'agent';
  key: string;
  lane: Lane;
  session: Session;
}

export interface SessionSection {
  id: string;
  label: string;
  rows: SessionItem[];
}

/** Today, Yesterday, Last 7 days and Older, by last activity in local time, newest first. */
export function timeSections(rows: SessionItem[], now: number): SessionSection[] {
  const today = new Date(now);
  today.setHours(0, 0, 0, 0);
  const daysBack = (n: number) => {
    const d = new Date(today);
    d.setDate(d.getDate() - n);
    return d.getTime();
  };
  const [t0, t1, t6] = [today.getTime(), daysBack(1), daysBack(6)];
  const sections: SessionSection[] = [
    { id: 'today', label: 'Today', rows: [] },
    { id: 'yesterday', label: 'Yesterday', rows: [] },
    { id: 'week', label: 'Last 7 days', rows: [] },
    { id: 'older', label: 'Older', rows: [] },
  ];
  for (const r of [...rows].sort((a, b) => b.session.lastActivity - a.session.lastActivity)) {
    const t = r.session.lastActivity;
    sections[t >= t0 ? 0 : t >= t1 ? 1 : t >= t6 ? 2 : 3].rows.push(r);
  }
  return sections.filter((s) => s.rows.length > 0);
}

/**
 * Moves `key` to insertion point `index` among the pinned keys showing, and returns the full
 * pinned list. Pins hidden by the project or the filter keep their places.
 */
export function reorderPinned(all: string[], shown: string[], key: string, index: number): string[] {
  const from = shown.indexOf(key);
  if (from < 0) return all;
  const next = shown.filter((k) => k !== key);
  next.splice(Math.max(0, Math.min(next.length, index > from ? index - 1 : index)), 0, key);
  if (next.every((k, i) => k === shown[i])) return all;
  const slots = new Set(shown);
  let i = 0;
  return all.map((k) => (slots.has(k) ? next[i++] : k));
}

function SectionHeader({ icon, label, count, hint }: { icon?: string; label: string; count: number; hint?: ReactNode }) {
  return (
    <div className="ca-bay ca-bay--section" role="heading" aria-level={3}>
      {icon && <Icon name={icon} size={12} />}
      <span>{label}</span>
      <span className="ca-bay-count">{count}</span>
      {hint && <span className="ca-bay-action ca-bay-hint">{hint}</span>}
    </div>
  );
}

interface SessionsViewProps {
  pinned: SessionItem[];
  sections: SessionSection[];
  now: number;
  selectedKey: string | null;
  domId: (row: SessionItem) => string;
  onSelect: (row: SessionItem) => void;
  onOpen: (row: SessionItem) => void;
  onTogglePin: (row: SessionItem) => void;
  /** Moves a pinned session to insertion point `index` among the pinned ones showing. */
  onMovePinned: (key: string, index: number) => void;
}

export function SessionsView({ pinned, sections, now, selectedKey, domId, onSelect, onOpen, onTogglePin, onMovePinned }: SessionsViewProps) {
  const pinnedRef = useRef<HTMLDivElement>(null);
  const dragRef = useRef<{ key: string; startY: number; active: boolean; index: number } | null>(null);
  const [drag, setDrag] = useState<{ key: string; index: number } | null>(null);

  /** Where a drop at height `y` would land: before the first row whose middle is below it. */
  const insertionIndex = (y: number) => {
    const rows = pinnedRef.current?.querySelectorAll<HTMLElement>('[data-pin-index]') ?? [];
    for (let i = 0; i < rows.length; i++) {
      const r = rows[i].getBoundingClientRect();
      if (y < r.top + r.height / 2) return i;
    }
    return rows.length;
  };

  // A pointer drag, not HTML drag and drop: it behaves the same in every webview and leaves
  // clicks alone until the pointer has moved a few pixels.
  const startDrag = (e: ReactPointerEvent<HTMLDivElement>, key: string) => {
    if (e.button !== 0 || (e.target as HTMLElement).closest('button')) return;
    dragRef.current = { key, startY: e.clientY, active: false, index: -1 };
    const move = (ev: PointerEvent) => {
      const d = dragRef.current;
      if (!d || (!d.active && Math.abs(ev.clientY - d.startY) < 4)) return;
      d.active = true;
      d.index = insertionIndex(ev.clientY);
      setDrag({ key: d.key, index: d.index });
    };
    const end = () => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', end);
      window.removeEventListener('pointercancel', end);
      const d = dragRef.current;
      dragRef.current = null;
      setDrag(null);
      if (d?.active && d.index >= 0) onMovePinned(d.key, d.index);
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', end);
    window.addEventListener('pointercancel', end);
  };

  const row = (r: SessionItem, extra?: { index: number }) => {
    const dragging = drag?.key === r.session.key;
    const dropBefore = drag != null && extra != null && drag.index === extra.index;
    const dropAfter = drag != null && extra != null && extra.index === pinned.length - 1 && drag.index === pinned.length;
    return (
      <AgentRow
        key={r.key}
        lane={r.lane}
        session={r.session}
        domId={domId(r)}
        now={now}
        selected={r.key === selectedKey}
        onSelect={() => onSelect(r)}
        onOpen={() => onOpen(r)}
        pinned={extra != null}
        onTogglePin={() => onTogglePin(r)}
        pinIndex={extra?.index}
        onPointerDown={extra ? (e) => startDrag(e, r.session.key) : undefined}
        extraClass={[dragging && 'is-drag-source', dropBefore && 'is-drop-before', dropAfter && 'is-drop-after'].filter(Boolean).join(' ') || undefined}
      />
    );
  };

  return (
    <>
      {pinned.length > 0 && (
        <section className="ca-bayblock ca-pinned">
          <SectionHeader
            icon="pinned"
            label="Pinned"
            count={pinned.length}
            hint={
              <>
                Drag or <Kbd keys={['⌥', '↑']} /> <Kbd keys={['⌥', '↓']} /> to reorder
              </>
            }
          />
          <div ref={pinnedRef} className={['ca-bay-lanes', drag && 'is-dragging'].filter(Boolean).join(' ')}>
            {pinned.map((r, index) => row(r, { index }))}
          </div>
        </section>
      )}
      {sections.map((s) => (
        <section key={s.id} className="ca-bayblock">
          <SectionHeader label={s.label} count={s.rows.length} />
          <div className="ca-bay-lanes">{s.rows.map((r) => row(r))}</div>
        </section>
      ))}
    </>
  );
}
