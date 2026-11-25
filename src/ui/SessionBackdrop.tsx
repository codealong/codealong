// Grainy multi-color mesh backdrop layers and live header swatch picker (src/sessionTheme.ts,
// src/ui/meshCanvas.ts): renders five distinct atmospheric styles (`grain-mesh`, `horizon`,
// `velvet`, `prism`, `paper`) with domain-warped 4-color chords and crisp film grain.

import { useEffect, useMemo, useRef, useState, type CSSProperties } from 'react';
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
import { getGrainTileUrl, renderMeshDataUrl } from './meshCanvas';
import { cx, Icon } from './primitives';
import type { ActiveSessionTheme } from './SessionThemeProvider';
import { useDismiss } from './useDismiss';

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
}

/** Renders the multi-color grainy mesh atmosphere behind a session panel, header, or preview. */
export function SessionBackdrop({ theme, vibe, intensity, place, working = false }: SessionBackdropProps) {
  const mode = useResolvedMode();
  const chord = mode === 'dark' ? theme.color.dark : theme.color.light;

  const meshUrl = useMemo(() => renderMeshDataUrl(chord, theme.seed, vibe, mode), [chord, theme.seed, vibe, mode]);
  const grainUrl = useMemo(() => getGrainTileUrl(), []);
  const ribbons = useMemo(() => (vibe === 'prism' ? prismRibbons(theme.seed) : null), [vibe, theme.seed]);

  const hasAi = !!theme.backdropImage && place !== 'preview';
  const darkArt = (theme.backdrop?.luminance ?? 128) < 130;

  const layerStyle = useMemo<CSSProperties>(() => {
    const vars = themeCssVars(theme.color, intensity, theme.accentOverride);
    return {
      ...vars,
      ...(meshUrl ? { '--st-mesh-img': `url("${meshUrl}")` } : null),
      ...(grainUrl ? { '--st-grain-img': `url("${grainUrl}")` } : null),
      ...(hasAi && theme.backdropImage ? { '--st-ai-img': `url("${theme.backdropImage}")` } : null),
    } as CSSProperties;
  }, [theme.color, theme.accentOverride, theme.backdropImage, hasAi, intensity, meshUrl, grainUrl]);

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
      style={layerStyle}
      aria-hidden="true"
    >
      <div className="ca-st-canvas" />
      {hasAi && <div className="ca-st-ai" />}
      {!hasAi && ribbons && (
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

/** Compact multi-color chord & mesh-vibe popover triggered from a session's header swatch orb. */
export function SessionThemePicker({ theme }: { theme: ActiveSessionTheme }) {
  const [open, setOpen] = useState(false);
  const [redrawing, setRedrawing] = useState(false);
  const anchor = useRef<HTMLSpanElement>(null);
  useDismiss(anchor, () => setOpen(false), { when: open });

  const { assigned, aiEnabled, prefs, setPrefs, picked, pickColor, redraw } = theme;
  useEffect(() => {
    setRedrawing(false);
  }, [assigned.backdrop?.picture]);
  const sourceWord =
    assigned.source === 'picked'
      ? 'Picked'
      : assigned.source === 'backdrop'
        ? 'Matched to artwork'
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
        style={themeCssVars(assigned.color, prefs.intensity, assigned.accentOverride)}
        aria-label={`Session theme: ${assigned.color.label} (${sourceWord})`}
        aria-expanded={open}
        aria-haspopup="dialog"
        title={`Session theme: ${assigned.backdrop?.name ? `${assigned.backdrop.name} · ` : ''}${assigned.color.label} · ${sourceWord}. Click to customize.`}
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
            <span className="ca-st-poptitle">{assigned.color.label}</span>
            <span className="ca-st-popmeta">{sourceWord}</span>
          </div>
          {(assigned.backdrop || aiEnabled) && (
            <div className="ca-st-popai">
              <div className="ca-st-popai-copy">
                <span className="ca-st-poplabel">AI background</span>
                <span className="ca-st-popai-name">
                  {redrawing ? 'Redrawing…' : assigned.backdrop?.name || 'Drawing automatically…'}
                </span>
              </div>
              <button
                type="button"
                className="ca-st-redraw"
                disabled={redrawing}
                title="Draw a fresh AI background for this session"
                onClick={() => {
                  setRedrawing(true);
                  redraw();
                }}
              >
                <Icon name="refresh" size={11} />
                Redraw
              </button>
            </div>
          )}
          <div className="ca-st-swatches" role="radiogroup" aria-label="Session palette">
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
            <span className="ca-st-poplabel">Mesh vibe</span>
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
