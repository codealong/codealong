import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import { CLICK_WAIT_MS } from '../bar';
import * as fx from '../testing/fixtures';
import type { GitInfo } from '../types';
import { Chip, chipHandlers, driftWords, HarnessChip, MoreChip, PrChip, Row, rowMenu, Seg, selects } from './primitives';

describe('driftWords', () => {
  const git = (g: Partial<GitInfo>): GitInfo => fx.git({ base: 'origin/main', baseAhead: 0, baseBehind: 0, ...g });

  it('says how far a worktree has moved from the default branch', () => {
    expect(driftWords(git({ baseAhead: 3, baseBehind: 42 }))).toBe('3 commits ahead of origin/main, 42 behind');
    expect(driftWords(git({ baseAhead: 1 }))).toBe('1 commit ahead of origin/main');
    expect(driftWords(git({ baseBehind: 1200 }))).toBe('1,200 commits behind origin/main');
  });

  it('says nothing when it is even with it or git has not said', () => {
    expect(driftWords(git({}))).toBeNull();
    expect(driftWords(git({ base: undefined, baseAhead: undefined, baseBehind: undefined }))).toBeNull();
    expect(driftWords(null)).toBeNull();
  });
});

describe('Seg', () => {
  const options = [
    { id: 'a', label: 'First', title: 'The first one' },
    { id: 'b', label: 'Second', icon: 'sun' },
  ];
  const html = (props: object) => renderToStaticMarkup(createElement(Seg<string>, { options, value: 'b', onPick: () => {}, label: 'Order', ...props }));

  it('is a group of radios that says which is on', () => {
    const out = html({ className: 'ca-sessionbar-seg' });
    expect(out).toMatch(/^<div class="ca-segctl ca-sessionbar-seg" role="radiogroup" aria-label="Order">/);
    expect(out).toContain('<button type="button" role="radio" aria-checked="false" title="The first one">First</button>');
    expect(out).toMatch(/<button type="button" role="radio" aria-checked="true" class="is-on"><svg[^>]*>.*<\/svg>Second<\/button>/);
  });

  it('draws words alone, or icons alone named by their words', () => {
    expect(html({ look: 'text' })).not.toContain('<svg');
    const icons = html({ look: 'icon' });
    expect(icons).toContain('aria-label="Second"');
    expect(icons).toContain('title="Second"');
    expect(icons).not.toContain('>Second<');
  });

  it('is pressed buttons where none need be on', () => {
    const out = html({ toggles: true, value: null });
    expect(out).toContain('role="group"');
    expect(out).not.toContain('role="radio"');
    expect(out.match(/aria-pressed="false"/g)).toHaveLength(2);
  });

  it('turns every choice off when it is disabled, or one by itself', () => {
    expect(html({ disabled: true }).match(/disabled=""/g)).toHaveLength(2);
    expect(html({ options: [options[0], { ...options[1], disabled: true }] }).match(/disabled=""/g)).toHaveLength(1);
  });
});

