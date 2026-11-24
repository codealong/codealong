// Generative session backdrop layers and header swatch picker (src/sessionTheme.ts):
// renders one of seven deterministic, readability-safe vibes (`wash`, `mesh`, `topo`, `arcs`,
// `waves`, `dots`, `emblem`) for a session header or conversation backdrop, plus a compact
// popover in SessionHeader so you can pick a session's color or try vibes live.

import { useMemo, useRef, useState } from 'react';
import {
  seededRandom,
  SESSION_INTENSITIES,
  SESSION_PALETTE,
  SESSION_SURFACES,
  SESSION_VIBES,
  themeCssVars,
  type AssignedSessionTheme,
  type SessionThemeIntensity,
  type SessionThemeVibe,
} from '../sessionTheme';
import { cx, Icon } from './primitives';
import type { ActiveSessionTheme } from './SessionThemeProvider';
import { useDismiss } from './useDismiss';

/** Builds smooth closed organic contour paths around a seeded focal point (for `topo`). */
function topoPaths(seed: number, rings: number, w: number, h: number): string[] {
  const rand = seededRandom(seed ^ 0x9e3779b9);
  const cx0 = w * (0.64 + rand() * 0.24);
  const cy0 = h * (0.25 + rand() * 0.55);
  const f1 = 2 + Math.floor(rand() * 2);
  const f2 = 3 + Math.floor(rand() * 3);
  const p1 = rand() * Math.PI * 2;
  const p2 = rand() * Math.PI * 2;
  const stretchX = 1.25 + rand() * 0.65;
  const stretchY = 0.72 + rand() * 0.38;
  const out: string[] = [];
  const steps = 40;
  for (let r = 1; r <= rings; r++) {
    const base = (Math.min(w, h) * 0.16) + r * (Math.max(w, h) * 0.085);
    const pts: [number, number][] = [];
    for (let i = 0; i < steps; i++) {
      const a = (i / steps) * Math.PI * 2;
      const wobble = 1 + 0.14 * Math.sin(f1 * a + p1 + r * 0.18) + 0.08 * Math.cos(f2 * a + p2 - r * 0.12);
      const x = cx0 + Math.cos(a) * base * stretchX * wobble;
      const y = cy0 + Math.sin(a) * base * stretchY * wobble;
      pts.push([x, y]);
    }
    let d = '';
    for (let i = 0; i < steps; i++) {
      const [x0, y0] = pts[i];
      const [x1, y1] = pts[(i + 1) % steps];
      const mx = ((x0 + x1) / 2).toFixed(1);
      const my = ((y0 + y1) / 2).toFixed(1);
      if (i === 0) {
        const [xp, yp] = pts[steps - 1];
        d += `M ${((xp + x0) / 2).toFixed(1)} ${((yp + y0) / 2).toFixed(1)} `;
      }
      d += `Q ${x0.toFixed(1)} ${y0.toFixed(1)} ${mx} ${my} `;
    }
    out.push(`${d}Z`);
  }
  return out;
}

/** Builds layered harmonic silk ribbons across the width (for `waves`). */
function wavePaths(seed: number, count: number, w: number, h: number): { d: string; fill?: string }[] {
  const rand = seededRandom(seed ^ 0x85ebca6b);
  const freq1 = 1.1 + rand() * 1.1;
  const freq2 = 2.2 + rand() * 1.4;
  const phase1 = rand() * Math.PI * 2;
  const phase2 = rand() * Math.PI * 2;
  const tilt = (rand() - 0.5) * h * 0.35;
  const out: { d: string; fill?: string }[] = [];
  const steps = 28;
  for (let l = 0; l < count; l++) {
    const t = l / Math.max(1, count - 1);
    const yBase = h * (0.14 + t * 0.78);
    const amp1 = h * (0.12 + 0.08 * Math.sin(t * Math.PI + phase1));
    const amp2 = h * 0.05;
    let d = '';
    for (let i = 0; i <= steps; i++) {
      const u = i / steps;
      const x = u * w;
      const y =
        yBase +
        tilt * (u - 0.5) +
        amp1 * Math.sin(u * Math.PI * freq1 + phase1 + l * 0.34) +
        amp2 * Math.cos(u * Math.PI * freq2 + phase2 - l * 0.22);
      d += `${i === 0 ? 'M' : 'L'} ${x.toFixed(1)} ${y.toFixed(1)} `;
    }
    const fill = l === Math.floor(count / 2) ? `${d} L ${w} ${h} L 0 ${h} Z` : undefined;
    out.push({ d, fill });
  }
  return out;
}

