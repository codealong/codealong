// @vitest-environment jsdom
import { describe, expect, it, vi } from 'vitest';
import { act, engine, mountHook, settle } from '../testing/dom';
import { createElement } from 'react';
import { createRoot } from 'react-dom/client';
import { Toasts, useToasts, type Archived } from './Toasts';

globalThis.ResizeObserver ??= class {
  observe() {}
  unobserve() {}
  disconnect() {}
};

describe('useToasts agent-done events', () => {
  it('keeps a single key when agent-done sends one session', async () => {
    const mounted = mountHook(() => useToasts());
    await settle();
    act(() => {
      engine.emit('agent-done', {
        key: 'claude:a:child',
        title: 'Helper run',
        message: '“Main” marked “Helper run” done: it’s archived.',
      });
    });
    expect(mounted.current.archived).toEqual({
      keys: ['claude:a:child'],
      title: 'Helper run',
      message: '“Main” marked “Helper run” done: it’s archived.',
    });
  });

  it('passes every archived key to Undo when an agent archives a batch', async () => {
    const onUnarchive = vi.fn();
    let captured: Archived | null = null;
    function Host() {
      const state = useToasts();
      captured = state.archived;
      return createElement(Toasts, { state, onUnarchive });
    }
    const container = document.body.appendChild(document.createElement('div'));
    const root = createRoot(container);
    act(() => root.render(createElement(Host)));
    await settle();

    act(() => {
      engine.emit('agent-done', {
        key: 'claude:a:one',
        keys: ['claude:a:one', 'claude:a:two', 'claude:a:three'],
        title: 'Old cleanup 1',
        message: '“Cleanup review” archived 3 sessions.',
      });
    });

    expect(captured).toEqual({
      keys: ['claude:a:one', 'claude:a:two', 'claude:a:three'],
      title: 'Old cleanup 1',
      message: '“Cleanup review” archived 3 sessions.',
    });
    expect(container.textContent).toContain('“Cleanup review” archived 3 sessions.');

    const undo = container.querySelector('button.ca-toast-action') as HTMLButtonElement | null;
    expect(undo?.textContent).toBe('Undo');
    act(() => undo!.click());
    expect(onUnarchive).toHaveBeenCalledWith({
      keys: ['claude:a:one', 'claude:a:two', 'claude:a:three'],
      title: 'Old cleanup 1',
      message: '“Cleanup review” archived 3 sessions.',
    });
    expect(captured).toBeNull();
    act(() => root.unmount());
    container.remove();
  });
});
