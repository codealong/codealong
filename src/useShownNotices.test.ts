// @vitest-environment jsdom
import { afterEach, describe, expect, it } from 'vitest';
import { act, mountHook } from './testing/dom';
import { clearNotice, drawNoticesHere, noticeId, reportNotice } from './notices';
import * as fx from './testing/fixtures';
import type { Board, Notice } from './types';
import { useShownNotices } from './useShownNotices';

// What isn't working in the background, as the main window shows it (useShownNotices.ts): the
// engine's notices and the window's own, less the ones dismissed while they still stand.

const github: Notice = { key: 'github', title: 'GitHub isn’t answering', text: 'gh: timed out', until: null };
const usage: Notice = { key: 'usage', title: 'Usage can’t be read', text: 'rate limited', until: null };
const canvas: Notice = { key: 'canvas', title: 'The canvas isn’t saving', text: 'disk full', until: null };
const DISMISSED = 'codealong.dismissedNotices';
const stored = (): string[] => JSON.parse(window.localStorage.getItem(DISMISSED) ?? '[]') as string[];

// The main window draws the notices its own code reports.
drawNoticesHere();
const mount = (board: Board | null) => mountHook((b: Board | null) => useShownNotices(b), board);
afterEach(() => {
  clearNotice('canvas');
});

describe('the notices the main window shows', () => {
  it('are the engine’s and then the window’s own', () => {
    const w = mount(fx.board({ notices: [github] }));
    expect(w.current.notices).toEqual([github]);
    act(() => reportNotice(canvas));
    expect(w.current.notices).toEqual([github, canvas]);
    act(() => clearNotice('canvas'));
    expect(w.current.notices).toEqual([github]);
    w.rerender(null);
    expect(w.current.notices).toEqual([]);
  });

  it('leave out the ones dismissed, here and after a relaunch, while they still stand', () => {
    const board = fx.board({ notices: [github, usage] });
    const w = mount(board);
    act(() => w.current.setDismissed((d) => [...d, noticeId(github)]));
    expect(w.current.notices).toEqual([usage]);
    expect(stored()).toEqual([noticeId(github)]);
    // Another window on the same storage, as after a relaunch.
    w.unmount();
    expect(mount(board).current.notices).toEqual([usage]);
  });

  it('forget a dismissal once what it told of has gone, so it shows when it comes back', () => {
    const w = mount(fx.board({ notices: [github] }));
    act(() => w.current.setDismissed((d) => [...d, noticeId(github)]));
    w.rerender(fx.board({ notices: [] }));
    expect(stored()).toEqual([]);
    w.rerender(fx.board({ notices: [github] }));
    expect(w.current.notices).toEqual([github]);
  });

  it('keep a dismissal while the board hasn’t been read, when it lists none yet', () => {
    window.localStorage.setItem(DISMISSED, JSON.stringify([noticeId(github)]));
    const w = mount(null);
    w.rerender(fx.board({ loading: true, notices: [] }));
    expect(stored()).toEqual([noticeId(github)]);
    w.rerender(fx.board({ notices: [github] }));
    expect([w.current.notices, stored()]).toEqual([[], [noticeId(github)]]);
  });
});
