// Lines between cards pinned to the desktop whose sessions came one of the other, while the
// pointer is on one of them (lineage.rs). The window is a clear panel over the screen, just under
// the cards, that lets every click through; the engine says where the cards are in it. Each line
// runs from the card of the session the other came from to the card of the one that came of it,
// with an arrowhead there (lineage.ts has the shape, which the Canvas view draws too).

import { useEffect, useState } from 'react';
import { getLineageFeed, onLineageFeed } from '../api';
import { arrowhead, connector, pathOf } from '../lineage';
import type { LineageFeed } from '../types';
import { useTheme } from './ThemeSwitch';

export function DesktopLines() {
  const [feed, setFeed] = useState<LineageFeed | null>(null);
  useEffect(() => {
    const stop = onLineageFeed(setFeed);
    getLineageFeed()
      .then((first) => setFeed((current) => current ?? first))
      // The engine pushes the feed as it changes, so one that didn't read comes with the next.
      .catch(() => {});
    return stop;
  }, []);
  useTheme(feed?.theme ?? 'system');

  return (
    <svg className="ca-lineage" width="100%" height="100%" aria-hidden="true">
      {(feed?.lines ?? []).map((l, i) => {
        const curve = connector(l.from, l.to, l.side);
        const [b1, b2] = arrowhead(curve, 9, 0.45);
        const d = pathOf(curve);
        const head = `M ${b1.x.toFixed(1)} ${b1.y.toFixed(1)} L ${curve.b.x.toFixed(1)} ${curve.b.y.toFixed(1)} L ${b2.x.toFixed(1)} ${b2.y.toFixed(1)}`;
        return (
          <g key={i}>
            {/* A soft edge under the line, so it reads over any wallpaper. */}
            <path className="ca-lineage-halo" d={d} />
            <path className="ca-lineage-halo" d={head} />
            <path className="ca-lineage-line" d={d} />
            <path className="ca-lineage-line" d={head} />
            <circle className="ca-lineage-dot" cx={curve.a.x} cy={curve.a.y} r={3.5} />
          </g>
        );
      })}
    </svg>
  );
}
