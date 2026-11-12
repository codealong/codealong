// What the Analytics window's charts share (UsageDialog.tsx's and UsageAnalytics.tsx's): their
// width in their own units, so every chart's days line up under one another, the axes, the day
// under the pointer and the tip that says it.

import { dayLabel, moneyTick } from '../spend';
import { cx } from './primitives';

/** The charts' width in their own units: they scale to the dialog, both the same, so their days
 *  line up. */
export const W = 800;
export const LEFT = 48;
export const RIGHT = 12;
export const PLOT = W - LEFT - RIGHT;
export const AXIS = 22;

export function xOf(i: number, n: number): number {
  return LEFT + ((i + 0.5) * PLOT) / Math.max(n, 1);
}

/** About six of the days, the last always among them, for the x axis. */
export function labelled(n: number): Set<number> {
  const step = Math.max(1, Math.ceil(n / 6));
  const out = new Set<number>();
  for (let i = n - 1; i >= 0; i -= step) out.add(i);
  return out;
}

/** The hit area over a chart: the day under the pointer, nearest by its x. */
export function HitArea({ n, top, height, onHover }: { n: number; top: number; height: number; onHover: (i: number | null) => void }) {
  return (
    <rect
      x={LEFT}
      y={top}
      width={PLOT}
      height={height}
      fill="transparent"
      onPointerMove={(e) => {
        const box = e.currentTarget.getBoundingClientRect();
        const at = ((e.clientX - box.left) / box.width) * n;
        onHover(Math.min(n - 1, Math.max(0, Math.floor(at))));
      }}
      onPointerLeave={() => onHover(null)}
    />
  );
}

export function YAxis({ marks, y, top, format = moneyTick }: { marks: number[]; y: (v: number) => number; top: number; format?: (v: number) => string }) {
  return (
    <g className="ca-usage-axis">
      {marks.map((m) => (
        <g key={m}>
          <line x1={LEFT} x2={W - RIGHT} y1={y(m)} y2={y(m)} className="ca-usage-grid" />
          <text x={LEFT - 8} y={Math.max(top + 8, y(m) + 4)} textAnchor="end">
            {format(m)}
          </text>
        </g>
      ))}
    </g>
  );
}

export function XAxis({ days, y }: { days: { day: string }[]; y: number }) {
  const shown = labelled(days.length);
  return (
    <g className="ca-usage-axis">
      {days.map((d, i) =>
        shown.has(i) ? (
          <text key={d.day} x={xOf(i, days.length)} y={y} textAnchor={i === days.length - 1 ? 'end' : 'middle'} dx={i === days.length - 1 ? 6 : 0}>
            {dayLabel(d.day)}
          </text>
        ) : null,
      )}
    </g>
  );
}

export function Tip({ x, children }: { x: number; children: React.ReactNode }) {
  // Kept inside the chart: past its middle it opens to the left of the day.
  const left = x / W > 0.5;
  return (
    <div className={cx('ca-usage-tip', left && 'is-left')} style={{ left: `${(x / W) * 100}%` }} role="status">
      {children}
    </div>
  );
}

/** A bar's outline: square at the baseline, its top corners rounded by `r`. */
export function bar(x: number, y: number, w: number, h: number, r: number): string {
  if (h <= 0) return '';
  return `M${x},${y + h} V${y + r} Q${x},${y} ${x + r},${y} H${x + w - r} Q${x + w},${y} ${x + w},${y + r} V${y + h} Z`;
}
