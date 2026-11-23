import { describe, expect, it } from 'vitest';
import { LiveReply, type Schedule } from './liveReply';

/** A frame that comes when the test says. */
function frames() {
  let due: (() => void)[] = [];
  const schedule: Schedule = (run) => {
    due.push(run);
    return () => {
      due = due.filter((r) => r !== run);
    };
  };
  const frame = () => {
    const now = due;
    due = [];
    for (const run of now) run();
  };
  return { schedule, frame, waiting: () => due.length };
}

const st = (text: string) => ({ key: 'k', text, tool: null });

describe('LiveReply', () => {
  it('shows only the latest of the chunks that came within a frame, once', () => {
    const f = frames();
    const reply = new LiveReply(f.schedule);
    const shown: (string | undefined)[] = [];
    reply.subscribe(() => shown.push(reply.get()?.text));
    reply.put(st('a'), true);
    reply.put(st('ab'), true);
    reply.put(st('abc'), true);
    expect(f.waiting()).toBe(1);
    expect(reply.get()).toBeNull();
    f.frame();
    expect(shown).toEqual(['abc']);
    // The last one is never dropped: it shows at the next frame.
    reply.put(st('abcd'), true);
    f.frame();
    expect(shown).toEqual(['abc', 'abcd']);
  });

  it('asks whether to follow as it shows, when any chunk that frame came as it was written', () => {
    const f = frames();
    const reply = new LiveReply(f.schedule);
    let asked = 0;
    reply.onFollow = () => asked++;
    reply.put(st('a'), true);
    reply.put(st('ab'), false);
    expect(asked).toBe(0);
    f.frame();
    expect(asked).toBe(1);
    // One asked for only doesn't.
    reply.put(st('abc'), false);
    f.frame();
    expect(asked).toBe(1);
    // A tool starting or stopping without reply text changing doesn't change the conversation's
    // length, so it doesn't ask to follow either.
    reply.put({ key: 'k', text: 'abc', tool: 'Terminal' }, true);
    f.frame();
    expect(reply.get()?.tool).toBe('Terminal');
    expect(asked).toBe(1);
    reply.put({ key: 'k', text: 'abc', tool: null }, true);
    f.frame();
    expect(reply.get()?.tool).toBeNull();
    expect(asked).toBe(1);
  });

  it('clears at once, dropping what was waiting', () => {
    const f = frames();
    const reply = new LiveReply(f.schedule);
    reply.put(st('a'), true);
    f.frame();
    reply.put(st('ab'), true);
    reply.clear();
    expect(reply.get()).toBeNull();
    expect(f.waiting()).toBe(0);
    f.frame();
    expect(reply.get()).toBeNull();
  });
});
