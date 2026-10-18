// The mark in parts, for the bar, where it moves (mark.ts says when; MarkLive.css has the motion).
// Each part that moves is an element of its own: WebKit hands an element's transform and opacity
// to the compositor, but redraws a path that moves inside one SVG on every frame, and the node
// breathes for as long as agents work.

import { useEffect, useRef, type CSSProperties } from 'react';
import { LOGO_LABEL } from '../logo';
import { MOMENT_MS, MOMENTS, type Moment } from '../mark';
import type { LogoStatus } from '../types';
import { MARK_PATHS, MARK_VIEW_BOX } from './brand';

/** The C as a mask, so the color that runs round it shows only on it. */
const C_MASK = `url("data:image/svg+xml,${encodeURIComponent(`<svg xmlns="http://www.w3.org/2000/svg" viewBox="${MARK_VIEW_BOX}"><path d="${MARK_PATHS[0]}"/></svg>`)}")`;
/** A moment older than this was played by an earlier drawing of the mark. */
const NEW_MS = 500;

/** `quiet` is the mark beside a line of text: its C gray, and still while agents work, turning
 *  only as you point at the line (MarkLive.css). */
export function MarkLive({ size = 16, status = 'idle', moment, quiet = false }: { size?: number; status?: LogoStatus; moment: Moment | null; quiet?: boolean }) {
  const el = useRef<HTMLSpanElement>(null);
  // A moment is a class on the mark for as long as it plays; React never writes the class, so
  // drawing the bar again leaves it. Taking it off and reading the layout starts one over.
  useEffect(() => {
    const mark = el.current;
    if (!mark || !moment || Date.now() - moment.at > NEW_MS) return;
    const name = `is-${moment.kind}`;
    mark.classList.remove(...MOMENTS.map((k) => `is-${k}`));
    void mark.offsetWidth;
    mark.classList.add(name);
    const id = window.setTimeout(() => mark.classList.remove(name), MOMENT_MS[moment.kind] + 60);
    return () => window.clearTimeout(id);
  }, [moment]);
  return (
    <span ref={el} className={quiet ? 'ca-lmark is-quiet' : 'ca-lmark'} data-status={status} role="img" aria-label={`Codealong · ${LOGO_LABEL[status]}`} style={{ width: size, height: size, '--s': `${size}px` } as CSSProperties}>
      <span className="ca-lmark-c">
        <svg viewBox={MARK_VIEW_BOX} aria-hidden="true" focusable="false">
          <path d={MARK_PATHS[0]} />
        </svg>
      </span>
      <span className="ca-lmark-run" aria-hidden="true" style={{ maskImage: C_MASK, WebkitMaskImage: C_MASK }}>
        <span className="ca-lmark-arc is-first">
          <i />
        </span>
        <span className="ca-lmark-arc is-second">
          <i />
        </span>
      </span>
      <span className="ca-lmark-ring" aria-hidden="true" />
      <span className="ca-lmark-pop">
        <svg className="ca-lmark-node" viewBox={MARK_VIEW_BOX} aria-hidden="true" focusable="false">
          <path d={MARK_PATHS[1]} />
        </svg>
      </span>
      <span className="ca-lmark-dot" aria-hidden="true" />
    </span>
  );
}