describe('chipHandlers', () => {
  const event = (detail = 1) => ({ detail, stopPropagation: vi.fn() });

  it('leaves a chip that does nothing to what it is on', () => {
    expect(chipHandlers(undefined, false)).toEqual({});
    expect(chipHandlers(undefined, true)).toEqual({});
  });

  it('acts once on a click, however many came in the run', () => {
    const act = vi.fn();
    const h = chipHandlers(act, false);
    // A double-click: two clicks, then the double-click itself.
    h.onClick?.(event(1));
    h.onClick?.(event(2));
    h.onDoubleClick?.(event(2));
    expect(act).toHaveBeenCalledTimes(1);
    // A third in the same run is no new click either.
    h.onClick?.(event(3));
    expect(act).toHaveBeenCalledTimes(1);
    // Return or Space on the chip is a click that counts none.
    h.onClick?.(event(0));
    expect(act).toHaveBeenCalledTimes(2);
  });

  it('keeps a click on a button chip from what the chip is on', () => {
    const h = chipHandlers(() => {}, false);
    for (const on of [h.onMouseDown, h.onClick, h.onDoubleClick]) {
      const e = event();
      on?.(e);
      expect(e.stopPropagation).toHaveBeenCalledTimes(1);
    }
  });

  it("on a list's row leaves the click to the row and acts on a double-click, in the row's place", () => {
    const act = vi.fn();
    const h = chipHandlers(act, true);
    // The press and the click reach the row, which selects.
    expect(h.onMouseDown).toBeUndefined();
    expect(h.onClick).toBeUndefined();
    const e = event(2);
    h.onDoubleClick?.(e);
    expect(act).toHaveBeenCalledTimes(1);
    expect(e.stopPropagation).toHaveBeenCalledTimes(1);
  });

  describe('with something else for a double-click', () => {
    const setup = (inRow: boolean) => {
      const act = vi.fn();
      const run = vi.fn();
      return { act, run, h: chipHandlers(act, inRow, { run, wait: { current: null } }) };
    };

    it('acts on a click once no second has come', () => {
      vi.useFakeTimers();
      try {
        const { act, run, h } = setup(false);
        h.onClick?.(event(1));
        vi.advanceTimersByTime(CLICK_WAIT_MS - 1);
        expect(act).not.toHaveBeenCalled();
        vi.advanceTimersByTime(1);
        expect(act).toHaveBeenCalledTimes(1);
        expect(run).not.toHaveBeenCalled();
      } finally {
        vi.useRealTimers();
      }
    });

    it("does the double-click's thing in place of the click's, once however long the run", () => {
      vi.useFakeTimers();
      try {
        const { act, run, h } = setup(false);
        h.onClick?.(event(1));
        h.onClick?.(event(2));
        h.onDoubleClick?.(event(2));
        h.onClick?.(event(3));
        vi.advanceTimersByTime(CLICK_WAIT_MS * 2);
        expect(run).toHaveBeenCalledTimes(1);
        expect(act).not.toHaveBeenCalled();
      } finally {
        vi.useRealTimers();
      }
    });

    it('acts at once on a key, which no second click follows', () => {
      const { act, h } = setup(false);
      h.onClick?.(event(0));
      expect(act).toHaveBeenCalledTimes(1);
    });

    it("takes the click on a list's row too, rather than leave it to the row", () => {
      const { h } = setup(true);
      for (const on of [h.onMouseDown, h.onClick, h.onDoubleClick]) {
        const e = event();
        on?.(e);
        expect(e.stopPropagation).toHaveBeenCalledTimes(1);
      }
    });
  });
});

describe('Chip', () => {
  const html = (props: object, children?: string) => renderToStaticMarkup(createElement(Chip, props, children));

  it('is a span until it acts off a row, then a button', () => {
    expect(html({ className: 'ca-reusechip', title: 'Free' }, 'Free')).toBe('<span class="ca-chip ca-reusechip" title="Free">Free</span>');
    expect(html({ act: () => {}, inRow: true }, 'x')).toBe('<span class="ca-chip">x</span>');
    expect(html({ act: () => {} }, 'x')).toBe('<button type="button" class="ca-chip ca-chip--button">x</button>');
  });

  it('takes the classes of the chip it is', () => {
    expect(html({ kind: 'label', act: () => {} }, 'x')).toContain('class="ca-labelchip ca-labelchip--button"');
    expect(html({ kind: 'pr', className: 'ca-pr--open', act: () => {} }, 'x')).toContain('class="ca-pr ca-pr--button ca-pr--open"');
  });

  it('says in its tooltip how it acts', () => {
    expect(html({ title: 'A todo', does: 'show it in Todo' }, 'x')).toContain('title="A todo"');
    expect(html({ title: 'A todo', does: 'show it in Todo', act: () => {} }, 'x')).toContain('title="A todo. Click to show it in Todo."');
    expect(html({ title: 'A todo', does: 'show it in Todo', act: () => {}, inRow: true }, 'x')).toContain('title="A todo. Double-click to show it in Todo."');
  });

  it('ends with a button that takes it away', () => {
    expect(html({ kind: 'label', remove: { label: 'Remove Bug', title: 'Remove Bug', run: () => {} } }, 'Bug')).toMatch(/^<span class="ca-labelchip">Bug<button type="button" aria-label="Remove Bug" title="Remove Bug"><svg/);
  });
});