/** Builds architectural concentric arcs and orbits (for `arcs`). */
function arcShapes(seed: number, w: number, h: number) {
  const rand = seededRandom(seed ^ 0xc2b2ae35);
  const hubs = [
    { cx: w * (0.72 + rand() * 0.22), cy: h * (0.1 + rand() * 0.75), count: 5, step: Math.max(w, h) * 0.085, start: 22 },
    { cx: w * (0.44 + rand() * 0.2), cy: h * (0.85 + rand() * 0.4), count: 3, step: Math.max(w, h) * 0.11, start: 34 },
  ];
  const circles: { cx: number; cy: number; r: number; dash?: string; accent?: boolean }[] = [];
  for (let hi = 0; hi < hubs.length; hi++) {
    const hub = hubs[hi];
    for (let i = 0; i < hub.count; i++) {
      const r = hub.start + i * hub.step;
      const dashed = i === 1 || (hi === 1 && i === 0);
      circles.push({
        cx: hub.cx,
        cy: hub.cy,
        r,
        dash: dashed ? `${(10 + rand() * 14).toFixed(0)} ${(8 + rand() * 10).toFixed(0)}` : undefined,
        accent: hi === 0 && i === 2,
      });
    }
  }
  const angle = -0.35 + rand() * 0.7;
  const line = {
    x1: hubs[0].cx - Math.cos(angle) * w * 0.45,
    y1: hubs[0].cy - Math.sin(angle) * w * 0.45,
    x2: hubs[0].cx + Math.cos(angle) * w * 0.45,
    y2: hubs[0].cy + Math.sin(angle) * w * 0.45,
  };
  return { circles, line };
}

/** Builds a wave-modulated risograph halftone dot field (for `dots`). */
function dotField(seed: number, w: number, h: number, cols: number, rows: number) {
  const rand = seededRandom(seed ^ 0x27d4eb2f);
  const px = rand() * Math.PI * 2;
  const py = rand() * Math.PI * 2;
  const fx = 1.4 + rand() * 1.2;
  const fy = 1.2 + rand() * 1.1;
  const dots: { cx: number; cy: number; r: number; o: number; alt: boolean }[] = [];
  for (let r = 0; r < rows; r++) {
    for (let c = 0; c < cols; c++) {
      const u = (c + 0.5) / cols;
      const v = (r + 0.5) / rows;
      // Keep the left reading area calm; let the wave blossom toward the right/corner.
      const edgeFade = Math.max(0, Math.min(1, (u - 0.16) * 1.35));
      const wave = 0.5 + 0.5 * Math.sin(u * Math.PI * fx + px) * Math.cos(v * Math.PI * fy + py);
      const radius = (0.75 + wave * 2.1) * (0.45 + 0.55 * edgeFade);
      if (radius < 0.7 || edgeFade <= 0.04) continue;
      dots.push({
        cx: +(u * w).toFixed(1),
        cy: +(v * h).toFixed(1),
        r: +radius.toFixed(2),
        o: +(0.25 + wave * 0.65 * edgeFade).toFixed(2),
        alt: wave > 0.62,
      });
    }
  }
  return dots;
}

/** Builds seeded mesh blob positions in percentages (for `mesh` and `emblem` fallback). */
function meshBlobs(seed: number) {
  const rand = seededRandom(seed ^ 0x165667b1);
  return {
    x1: Math.round(62 + rand() * 28),
    y1: Math.round(10 + rand() * 35),
    x2: Math.round(28 + rand() * 32),
    y2: Math.round(55 + rand() * 35),
    x3: Math.round(80 + rand() * 18),
    y3: Math.round(62 + rand() * 32),
  };
}

export interface SessionBackdropProps {
  theme: AssignedSessionTheme;
  vibe: SessionThemeVibe;
  intensity: SessionThemeIntensity;
  /** `header`: fills SessionHeader; `thread`: subtle stationary ambient layer behind conversation;
   *  `preview`: miniature swatch card in Settings. */
  place: 'header' | 'thread' | 'preview';
  /** Working sessions get a very calm slow drift on mesh/waves/emblem (respects reduced motion). */
  working?: boolean;
}

