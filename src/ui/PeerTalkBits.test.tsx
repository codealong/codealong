// @vitest-environment jsdom
import { createRoot } from 'react-dom/client';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { act } from '../testing/dom';
import * as fx from '../testing/fixtures';
import type { Lane, PrItem, Session } from '../types';
import { OtherOwnerChips, PeerChatterChip, PeerTalkChips, TodoOwnerTag, isPeerChatter } from './PeerTalkBits';
import { usePrLinks } from './PrLink';

let mounted: { root: ReturnType<typeof createRoot>; el: HTMLElement }[] = [];

function mount(node: React.ReactNode) {
  const el = document.body.appendChild(document.createElement('div'));
  const root = createRoot(el);
  act(() => root.render(node));
  mounted.push({ root, el });
  return el;
}

function Host({ lanes, prs, children }: { lanes: Lane[]; prs?: PrItem[]; children: React.ReactNode }) {
  usePrLinks(
    prs ?? [],
    lanes,
    {
      showSession: () => {},
      openSession: () => {},
      sessionMenu: () => {},
      prMenu: () => {},
    },
    null,
  );
  return <>{children}</>;
}

afterEach(() => {
  for (const m of mounted) {
    act(() => m.root.unmount());
    m.el.remove();
  }
  mounted = [];
});

describe('isPeerChatter', () => {
  it('flags pairs at or above 4 messages and 2 direction flips', () => {
    expect(isPeerChatter({ key: 'k', title: 'Other', count: 4, flips: 2, lastAt: fx.NOW })).toBe(true);
    expect(isPeerChatter({ key: 'k', title: 'Other', count: 6, flips: 4, lastAt: fx.NOW })).toBe(true);
    expect(isPeerChatter({ key: 'k', title: 'Other', count: 3, flips: 2, lastAt: fx.NOW })).toBe(false);
    expect(isPeerChatter({ key: 'k', title: 'Other', count: 5, flips: 1, lastAt: fx.NOW })).toBe(false);
  });
});

describe('PeerChatterChip', () => {
  it('shows a compact warning chip only when peer chatter crosses the ping-pong threshold', () => {
    const quiet = fx.session({ peers: [{ key: 'claude:claude:s2', title: 'Other', count: 2, flips: 1, lastAt: fx.NOW }] });
    expect(mount(<PeerChatterChip session={quiet} />).textContent).toBe('');

    const hot = fx.session({ peers: [{ key: 'claude:claude:s2', title: 'Ranking', count: 6, flips: 4, lastAt: fx.NOW }] });
    const el = mount(<PeerChatterChip session={hot} />);
    const chip = el.querySelector('.ca-insight.is-warn');
    expect(chip?.textContent).toBe('↔ 6');
    expect(chip?.getAttribute('title')).toContain('Ranking');
  });
});

describe('PeerTalkChips', () => {
  it('renders clickable peer chips and marks ping-pong pairs with is-warn', () => {
    const other = fx.session({ key: 'codex:codex:peer', title: 'Ranking scorer' });
    const current = fx.session({
      key: 'claude:claude:cur',
      peers: [{ key: other.key, title: other.title, count: 6, flips: 4, lastAt: fx.NOW - 60_000 }],
    });
    const lane = fx.lane({ sessions: [current, other] });
    const onShowSession = vi.fn();

    const el = mount(
      <Host lanes={[lane]}>
        <PeerTalkChips session={current} now={fx.NOW} onShowSession={onShowSession} />
      </Host>,
    );
    const btn = el.querySelector<HTMLButtonElement>('button.ca-chip.ca-peertalkchip.is-warn');
    expect(btn).not.toBeNull();
    expect(btn?.textContent).toContain('6 msgs');
    expect(btn?.querySelector('.ca-chipname')?.textContent).toBe('Ranking scorer');
    expect(btn?.title).toMatch(/Click to show it\.$/);

    act(() => btn!.click());
    expect(onShowSession).toHaveBeenCalledWith(lane, other);
  });
});

describe('TodoOwnerTag', () => {
  it('shows Owner only when the todo has multiple sessions and this session is primary', () => {
    const solo = fx.todo({
      sessions: ['claude:claude:s1'],
      primary: { session: 'claude:claude:s1', since: fx.NOW },
    });
    expect(mount(<TodoOwnerTag todo={solo} sessionKey="claude:claude:s1" />).textContent).toBe('');

    const shared = fx.todo({
      sessions: ['claude:claude:s1', 'codex:codex:s2'],
      primary: { session: 'claude:claude:s1', since: fx.NOW },
    });
    expect(mount(<TodoOwnerTag todo={shared} sessionKey="claude:claude:s1" />).textContent).toBe('Owner');
    expect(mount(<TodoOwnerTag todo={shared} sessionKey="codex:codex:s2" />).textContent).toBe('');
  });
});

describe('OtherOwnerChips', () => {
  it('shows one clickable chip an owner, naming all it owns, for a shared todo and open PRs owned by another session, skipping merged PRs', () => {
    const owner: Session = fx.session({ key: 'claude:claude:owner', title: 'Primary fix' });
    const other: Session = fx.session({ key: 'claude:claude2:other', title: 'Docs pass' });
    const current: Session = fx.session({ key: 'codex:codex:side', title: 'Side check' });
    const lane = fx.lane({ sessions: [owner, other, current] });
    const openPr = fx.pr({ number: 12, state: 'open' });
    const otherPr = fx.pr({ number: 14, state: 'open' });
    const mergedPr = fx.pr({ number: 13, state: 'merged' });
    const openItem = fx.prItem({ number: 12, state: 'open', primary: owner.key });
    const otherItem = fx.prItem({ number: 14, state: 'open', primary: other.key });
    const mergedItem = fx.prItem({ number: 13, state: 'merged', primary: owner.key });
    const sharedTodo = fx.todo({
      id: 't-shared',
      sessions: [owner.key, current.key],
      primary: { session: owner.key, since: fx.NOW },
    });
    const onShowSession = vi.fn();

    const el = mount(
      <Host lanes={[lane]} prs={[openItem, otherItem, mergedItem]}>
        <OtherOwnerChips session={current} prs={[openPr, mergedPr, otherPr]} todo={sharedTodo} onShowSession={onShowSession} />
      </Host>,
    );
    const chips = [...el.querySelectorAll<HTMLButtonElement>('button.ca-chip.ca-ownerchip')];
    // What it owns is a span of its own, whole, and the owner's title the one cut short.
    expect(chips.map((b) => [...b.children].map((c) => c.textContent))).toEqual([
      ['Todo and #12 owner:', 'Primary fix'],
      ['#14 owner:', 'Docs pass'],
    ]);
    expect(chips.map((b) => b.querySelector('.ca-chipname')?.textContent)).toEqual(['Primary fix', 'Docs pass']);
    expect(chips[0].title).toBe('Todo and #12 are owned by “Primary fix”. Click to open the owning session.');

    act(() => chips[0].click());
    expect(onShowSession).toHaveBeenCalledWith(lane, owner);
  });
});
