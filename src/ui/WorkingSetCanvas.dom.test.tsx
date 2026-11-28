// @vitest-environment jsdom
import { describe, expect, it, vi } from 'vitest';
import { createRoot } from 'react-dom/client';
import type { BoardDoc } from '../boards/model';
import { act } from '../testing/dom';
import * as fx from '../testing/fixtures';
import { emptyDoc, entitiesOf, refId, type CanvasDoc } from '../canvas/model';
import { WorkingSetCanvas } from './WorkingSetCanvas';

describe('WorkingSetCanvas', () => {
  it('renders clustered cards, hops through actionable items on Tab, toggles spotlight and shelves with undo', () => {
    window.localStorage.clear();
    const el = document.body.appendChild(document.createElement('div'));
    Object.defineProperty(el, 'clientWidth', { value: 1200, configurable: true });
    Object.defineProperty(el, 'clientHeight', { value: 800, configurable: true });
    const root = createRoot(el);

    const s1 = fx.session({
      key: 'claude:a:1111aaaa',
      id: '1111aaaa',
      title: 'Kestrel rollout publish',
      status: 'needs',
      need: 'approval',
      insight: { note: 'Run publish script after approval.' },
    });
    const s2 = fx.session({
      key: 'claude:a:2222bbbb',
      id: '2222bbbb',
      title: 'Kestrel rollout verification',
      status: 'working',
      insight: { note: 'Waiting on 1111aaaa approval to verify.' },
    });
    const entities = entitiesOf(null, [fx.lane({ id: 'l1', repo: 'atlas', sessions: [s1, s2] })], [], () => true);

    const onSelectItem = vi.fn();
    const onUndo = vi.fn();
    const keys = { current: (_e: KeyboardEvent) => false };

    act(() =>
      root.render(
        <WorkingSetCanvas
          entities={entities}
          schedules={[]}
          prs={[]}
          todos={[]}
          labels={new Map()}
          now={fx.NOW}
          project="Demo"
          query=""
          onSelectItem={onSelectItem}
          onOpen={vi.fn()}
          menuFor={() => []}
          onToast={vi.fn()}
          onUndo={onUndo}
          keys={keys}
        />,
      ),
    );

    // Both sessions rendered inside one auto-clustered halo, with urgent beacon on s1
    expect(el.querySelectorAll('.ca-ws-card').length).toBe(2);
    expect(el.querySelectorAll('.ca-ws-cluster').length).toBe(1);
    expect(el.querySelector('.ca-ws-beacon.tier-urgent')?.textContent).toBe('Approve');

    // Tab jumps straight to the actionable session and notifies MainPanel via onSelectItem
    act(() => {
      keys.current(new KeyboardEvent('keydown', { key: 'Tab' }));
    });
    expect(onSelectItem).toHaveBeenLastCalledWith(expect.objectContaining({ ref: 'session', key: s1.key }));

    // Action-only spotlight dims the non-actionable working card
    const spotlight = el.querySelector<HTMLButtonElement>('.ca-ws-pill');
    act(() => spotlight?.click());
    const workingCard = el.querySelector<HTMLElement>(`[data-ws-node="${refId('session', s2.key)}"]`);
    expect(workingCard?.classList.contains('is-dimmed')).toBe(true);

    // Shelving a card removes it and offers undo
    const shelveBtn = el.querySelector<HTMLButtonElement>(`[data-ws-node="${refId('session', s1.key)}"] .ca-ws-shelve-btn`);
    act(() => shelveBtn?.click());
    expect(el.querySelectorAll('.ca-ws-card').length).toBe(1);
    expect(onUndo).toHaveBeenCalledTimes(1);

    // Running undo restores the shelved card
    const undoFn = onUndo.mock.calls[0][1] as () => void;
    act(() => undoFn());
    expect(el.querySelectorAll('.ca-ws-card').length).toBe(2);

    act(() => root.unmount());
    el.remove();
  });

  it('persists shelve/undo through board ops and cluster renames inside CanvasDoc.map', () => {
    window.localStorage.clear();
    const el = document.body.appendChild(document.createElement('div'));
    Object.defineProperty(el, 'clientWidth', { value: 1200, configurable: true });
    Object.defineProperty(el, 'clientHeight', { value: 800, configurable: true });
    const root = createRoot(el);

    const s1 = fx.session({ key: 'claude:a:aaaa1111', id: 'aaaa1111', title: 'Northwind rollout', status: 'needs', need: 'approval' });
    const s2 = fx.session({ key: 'claude:a:bbbb2222', id: 'bbbb2222', title: 'Northwind check', status: 'working', insight: { note: 'Depends on aaaa1111.' } });
    const entities = entitiesOf(null, [fx.lane({ id: 'l1', repo: 'atlas', sessions: [s1, s2] })], [], () => true);

    const board: BoardDoc = { id: 'b-nw', name: 'Northwind', project: 'Northwind', rule: {}, added: [], removed: [] };
    let doc: CanvasDoc = emptyDoc();
    const update = vi.fn((fn: (d: CanvasDoc) => CanvasDoc) => {
      doc = fn(doc);
    });
    const onTakeOff = vi.fn();
    const onPutOn = vi.fn();
    const onUndo = vi.fn();
    const keys = { current: (_e: KeyboardEvent) => false };

    const renderWith = (d: CanvasDoc) =>
      act(() =>
        root.render(
          <WorkingSetCanvas
            board={board}
            doc={d}
            update={update}
            holds={{ shows: new Set([refId('session', s1.key), refId('session', s2.key)]), has: () => true, removed: new Set() }}
            onTakeOff={onTakeOff}
            onPutOn={onPutOn}
            entities={entities}
            schedules={[]}
            prs={[]}
            todos={[]}
            labels={new Map()}
            now={fx.NOW}
            project="Demo"
            query=""
            onSelectItem={vi.fn()}
            onOpen={vi.fn()}
            menuFor={() => []}
            onToast={vi.fn()}
            onUndo={onUndo}
            keys={keys}
          />,
        ),
      );

    renderWith(doc);
    expect(el.querySelectorAll('.ca-ws-card').length).toBe(2);

    // Shelving delegates to onTakeOff (board_op: remove), and Undo delegates to onPutOn (board_op: add)
    const shelveBtn = el.querySelector<HTMLButtonElement>(`[data-ws-node="${refId('session', s1.key)}"] .ca-ws-shelve-btn`);
    act(() => shelveBtn?.click());
    expect(onTakeOff).toHaveBeenCalledWith([refId('session', s1.key)]);
    const undoFn = onUndo.mock.calls[0][1] as () => void;
    act(() => undoFn());
    expect(onPutOn).toHaveBeenCalledWith([refId('session', s1.key)]);

    // Double-clicking a cluster header renames it into doc.map (not localStorage)
    const head = el.querySelector<HTMLElement>('.ca-ws-cluster-head');
    act(() => {
      head?.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }));
    });
    const input = el.querySelector<HTMLInputElement>('.ca-ws-cluster-input');
    expect(input).toBeTruthy();
    if (input) {
      act(() => {
        input.focus();
        input.value = 'Northwind Rollout';
        input.blur();
      });
    }
    expect(update).toHaveBeenCalled();
    expect(Object.values(doc.map?.groupNames ?? {})).toContain('Northwind Rollout');
    renderWith(doc);
    expect(el.querySelector('.ca-ws-cluster-title')?.textContent).toBe('Northwind Rollout');

    act(() => root.unmount());
    el.remove();
  });
});