/** Renders the atmospheric color + procedural/artwork backdrop behind a session surface. */
export function SessionBackdrop({ theme, vibe, intensity, place, working = false }: SessionBackdropProps) {
  const w = place === 'thread' ? 720 : place === 'preview' ? 220 : 640;
  const h = place === 'thread' ? 680 : place === 'preview' ? 92 : 140;

  const geometry = useMemo(() => {
    switch (vibe) {
      case 'topo':
        return { kind: 'topo' as const, paths: topoPaths(theme.seed, place === 'preview' ? 5 : place === 'thread' ? 10 : 7, w, h) };
      case 'waves':
        return { kind: 'waves' as const, paths: wavePaths(theme.seed, place === 'preview' ? 5 : place === 'thread' ? 9 : 6, w, h) };
      case 'arcs':
        return { kind: 'arcs' as const, shapes: arcShapes(theme.seed, w, h) };
      case 'dots':
        return { kind: 'dots' as const, dots: dotField(theme.seed, w, h, place === 'preview' ? 16 : 30, place === 'preview' ? 6 : place === 'thread' ? 18 : 8) };
      case 'mesh':
      case 'emblem':
        return { kind: 'mesh' as const, blobs: meshBlobs(theme.seed) };
      case 'wash':
        return { kind: 'wash' as const };
    }
  }, [vibe, theme.seed, place, w, h]);

  const vars = themeCssVars(theme.color, intensity);
  const meshStyle =
    geometry.kind === 'mesh'
      ? ({
          ...vars,
          '--st-x1': `${geometry.blobs.x1}%`,
          '--st-y1': `${geometry.blobs.y1}%`,
          '--st-x2': `${geometry.blobs.x2}%`,
          '--st-y2': `${geometry.blobs.y2}%`,
          '--st-x3': `${geometry.blobs.x3}%`,
          '--st-y3': `${geometry.blobs.y3}%`,
        } as React.CSSProperties)
      : vars;

  return (
    <div
      className={cx('ca-st-layer', `is-${place}`, `is-${vibe}`, working && 'is-working')}
      style={meshStyle}
      aria-hidden="true"
    >
      <div className="ca-st-wash" />
      {(vibe === 'mesh' || vibe === 'emblem') && <div className="ca-st-mesh" />}
      {vibe === 'emblem' && theme.emblemImage && (
        <img className="ca-st-art" src={theme.emblemImage} alt="" draggable={false} />
      )}
      {geometry.kind === 'topo' && (
        <svg className="ca-st-svg" viewBox={`0 0 ${w} ${h}`} preserveAspectRatio="xMaxYMid slice">
          {geometry.paths.map((d, i) => (
            <path key={i} d={d} className={cx('ca-st-stroke', i % 2 === 1 && 'is-alt')} />
          ))}
        </svg>
      )}
      {geometry.kind === 'waves' && (
        <svg className="ca-st-svg" viewBox={`0 0 ${w} ${h}`} preserveAspectRatio="none">
          {geometry.paths.map((p, i) => (
            <g key={i}>
              {p.fill && <path d={p.fill} className="ca-st-fill" />}
              <path d={p.d} className={cx('ca-st-stroke', i % 2 === 1 && 'is-alt')} />
            </g>
          ))}
        </svg>
      )}
      {geometry.kind === 'arcs' && (
        <svg className="ca-st-svg" viewBox={`0 0 ${w} ${h}`} preserveAspectRatio="xMaxYMid slice">
          <line x1={geometry.shapes.line.x1} y1={geometry.shapes.line.y1} x2={geometry.shapes.line.x2} y2={geometry.shapes.line.y2} className="ca-st-stroke is-faint" />
          {geometry.shapes.circles.map((c, i) => (
            <circle key={i} cx={c.cx} cy={c.cy} r={c.r} strokeDasharray={c.dash} className={cx('ca-st-stroke', c.accent && 'is-alt')} />
          ))}
        </svg>
      )}
      {geometry.kind === 'dots' && (
        <svg className="ca-st-svg" viewBox={`0 0 ${w} ${h}`} preserveAspectRatio="xMaxYMid slice">
          {geometry.dots.map((d, i) => (
            <circle key={i} cx={d.cx} cy={d.cy} r={d.r} opacity={d.o} className={cx('ca-st-dot', d.alt && 'is-alt')} />
          ))}
        </svg>
      )}
    </div>
  );
}

