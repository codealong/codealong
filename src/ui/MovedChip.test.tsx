// @vitest-environment jsdom

import { createElement } from 'react';
import { createRoot } from 'react-dom/client';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { act } from '../testing/dom';
import * as fx from '../testing/fixtures';
import { AgentRow } from './LaneRow';

let cleanup: (() => void) | null = null;
afterEach(() => {
  cleanup?.();
  cleanup = null;
});

function renderRow(props: Parameters<typeof AgentRow>[0]): HTMLDivElement {
  const el = document.body.appendChild(document.createElement('div'));
  const root = createRoot(el);
  act(() => root.render(createElement(AgentRow, props)));
  cleanup = () => {
    act(() => root.unmount());
    el.remove();
  };
  return el;
}

describe('AgentRow MovedChip', () => {
  const lane = fx.lane({ id: '/r/app', callsign: 'app', path: '/r/app' });
  const oldSession = fx.session({ key: 'claude:claude-work:old', account: 'claude-work', archived: true, title: 'Northwind mocks' });
  const newSession = fx.session({ key: 'claude:claude2:new', account: 'claude2', title: 'Northwind mocks', origin: { key: oldSession.key, how: 'fork' } });
  const movedTo = { lane, session: newSession };

  it('shows Moved to chip in both one-line and two-line rows and jumps on double-click', () => {
    for (const slim of [false, true]) {
      const onShowMoved = vi.fn();
      const onOpen = vi.fn();
      const el = renderRow({
        lane,
        session: oldSession,
        domId: 'row-old',
        now: fx.NOW,
        slim,
        selected: false,
        onSelect: () => {},
        onOpen,
        movedTo,
        onShowMoved,
      });
      const chip = el.querySelector('.ca-movedchip') as HTMLElement | null;
      expect(chip).not.toBeNull();
      expect(chip!.textContent).toContain('Moved to claude2');
      expect(chip!.getAttribute('title')).toBe('Moved to “Northwind mocks” on claude2. Double-click to show the new session.');
      act(() => {
        chip!.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }));
      });
      expect(onShowMoved).toHaveBeenCalledTimes(1);
      expect(onShowMoved).toHaveBeenCalledWith(movedTo);
      expect(onOpen).not.toHaveBeenCalled();
      cleanup?.();
      cleanup = null;
    }
  });

  it('renders nothing when the session has not moved', () => {
    const el = renderRow({
      lane,
      session: oldSession,
      domId: 'row-old',
      now: fx.NOW,
      selected: false,
      onSelect: () => {},
      onOpen: () => {},
    });
    expect(el.querySelector('.ca-movedchip')).toBeNull();
  });
});
