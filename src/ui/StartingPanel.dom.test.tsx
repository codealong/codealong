// @vitest-environment jsdom
import { describe, expect, it, vi } from 'vitest';
import { act, engine, press, settle } from '../testing/dom';
import { createRoot } from 'react-dom/client';
import * as fx from '../testing/fixtures';
import type { Starting } from '../types';
import { StartingListRow } from './LaneRow';
import { SessionHeader } from './SessionHeader';
import { StartingPanel } from './StartingPanel';

// QueueStrip and OneLine measure layout with ResizeObserver, which jsdom omits.
if (typeof globalThis.ResizeObserver === 'undefined') {
  globalThis.ResizeObserver = class {
    observe() {}
    unobserve() {}
    disconnect() {}
  };
}

const SCREEN = '  Hooks need review\n  3 hooks are new or changed.\n\n› 1. Review hooks\n  2. Trust all and continue';

function starting(overrides: Partial<Starting> = {}): Starting {
  return {
    at: fx.NOW - 5 * 60_000,
    account: 'codex',
    harness: 'Codex',
    lane: '/r/app',
    place: 'app',
    helper: true,
    opens: 'Draft the release notes',
    terminal: 'Rex',
    stuck: true,
    screen: SCREEN,
    pid: 4242,
    choices: [
      { number: 1, words: 'Review hooks' },
      { number: 2, words: 'Trust all and continue' },
    ],
    ...overrides,
  };
}

function render(node: React.ReactNode) {
  const el = document.body.appendChild(document.createElement('div'));
  const root = createRoot(el);
  act(() => root.render(node));
  return {
    el,
    unmount: () => {
      act(() => root.unmount());
      el.remove();
    },
  };
}

describe('SessionHeader StartingRow', () => {
  it('renders a single clickable line with no inline action buttons (no Forget it or Stop)', () => {
    const lane = fx.lane({ id: '/r/app', callsign: 'app' });
    const session = fx.session({ key: 'boss', title: 'Boss' });
    const st = starting({ pid: null, screen: null, choices: [] });
    const onShowStarting = vi.fn();
    const { el, unmount } = render(
      <SessionHeader
        lane={lane}
        session={session}
        apps={fx.board().apps}
        now={fx.NOW}
        onOpen={() => {}}
        onShowLane={() => {}}
        lineage={{ from: null, to: [], runs: [], sides: [], starting: [st] }}
        onShowStarting={onShowStarting}
      />,
    );
    expect(el.textContent).not.toContain('Forget it');
    expect(el.textContent).not.toContain('Stop');
    const btn = el.querySelector<HTMLButtonElement>('.ca-sessinsp-kin.is-starting');
    expect(btn).not.toBeNull();
    expect(btn!.disabled).toBe(false);
    act(() => btn!.click());
    expect(onShowStarting).toHaveBeenCalledWith(st);
    unmount();
  });
});

describe('StartingPanel', () => {
  it('answers terminal choices, shows its tab, and marks done on click and ⇧⌘E without leaking to parent', async () => {
    engine.answer('start_answer', () => null);
    engine.answer('open_target', () => null);
    engine.answer('start_stop', () => null);
    const lane = fx.lane({ id: '/r/app', callsign: 'app' });
    const session = fx.session({ key: 'boss', title: 'Boss' });
    const st = starting();
    const onDone = vi.fn();
    const onBack = vi.fn();
    const bubbleSawDone = vi.fn();
    const bubbleListener = (e: KeyboardEvent) => {
      if (!e.defaultPrevented) bubbleSawDone();
    };
    window.addEventListener('keydown', bubbleListener);
    const { el, unmount } = render(
      <StartingPanel
        starting={st}
        parent={{ lane, session }}
        now={fx.NOW}
        variant="panel"
        hotkeys={{}}
        back={{ label: session.title, onBack }}
        onDone={onDone}
      />,
    );

    expect(el.querySelector('.ca-startpanel-screen')?.textContent).toBe(SCREEN);
    const choiceBtns = [...el.querySelectorAll<HTMLButtonElement>('.ca-startpanel-choices .ca-btn')];
    expect(choiceBtns.map((b) => b.textContent)).toEqual(['Review hooks', 'Trust all and continue']);

    act(() => choiceBtns[1].click());
    await settle();
    expect(engine.sent('start_answer')).toEqual([
      { at: st.at, asked: 'Hooks need review', number: 2, said: 'Trust all and continue' },
    ]);

    const showBtn = [...el.querySelectorAll<HTMLButtonElement>('.ca-btn')].find((b) => b.textContent?.includes('Show in Rex'))!;
    act(() => showBtn.click());
    await settle();
    expect(engine.sent('open_target')).toEqual([
      { laneId: st.lane, targetId: `start|${st.at}|focus` },
    ]);

    const ev = press('Shift+Cmd+E');
    await settle();
    expect(ev.defaultPrevented).toBe(true);
    expect(bubbleSawDone).not.toHaveBeenCalled();
    expect(engine.sent('start_stop')).toEqual([{ at: st.at }]);
    expect(onDone).toHaveBeenCalledTimes(1);

    window.removeEventListener('keydown', bubbleListener);
    unmount();
  });

  it('offers Done in the header when rendered as a card without a hotkey strip, and explains when no tab was matched', async () => {
    engine.answer('start_stop', () => null);
    const lane = fx.lane({ id: '/r/app', callsign: 'app' });
    const session = fx.session({ key: 'boss', title: 'Boss' });
    const st = starting({ pid: null, screen: null, choices: [] });
    const onDone = vi.fn();
    const { el, unmount } = render(
      <StartingPanel
        starting={st}
        parent={{ lane, session }}
        now={fx.NOW}
        variant="card"
        back={{ label: session.title, onBack: () => {} }}
        onDone={onDone}
      />,
    );

    expect(el.textContent).toContain('can’t tell which tab');
    expect(el.textContent).not.toContain('Show in Rex');
    const doneBtn = [...el.querySelectorAll<HTMLButtonElement>('.ca-btn')].find((b) => b.textContent === 'Done')!;
    expect(doneBtn).toBeDefined();
    act(() => doneBtn.click());
    await settle();
    expect(engine.sent('start_stop')).toEqual([{ at: st.at }]);
    expect(onDone).toHaveBeenCalledTimes(1);
    unmount();
  });
});

describe('StartingListRow', () => {
  it('renders nested under its session, highlights when selected, and invokes onShow on click', () => {
    const st = starting();
    const onShow = vi.fn();
    const { el, unmount } = render(<StartingListRow starting={st} now={fx.NOW} selected onShow={onShow} />);
    const row = el.querySelector<HTMLElement>('.ca-lane.is-nested.is-starting.is-stuck');
    expect(row).not.toBeNull();
    expect(row!.classList.contains('is-selected')).toBe(true);
    expect(row!.querySelector('.ca-agentline-title')?.textContent).toBe('Draft the release notes');
    expect(row!.querySelector('.ca-startingline-state')?.textContent).toBe('“Hooks need review”');
    act(() => {
      row!.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0 }));
    });
    expect(onShow).toHaveBeenCalledWith(st);
    unmount();
  });
});