/** Compact color & vibe popover triggered from a session's header swatch dot. */
export function SessionThemePicker({ theme }: { theme: ActiveSessionTheme }) {
  const [open, setOpen] = useState(false);
  const anchor = useRef<HTMLSpanElement>(null);
  useDismiss(anchor, () => setOpen(false), { when: open });

  const { assigned, prefs, setPrefs, picked, pickColor } = theme;
  const sourceWord =
    assigned.source === 'picked'
      ? 'Picked'
      : assigned.source === 'emblem'
        ? 'Matched to emblem'
        : assigned.source === 'inherited'
          ? 'Inherited from parent'
          : 'Auto (collision-free)';

  return (
    <span ref={anchor} className="ca-st-pick">
      <button
        type="button"
        className={cx('ca-st-swatchbtn', open && 'is-open')}
        style={themeCssVars(assigned.color, prefs.intensity)}
        aria-label={`Session theme: ${assigned.color.label} (${sourceWord})`}
        aria-expanded={open}
        aria-haspopup="dialog"
        title={`Session theme: ${assigned.color.label} · ${sourceWord}. Click to pick a color or try vibes.`}
        onClick={(e) => {
          e.stopPropagation();
          setOpen((v) => !v);
        }}
      >
        <span className="ca-st-swatchdot" />
      </button>
      {open && (
        <div
          className="ca-st-popover"
          role="dialog"
          aria-label="Session theme"
          onClick={(e) => e.stopPropagation()}
        >
          <div className="ca-st-pophead">
            <span className="ca-st-poptitle">Session theme</span>
            <span className="ca-st-popmeta">{sourceWord}</span>
          </div>
          <div className="ca-st-swatches" role="radiogroup" aria-label="Session color">
            <button
              type="button"
              role="radio"
              aria-checked={picked === null}
              className={cx('ca-st-auto', picked === null && 'is-on')}
              title="Automatic: matches the session’s emblem and avoids active collisions"
              onClick={() => pickColor(null)}
            >
              Auto
            </button>
            {SESSION_PALETTE.map((c) => {
              const active = picked === c.id || (picked === null && assigned.color.id === c.id);
              return (
                <button
                  key={c.id}
                  type="button"
                  role="radio"
                  aria-checked={picked === c.id}
                  aria-label={c.label}
                  title={c.label}
                  className={cx('ca-st-swatch', picked === c.id && 'is-picked', active && 'is-active')}
                  style={themeCssVars(c, 'medium')}
                  onClick={() => pickColor(c.id)}
                >
                  {picked === c.id && <Icon name="check" size={10} stroke={2.4} />}
                </button>
              );
            })}
          </div>
          <div className="ca-st-popsec">
            <span className="ca-st-poplabel">Vibe</span>
            <div className="ca-st-pills">
              {SESSION_VIBES.map((v) => (
                <button
                  key={v.id}
                  type="button"
                  className={cx('ca-st-pill', prefs.vibe === v.id && 'is-on')}
                  title={v.says}
                  onClick={() => setPrefs({ vibe: v.id })}
                >
                  {v.label}
                </button>
              ))}
            </div>
          </div>
          <div className="ca-st-poprow">
            <div className="ca-st-popsec">
              <span className="ca-st-poplabel">Surface</span>
              <div className="ca-st-pills">
                {SESSION_SURFACES.map((s) => (
                  <button
                    key={s.id}
                    type="button"
                    className={cx('ca-st-pill', prefs.surface === s.id && 'is-on')}
                    title={s.says}
                    onClick={() => setPrefs({ surface: s.id })}
                  >
                    {s.label}
                  </button>
                ))}
              </div>
            </div>
            <div className="ca-st-popsec">
              <span className="ca-st-poplabel">Strength</span>
              <div className="ca-st-pills">
                {SESSION_INTENSITIES.map((i) => (
                  <button
                    key={i.id}
                    type="button"
                    className={cx('ca-st-pill', prefs.intensity === i.id && 'is-on')}
                    onClick={() => setPrefs({ intensity: i.id })}
                  >
                    {i.label}
                  </button>
                ))}
              </div>
            </div>
          </div>
          <label className="ca-st-popcheck">
            <input
              type="checkbox"
              checked={prefs.listRows}
              onChange={(e) => setPrefs({ listRows: e.target.checked })}
            />
            <span>Accent rows in Sessions list</span>
          </label>
        </div>
      )}
    </span>
  );
}
