// The edges you drag to size a window that floats and has no frame of its own: the peek, a
// session kept on the desktop (Peek.tsx) and a terminal popped out (TerminalWindow.tsx). Each is
// a strip the page lays in the room the window keeps around its card, by the class it's given:
// the right edge (`is-side`), the far edge (`is-far`: the bottom, or the top for a peek over the
// bar) and the corner between them (`is-corner`). The window keeps its top left corner as it's
// sized (kept.rs `resize`, peek.rs).

import { useRef, type PointerEvent as ReactPointerEvent, type ReactElement } from 'react';

/** The edges that size a window: its right edge, its far edge, and the corner between them. */
export type Edge = 'side' | 'far' | 'corner';

/** How far an edge moves with the button down before it sizes the window rather than clicks. */
const SIZE_AFTER = 3;

export interface Sizing {
  /** The far edge is the top (a peek over the bar): dragging it up makes the window taller. */
  up?: boolean;
  /** The button went down on an edge. */
  onStart?: () => void;
  /** The edge has moved far enough to size the window. */
  onMoved?: () => void;
  /** The size the edge is dragged to, once a display frame. */
  size: (width: number, height: number) => void;
  /** Let go at this size, once the edge moved. */
  done: (width: number, height: number) => void;
}

interface Dragged {
  edge: Edge;
  x: number;
  y: number;
  w: number;
  h: number;
  moved: boolean;
  frame: number;
  to: [number, number];
}

/** What draws a window's edges: `grip(edge)` is the strip for that edge, of class
 *  `<className> is-<edge>`, which the window's styles place. */
export function useSizeGrips(className: string, sizing: Sizing): (edge: Edge) => ReactElement {
  const dragged = useRef<Dragged | null>(null);
  const start = (edge: Edge) => (e: ReactPointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();
    e.currentTarget.setPointerCapture(e.pointerId);
    sizing.onStart?.();
    dragged.current = { edge, x: e.screenX, y: e.screenY, w: window.innerWidth, h: window.innerHeight, moved: false, frame: 0, to: [window.innerWidth, window.innerHeight] };
  };
  const move = (e: ReactPointerEvent<HTMLDivElement>) => {
    const d = dragged.current;
    if (!d) return;
    const [dx, dy] = [e.screenX - d.x, e.screenY - d.y];
    if (!d.moved && Math.abs(dx) + Math.abs(dy) < SIZE_AFTER) return;
    if (!d.moved) sizing.onMoved?.();
    d.moved = true;
    d.to = [d.edge === 'far' ? d.w : d.w + dx, d.edge === 'side' ? d.h : d.h + (sizing.up ? -dy : dy)];
    if (d.frame) return;
    d.frame = requestAnimationFrame(() => {
      d.frame = 0;
      sizing.size(d.to[0], d.to[1]);
    });
  };
  const end = () => {
    const d = dragged.current;
    dragged.current = null;
    if (!d?.moved) return;
    cancelAnimationFrame(d.frame);
    sizing.done(d.to[0], d.to[1]);
  };
  return (edge) => <div key={edge} className={`${className} is-${edge}`} onPointerDown={start(edge)} onPointerMove={move} onPointerUp={end} onPointerCancel={end} />;
}
