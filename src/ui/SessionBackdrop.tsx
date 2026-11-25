// Atmospheric backdrop layer for experimental session themes (src/sessionTheme.ts,
// src/ui/meshCanvas.ts): renders either bespoke per-session AI artwork or one of five grainy
// multi-color mesh styles (`grain-mesh`, `horizon`, `velvet`, `prism`, `paper`).

import { useEffect, useMemo, useState, type CSSProperties } from 'react';
import {
  seededRandom,
  themeCssVars,
  type AssignedSessionTheme,
  type SessionThemeIntensity,
  type SessionThemeVibe,
} from '../sessionTheme';
import { getGrainTileUrl, renderMeshDataUrl } from './meshCanvas';
import { cx } from './primitives';

/** Tracks the active document theme (`light` or `dark`), following `data-theme` when set and
 *  otherwise `prefers-color-scheme` (matching ThemeSwitch.tsx and tokens.css). */
function useResolvedMode(): 'light' | 'dark' {
  const read = (): 'light' | 'dark' => {
    if (typeof document === 'undefined') return 'light';
    const explicit = document.documentElement.dataset.theme;
    if (explicit === 'dark' || explicit === 'light') return explicit;
    return typeof window !== 'undefined' && typeof window.matchMedia === 'function' && window.matchMedia('(prefers-color-scheme: dark)').matches
      ? 'dark'
      : 'light';
  };
  const [mode, setMode] = useState<'light' | 'dark'>(read);
  useEffect(() => {
    if (typeof document === 'undefined') return;
    const sync = () => setMode(read());
    sync();
    const obs = typeof MutationObserver !== 'undefined' ? new MutationObserver(sync) : null;
    obs?.observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme'] });
    const mq = typeof window !== 'undefined' && typeof window.matchMedia === 'function' ? window.matchMedia('(prefers-color-scheme: dark)') : null;
    mq?.addEventListener?.('change', sync);
    return () => {
      obs?.disconnect();
      mq?.removeEventListener?.('change', sync);
    };
  }, []);
  return mode;
}

/** Sweeping organic curves for the Frosted Prism vibe, seeded per session. */
function prismRibbons(seed: number) {
  const rand = seededRandom(seed ^ 0xc2b2ae35);
  const y1 = 18 + rand() * 24;
  const y2 = 44 + rand() * 26;
  const y3 = 72 + rand() * 20;
  const sway = (rand() - 0.5) * 32;
  return [
    { d: `M -60 ${y1} C 160 ${y1 - 24 + sway}, 380 ${y1 + 36 - sway}, 700 ${y1 + 8}`, stop: 1 },
    { d: `M -60 ${y2} C 200 ${y2 + 34 - sway}, 420 ${y2 - 28 + sway}, 700 ${y2 - 12}`, stop: 2 },
    { d: `M -60 ${y3} C 180 ${y3 - 30 + sway}, 440 ${y3 + 22 - sway}, 700 ${y3 - 6}`, stop: 3 },
  ];
}

export interface SessionBackdropProps {
  theme: AssignedSessionTheme;
  vibe: SessionThemeVibe;
  intensity: SessionThemeIntensity;
  /** `panel`: unified seamless backdrop spanning header + conversation; `header`: header only;
   *  `preview`: miniature swatch card in Settings. */
  place: 'panel' | 'header' | 'preview';
  /** Working sessions get a super subtle slow drift (disabled under prefers-reduced-motion). */
  working?: boolean;
  /** When AI backgrounds are enabled, skip rendering the procedural mesh before artwork arrives
   *  so sessions stay clean neutral rather than flashing an unrelated mesh beforehand. */
  aiOnly?: boolean;
}

/** Renders the AI artwork or multi-color grainy mesh atmosphere behind a session panel, header, or preview. */
export function SessionBackdrop({ theme, vibe, intensity, place, working = false, aiOnly = false }: SessionBackdropProps) {
  const mode = useResolvedMode();
  const chord = mode === 'dark' ? theme.color.dark : theme.color.light;

  const hasAi = !!theme.backdropImage && place !== 'preview';
  const suppressMesh = aiOnly && place !== 'preview';
  const showMesh = !hasAi && !suppressMesh;
  const darkArt = (theme.backdrop?.luminance ?? 128) < 130;

  const meshUrl = useMemo(() => (showMesh ? renderMeshDataUrl(chord, theme.seed, vibe, mode) : null), [showMesh, chord, theme.seed, vibe, mode]);
  const grainUrl = useMemo(() => getGrainTileUrl(), []);
  const ribbons = useMemo(() => (showMesh && vibe === 'prism' ? prismRibbons(theme.seed) : null), [showMesh, vibe, theme.seed]);

  const layerStyle = useMemo<CSSProperties>(() => {
    const vars = themeCssVars(theme.color, intensity, theme.accentOverride);
    return {
      ...vars,
      ...(meshUrl ? { '--st-mesh-img': `url("${meshUrl}")` } : null),
      ...(grainUrl ? { '--st-grain-img': `url("${grainUrl}")` } : null),
      ...(hasAi && theme.backdropImage ? { '--st-ai-img': `url("${theme.backdropImage}")` } : null),
    } as CSSProperties;
  }, [theme.color, theme.accentOverride, theme.backdropImage, hasAi, intensity, meshUrl, grainUrl]);

  if (suppressMesh && !hasAi) return null;

  return (
    <div
      className={cx(
        'ca-st-layer',
        `is-${place}`,
        `is-${vibe}`,
        hasAi && 'is-has-ai',
        hasAi && (darkArt ? 'is-dark-art' : 'is-light-art'),
        working && 'is-working',
      )}
      data-strength={intensity}
      style={layerStyle}
      aria-hidden="true"
    >
      {showMesh && <div className="ca-st-canvas" />}
      {hasAi && <div className="ca-st-ai" />}
      {ribbons && (
        <svg className="ca-st-prism" viewBox="0 0 640 120" preserveAspectRatio="none">
          {ribbons.map((r, i) => (
            <path key={i} d={r.d} className={cx('ca-st-ribbon', i === 1 && 'is-r2', i === 2 && 'is-r3')} />
          ))}
        </svg>
      )}
      <div className="ca-st-veil" />
      <div className="ca-st-grain" />
    </div>
  );
}
