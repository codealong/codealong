// @vitest-environment jsdom
import { describe, expect, it } from 'vitest';
import { createRoot } from 'react-dom/client';
import { LiveReply, type Schedule } from '../liveReply';
import { act } from '../testing/dom';
import * as fx from '../testing/fixtures';
import { Working } from './Working';

const sync: Schedule = (run) => {
  run();
  return () => {};
};

describe('Working’s running tool', () => {
  it('shows a hosted session’s running tool inline on the working line without calling onGrow', () => {
    const el = document.body.appendChild(document.createElement('div'));
    const root = createRoot(el);
    const reply = new LiveReply(sync);
    const session = fx.session({ status: 'working', statusSince: fx.NOW - 60_000, lastPromptAt: fx.NOW - 60_000 });
    let grew = 0;
    const onGrow = () => {
      grew += 1;
    };
    const render = (pending: boolean) =>
      act(() => root.render(<Working session={session} tips={false} reply={reply} pending={pending} onGrow={onGrow} />));

    render(false);
    const before = grew;
    expect(el.querySelector('.ca-working-tool')).toBeNull();

    // A tool starts: shown inside .ca-working, not as a separate row, and onGrow isn't called.
    act(() => reply.put({ key: session.key, text: '', tool: 'Terminal' }, true));
    expect(el.querySelector('.ca-working > .ca-working-tool')?.textContent).toBe('Running Terminal');
    expect(el.querySelector('.ca-thread-note')).toBeNull();
    expect(grew).toBe(before);

    // Once the transcript has that call (pending), it clears.
    render(true);
    expect(el.querySelector('.ca-working-tool')).toBeNull();

    // Once the tool is no longer pending, another tool shows again.
    render(false);
    expect(el.querySelector('.ca-working-tool')?.textContent).toBe('Running Terminal');

    // Reply text streaming in takes over; the tool label clears.
    act(() => reply.put({ key: session.key, text: 'Done.', tool: 'Terminal' }, true));
    expect(el.querySelector('.ca-working-tool')).toBeNull();

    // Clearing the tool keeps it clear without calling onGrow.
    act(() => reply.put({ key: session.key, text: '', tool: null }, true));
    expect(el.querySelector('.ca-working-tool')).toBeNull();
    expect(grew).toBe(before);

    act(() => root.unmount());
    el.remove();
  });
});
