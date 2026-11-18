// The reply a session is writing as it streams in (SessionThread.tsx `Writing`). It's held
// outside React's state, so that only what shows it renders as it grows, and handed on at most
// once a frame: a hosted session's agent sends the whole reply so far with each chunk (acp.rs),
// many a frame, and each one drawn re-laid the conversation's end and its Markdown whole.

import type { HostedStream } from './types';

/** Runs `run` once, later; the function it gives back calls it off. */
export type Schedule = (run: () => void) => () => void;

/** At the next frame, or after a quarter of a second for a page that draws none while it's
 *  hidden, so a reply that came while it was isn't held back for good. */
export const nextFrame: Schedule = (run) => {
  let frame = 0;
  let timer = 0;
  const off = () => {
    cancelAnimationFrame(frame);
    window.clearTimeout(timer);
  };
  const once = () => {
    off();
    run();
  };
  frame = requestAnimationFrame(once);
  timer = window.setTimeout(once, 250);
  return off;
};

export class LiveReply {
  private value: HostedStream | null = null;
  private waiting: { next: HostedStream | null; follow: boolean } | null = null;
  private cancel: (() => void) | null = null;
  private readonly heard = new Set<() => void>();
  /** Called just before a reply that came as it was written is shown, to say whether the view
   *  goes along with it: decided then, so a scroll made while it waited counts. */
  onFollow: (() => void) | null = null;

  constructor(private readonly schedule: Schedule = nextFrame) {}

  subscribe = (heard: () => void) => {
    this.heard.add(heard);
    return () => {
      this.heard.delete(heard);
    };
  };

  get = () => this.value;

  /** Shown at the next frame, in place of any still waiting for it, which never shows. `follow`
   *  for one sent as it was written, rather than asked for: it can bring the view along. */
  put(next: HostedStream | null, follow: boolean) {
    this.waiting = { next, follow: follow || !!this.waiting?.follow };
    this.cancel ??= this.schedule(() => this.flush());
  }

  /** Nothing, at once, and nothing that was waiting: as another session's conversation opens. */
  clear() {
    this.cancel?.();
    this.cancel = null;
    this.waiting = null;
    this.show(null);
  }

  private flush() {
    const w = this.waiting;
    this.cancel = null;
    this.waiting = null;
    if (!w) return;
    if (w.follow) this.onFollow?.();
    this.show(w.next);
  }

  private show(next: HostedStream | null) {
    if (next === this.value) return;
    this.value = next;
    for (const heard of [...this.heard]) heard();
  }
}