describe('the named chips', () => {
  it('say how a session and a PR open, by where they are', () => {
    const s = fx.session({ account: 'claude2', title: 'Fix the build', status: 'idle' });
    const chip = (props: object) => renderToStaticMarkup(createElement(HarnessChip, { session: s, ...props }));
    expect(chip({})).toContain('title="claude2 · Fix the build · Idle"');
    expect(chip({ onOpen: () => {}, inRow: true })).toMatch(/^<span class="ca-chip" title="claude2 · Fix the build · Idle\. Double-click to open in [^"]+\.">/);
    expect(chip({ onOpen: () => {}, className: 'is-extra' })).toMatch(/^<button type="button" class="ca-chip ca-chip--button is-extra" title="claude2 · Fix the build · Idle\. Click to open in [^"]+\.">/);
    // Named, where the chip stands for the session: its title, with its account on hover after it.
    const named = chip({ named: true, withStatus: false, detail: 'Primary' });
    expect(named).toMatch(/^<span class="ca-chip ca-sesschip" title="Fix the build\nclaude2 \(Claude Code\) · Idle\nPrimary">/);
    expect(named).toContain('<span class="ca-sesschip-title">Fix the build</span>');
    expect(named).not.toContain('>claude2<');
    const pr = fx.pr({ number: 12, title: 'Add it', state: 'open' });
    const prChip = (props: object) => renderToStaticMarkup(createElement(PrChip, { pr, ...props }));
    expect(prChip({ onOpen: () => {} })).toMatch(/^<button type="button" class="ca-pr ca-pr--button ca-pr--open" title="#12 Open: Add it\. Click to open it on GitHub\.">/);
    expect(prChip({ onOpen: () => {}, inRow: true, titled: false })).toMatch(/^<span class="ca-pr ca-pr--open">/);
    // Where Codealong can show it, a click does, on a list's row too, and two open it on GitHub.
    const shows = /^<button type="button" class="ca-pr ca-pr--button ca-pr--open" title="#12 Open: Add it\. Click to show it in Codealong, double-click to open it on GitHub\.">/;
    expect(prChip({ onShow: () => {}, onOpen: () => {} })).toMatch(shows);
    expect(prChip({ onShow: () => {}, onOpen: () => {}, inRow: true })).toMatch(shows);
  });

  it('marks a PR that conflicts with its base, while it is open', () => {
    const chip = (state: 'open' | 'merged', merge: 'conflicts' | 'clean') => renderToStaticMarkup(createElement(PrChip, { pr: fx.pr({ number: 12, state, merge }), compact: true }));
    expect(chip('open', 'conflicts')).toContain('<span class="ca-checks is-attn" title="Conflicts with its base: resolve them before it can merge">');
    expect(chip('open', 'clean')).not.toContain('is-attn');
    expect(chip('merged', 'conflicts')).not.toContain('is-attn');
  });
});

describe('MoreChip', () => {
  it('counts what a row left out and names it on hover', () => {
    expect(renderToStaticMarkup(createElement(MoreChip, { names: ['a', 'b'], className: 'is-wide' }))).toBe('<span class="ca-chip ca-chip--more is-wide" title="a\nb">+2</span>');
    expect(renderToStaticMarkup(createElement(MoreChip, { names: [] }))).toBe('');
  });
});

describe('Row', () => {
  const html = (props: object) => renderToStaticMarkup(createElement(Row, props, 'x'));

  it('is an option that says whether it is selected', () => {
    expect(html({ id: 'row-a', className: 'ca-lane is-agent' })).toBe('<div id="row-a" role="option" aria-selected="false" class="ca-lane is-agent">x</div>');
    expect(html({ className: 'ca-lane', selected: true })).toBe('<div role="option" aria-selected="true" class="ca-lane is-selected">x</div>');
  });

  it('is a named group where it holds rows of its own', () => {
    expect(html({ role: 'group', label: 'main, 2 agents', className: 'ca-lane is-group' })).toBe('<div role="group" aria-label="main, 2 agents" class="ca-lane is-group">x</div>');
  });

  it('carries its place and a grip when it can be dragged', () => {
    const out = html({ className: 'ca-lane', drag: { pinIndex: 1, onPointerDown: () => {}, extraClass: 'is-drop-before' } });
    expect(out).toMatch(/^<div role="option" aria-selected="false" class="ca-lane is-drop-before" data-pin-index="1"><span class="ca-grip" aria-hidden="true"><svg/);
    expect(html({ className: 'ca-lane', drag: { onPointerDown: () => {} } })).not.toContain('data-pin-index');
  });
});

describe('a row and the mouse', () => {
  it('is selected by the main button, not by a right-click or a ⌃-click', () => {
    expect(selects({ button: 0, ctrlKey: false })).toBe(true);
    expect(selects({ button: 2, ctrlKey: false })).toBe(false);
    expect(selects({ button: 0, ctrlKey: true })).toBe(false);
  });

  it('shows its menu on a right-click, in place of any other', () => {
    const show = vi.fn();
    const e = { preventDefault: vi.fn(), stopPropagation: vi.fn() };
    rowMenu(show)?.(e);
    expect(show).toHaveBeenCalledTimes(1);
    expect(e.preventDefault).toHaveBeenCalled();
    // A session's line on its worktree's row: the worktree's menu doesn't come up too.
    expect(e.stopPropagation).toHaveBeenCalled();
    expect(rowMenu(undefined)).toBeUndefined();
  });
});
