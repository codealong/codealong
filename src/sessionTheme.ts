// Experimental session themes (`[experimental] session_themes`): collision-free, emblem-aware
// per-session colors and generative backdrop vibes for headers, conversation backgrounds and
// optionally Sessions list rows.
//
// Palette hues are curated in OKLCH to stay calm and readable in both light and dark themes and to
// stay clear of Codealong's semantic amber (needs you) and bright red (errors). Assignment is
// deterministic and collision-free per project: active sessions claim their emblem's preferred hue
// (or a stable hash of their key) when free, and otherwise the nearest free hue so colors never
// jump as sessions update. Helpers inherit their parent's color family.

import type { CSSProperties } from 'react';
import { imageUrl } from './api';
import { shortOf } from './devices';
import { readStored, textByName, writeStored, type Codec } from './stored';
import type { Emblem } from './types';

export type SessionColorId =
  | 'slate'
  | 'cobalt'
  | 'indigo'
  | 'plum'
  | 'orchid'
  | 'rose'
  | 'clay'
  | 'olive'
  | 'sage'
  | 'pine'
  | 'teal'
  | 'sky';

export type SessionThemeVibe = 'wash' | 'mesh' | 'topo' | 'arcs' | 'dots' | 'waves' | 'emblem';

export type SessionThemeSurface = 'header' | 'full';

export type SessionThemeIntensity = 'subtle' | 'medium' | 'vivid';

export interface SessionThemeColor {
  id: SessionColorId;
  label: string;
  /** Primary OKLCH hue in degrees. */
  hue: number;
  /** Harmonious companion OKLCH hue for gradients and multi-tone patterns. */
  hue2: number;
  /** Chroma multiplier so desaturated hues (slate) and richer hues balance evenly. */
  chroma: number;
}

export interface SessionThemePrefs {
  vibe: SessionThemeVibe;
  surface: SessionThemeSurface;
  intensity: SessionThemeIntensity;
  listRows: boolean;
}

export interface AssignedSessionTheme {
  color: SessionThemeColor;
  /** Where the color came from: picked by hand, matched from the session's emblem, inherited from
   *  its parent helper session, or chosen automatically to avoid collisions. */
  source: 'picked' | 'emblem' | 'inherited' | 'auto';
  /** Stable 32-bit seed for deterministic pattern geometry. */
  seed: number;
  /** Drawn emblem picture URL when available (used by the ambient art vibe). */
  emblemImage: string | null;
}

/** Curated OKLCH families, avoiding amber (65–95°) and alarm red (18–30°). */
export const SESSION_PALETTE: readonly SessionThemeColor[] = [
  { id: 'sky', label: 'Cerulean', hue: 222, hue2: 244, chroma: 0.92 },
  { id: 'cobalt', label: 'Cobalt', hue: 256, hue2: 280, chroma: 0.96 },
  { id: 'indigo', label: 'Indigo', hue: 282, hue2: 306, chroma: 0.95 },
  { id: 'plum', label: 'Plum', hue: 308, hue2: 332, chroma: 0.9 },
  { id: 'orchid', label: 'Orchid', hue: 336, hue2: 312, chroma: 0.88 },
  { id: 'rose', label: 'Dusty rose', hue: 358, hue2: 336, chroma: 0.84 },
  { id: 'clay', label: 'Terracotta', hue: 44, hue2: 26, chroma: 0.86 },
  { id: 'olive', label: 'Moss', hue: 122, hue2: 146, chroma: 0.84 },
  { id: 'sage', label: 'Sage', hue: 152, hue2: 176, chroma: 0.9 },
  { id: 'pine', label: 'Pine', hue: 174, hue2: 196, chroma: 0.92 },
  { id: 'teal', label: 'Sea glass', hue: 198, hue2: 220, chroma: 0.92 },
  { id: 'slate', label: 'Slate', hue: 238, hue2: 214, chroma: 0.48 },
] as const;

export const PALETTE_BY_ID: Record<SessionColorId, SessionThemeColor> = Object.fromEntries(
  SESSION_PALETTE.map((c) => [c.id, c]),
) as Record<SessionColorId, SessionThemeColor>;

export const SESSION_VIBES: readonly { id: SessionThemeVibe; label: string; says: string }[] = [
  { id: 'mesh', label: 'Aurora', says: 'Soft overlapping OKLCH color glows that fade cleanly into the surface.' },
  { id: 'topo', label: 'Topo', says: 'Fine topographic contour rings unique to each session.' },
  { id: 'arcs', label: 'Arcs', says: 'Architectural concentric arcs and hairline orbits.' },
  { id: 'waves', label: 'Waves', says: 'Flowing harmonic silk ribbons curving across the backdrop.' },
  { id: 'dots', label: 'Halftone', says: 'Subtle risograph dot-wave texture with gentle print warmth.' },
  { id: 'emblem', label: 'Ambient art', says: 'Soft blurred bloom from the session’s drawn emblem, or painterly dual-tone art.' },
  { id: 'wash', label: 'Soft tint', says: 'Quiet tonal color wash with no pattern.' },
] as const;

export const SESSION_SURFACES: readonly { id: SessionThemeSurface; label: string; says: string }[] = [
  { id: 'header', label: 'Header only', says: 'Color and texture live in the session header and fade into clean neutral ground for the conversation.' },
  { id: 'full', label: 'Header + thread', says: 'Extends a restrained ambient tint and corner motif behind the conversation while keeping text and code crisp.' },
] as const;

export const SESSION_INTENSITIES: readonly { id: SessionThemeIntensity; label: string; scale: number }[] = [
  { id: 'subtle', label: 'Subtle', scale: 0.65 },
  { id: 'medium', label: 'Medium', scale: 1.0 },
  { id: 'vivid', label: 'Vivid', scale: 1.45 },
] as const;

export const DEFAULT_PREFS: SessionThemePrefs = {
  vibe: 'mesh',
  surface: 'header',
  intensity: 'medium',
  listRows: false,
};

export const PREFS_KEY = 'codealong.experimental.sessionThemes.prefs';
export const PICKS_KEY = 'codealong.experimental.sessionThemes.picks';

const VIBE_IDS = new Set<string>(SESSION_VIBES.map((v) => v.id));
const SURFACE_IDS = new Set<string>(SESSION_SURFACES.map((s) => s.id));
const INTENSITY_IDS = new Set<string>(SESSION_INTENSITIES.map((i) => i.id));
const COLOR_IDS = new Set<string>(SESSION_PALETTE.map((c) => c.id));

const prefsCodec: Codec<SessionThemePrefs> = {
  parse(raw) {
    try {
      const v: unknown = JSON.parse(raw);
      if (!v || typeof v !== 'object' || Array.isArray(v)) return undefined;
      const o = v as Record<string, unknown>;
      return {
        vibe: typeof o.vibe === 'string' && VIBE_IDS.has(o.vibe) ? (o.vibe as SessionThemeVibe) : DEFAULT_PREFS.vibe,
        surface: typeof o.surface === 'string' && SURFACE_IDS.has(o.surface) ? (o.surface as SessionThemeSurface) : DEFAULT_PREFS.surface,
        intensity: typeof o.intensity === 'string' && INTENSITY_IDS.has(o.intensity) ? (o.intensity as SessionThemeIntensity) : DEFAULT_PREFS.intensity,
        listRows: typeof o.listRows === 'boolean' ? o.listRows : DEFAULT_PREFS.listRows,
      };
    } catch {
      return undefined;
    }
  },
  format: (v) => JSON.stringify(v),
};

/** Deterministic FNV-1a 32-bit hash for stable seeds and fallback hue picks. */
export function hashString(input: string): number {
  let h = 0x811c9dc5;
  for (let i = 0; i < input.length; i++) {
    h ^= input.charCodeAt(i);
    h = Math.imul(h, 0x01000193);
  }
  return h >>> 0;
}

/** Deterministic PRNG in [0, 1) seeded by a 32-bit integer (Mulberry32). */
export function seededRandom(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** Shortest angular distance between two hues on the 360° color wheel. */
export function hueDistance(a: number, b: number): number {
  const d = Math.abs(((a % 360) + 360) % 360 - (((b % 360) + 360) % 360));
  return d > 180 ? 360 - d : d;
}

/** Closest curated palette family to an arbitrary hue angle (e.g. sampled from a drawn emblem). */
export function nearestPaletteColor(hueDeg: number): SessionColorId {
  let best = SESSION_PALETTE[0];
  let bestDist = Infinity;
  for (const c of SESSION_PALETTE) {
    // Skip low-chroma slate when matching vivid hues unless nothing else is close.
    const penalty = c.id === 'slate' ? 18 : 0;
    const d = hueDistance(hueDeg, c.hue) + penalty;
    if (d < bestDist) {
      bestDist = d;
      best = c;
    }
  }
  return best.id;
}

// Emblem name/emoji keywords mapped to curated color families so a session's default palette
// harmonizes with its emoji or drawn emblem subject.
const EMBLEM_HINTS: readonly [SessionColorId, readonly string[]][] = [
  ['sage', ['bug', 'test-tube', 'leaf', 'leaves', 'plant', 'cactus', 'tree', 'dinosaur', 'dino', 'frog', 'clover', 'sprout', 'garden', 'checkmark', 'chart-up', 'money-bag', '🐛', '🧪', '🌿', '🌵', '🌳', '🦕', '🦖', '🐸', '🍀', '🌱', '✅', '📈', '💰']],
  ['pine', ['forest', 'pine', 'turtle', 'snake', 'emerald', 'bamboo', 'fern', 'broccoli', 'moss', '🐊', '🐢', '🌲', '🎋', '🦎']],
  ['olive', ['avocado', 'olive', 'kiwi', 'pear', 'compass', 'map', 'backpack', 'tent', 'owl', 'acorn', '🧭', '🗺', '🏕', '🦉', '🍐', '🥝']],
  ['teal', ['ocean', 'wave', 'whale', 'dolphin', 'fish', 'water', 'drop', 'ice', 'anchor', 'sailboat', 'plug', 'link', 'hook', 'microscope', '🌊', '🐳', '🐬', '🐟', '💧', '🧊', '⚓', '⛵', '🔌', '🔗', '🪝', '🔬']],
  ['sky', ['cloud', 'sky', 'bird', 'feather', 'wind', 'snowflake', 'plane', 'paper-plane', 'globe', 'satellite', 'speech', 'mailbox', 'envelope', '☁', '🐦', '✈', '🌐', '📡', '💬', '📬', '✉', '💎']],
  ['cobalt', ['whale', 'shark', 'shield', 'passport', 'lock', 'detective', 'magnifier', 'books', 'book', 'calendar', 'credit-card', 'bar-chart', '🛡', '🛂', '🔒', '🕵', '🔍', '📚', '📖', '📅', '💳', '📊']],
  ['indigo', ['rocket', 'space', 'planet', 'moon', 'star', 'sparkles', 'crystal-ball', 'telescope', 'night', 'galaxy', 'comet', 'alien', '🚀', '🪐', '🌙', '✨', '🔮', '🔭', '🌌', '👾', '☄']],
  ['plum', ['brain', 'octopus', 'jellyfish', 'grape', 'eggplant', 'umbrella', 'magic', 'wand', 'puzzle', 'dice', 'music', 'movie', '🧠', '🐙', '🍇', '☂', '🧩', '🎲', '🎵', '🎬', '🔮']],
  ['orchid', ['palette', 'paintbrush', 'flower', 'blossom', 'hibiscus', 'butterfly', 'flamingo', 'unicorn', 'ribbon', 'gift', 'megaphone', '🎨', '🖌', '🌸', '🌺', '🦋', '🦩', '🦄', '🎀', '🎁', '📣']],
  ['rose', ['ladybug', 'strawberry', 'cherry', 'heart', 'rose', 'tulip', 'crayon', 'microphone', 'bell', 'heart', '🐞', '🍓', '🍒', '🌹', '🌷', '🖍', '🎤', '🔔', '🎈']],
  ['clay', ['fire', 'fox', 'crab', 'brick', 'package', 'hammer', 'wrench', 'toolbox', 'construction', 'coffee', 'bread', 'orange', 'pumpkin', 'volcano', 'tiger', 'lion', '🔥', '🦊', '🦀', '🧱', '📦', '🔨', '🔧', '🧰', '🚧', '☕', '🍊', '🎃', '🦁']],
  ['slate', ['gear', 'robot', 'nut-bolt', 'key', 'old-key', 'clipboard', 'memo', 'pencil', 'newspaper', 'receipt', 'abacus', 'stopwatch', 'hourglass', 'alarm-clock', 'bank', 'camera', 'frame', 'chain', '⚙', '🤖', '🔩', '🔑', '🗝', '📋', '📝', '✏', '📰', '🧾', '🧮', '⏱', '⏳', '⏰', '🏦', '📸', '🖼', '⛓']],
];

/** Infers a natural curated color from an emblem's emoji or subject name, when recognizable. */
export function emblemPreferredColor(emblem?: Emblem | null): SessionColorId | null {
  if (!emblem) return null;
  const emoji = emblem.emoji?.trim() ?? '';
  const name = emblem.name?.toLowerCase().trim() ?? '';
  if (!emoji && !name) return null;
  for (const [colorId, hints] of EMBLEM_HINTS) {
    for (const h of hints) {
      if (emoji && emoji.includes(h)) return colorId;
      if (name && (name === h || name.includes(h))) return colorId;
    }
  }
  return null;
}

export interface SessionColorInput {
  key: string;
  project: string;
  startedAt: number;
  emblem?: Emblem;
  /** Parent session key for helpers so they inherit their parent's color family. */
  helperOf?: string | null;
  archived?: boolean;
}

/**
 * Assigns a deterministic, collision-free color to every session on the board.
 *
 * Within each project, manual picks reserve their color first. Active top-level sessions are then
 * assigned in stable creation order (`startedAt`, then `key`), taking their emblem-derived or
 * hashed preferred color when free, or the closest unused color on the wheel so active sessions
 * side by side read apart without shuffling when status or activity changes. Helpers inherit their
 * parent's color family when the parent is present.
 */
export function assignSessionColors(
  sessions: readonly SessionColorInput[],
  manual: Readonly<Record<string, SessionColorId>> = {},
  imageHues: Readonly<Record<string, SessionColorId>> = {},
): Map<string, AssignedSessionTheme> {
  const result = new Map<string, AssignedSessionTheme>();
  const byProject = new Map<string, SessionColorInput[]>();
  const byKey = new Map<string, SessionColorInput>();

  for (const s of sessions) {
    byKey.set(s.key, s);
    const list = byProject.get(s.project);
    if (list) list.push(s);
    else byProject.set(s.project, [s]);
  }

  const emblemImg = (s: SessionColorInput): string | null => (s.emblem?.picture ? imageUrl(s.emblem.picture, shortOf(s.key)) : null);

  for (const group of byProject.values()) {
    const counts = new Map<SessionColorId, number>(SESSION_PALETTE.map((c) => [c.id, 0]));
    const helpers: SessionColorInput[] = [];
    const autoTop: SessionColorInput[] = [];

    for (const s of group) {
      const picked = manual[s.key];
      if (picked && PALETTE_BY_ID[picked]) {
        result.set(s.key, {
          color: PALETTE_BY_ID[picked],
          source: 'picked',
          seed: hashString(s.key),
          emblemImage: emblemImg(s),
        });
        if (!s.archived) counts.set(picked, (counts.get(picked) ?? 0) + 1);
      } else if (s.helperOf && byKey.has(s.helperOf)) {
        helpers.push(s);
      } else {
        autoTop.push(s);
      }
    }

    // Active sessions claim free slots first, in stable creation order; archived sessions follow
    // without blocking active ones from getting their preferred hue.
    autoTop.sort((a, b) => {
      if (!!a.archived !== !!b.archived) return a.archived ? 1 : -1;
      return (a.startedAt || 0) - (b.startedAt || 0) || a.key.localeCompare(b.key);
    });

    for (const s of autoTop) {
      const fromImage = s.emblem?.picture ? imageHues[s.emblem.picture] : undefined;
      const fromEmblem = fromImage ?? emblemPreferredColor(s.emblem);
      const seed = hashString(s.key);
      const fallback = SESSION_PALETTE[seed % SESSION_PALETTE.length].id;
      const preferredId = fromEmblem ?? fallback;
      const preferredColor = PALETTE_BY_ID[preferredId];

      let chosen = preferredColor;
      if (!s.archived) {
        let bestCount = Infinity;
        let bestDist = Infinity;
        for (const candidate of SESSION_PALETTE) {
          const used = counts.get(candidate.id) ?? 0;
          const dist = hueDistance(candidate.hue, preferredColor.hue);
          if (used < bestCount || (used === bestCount && dist < bestDist)) {
            bestCount = used;
            bestDist = dist;
            chosen = candidate;
          }
        }
        counts.set(chosen.id, (counts.get(chosen.id) ?? 0) + 1);
      }

      result.set(s.key, {
        color: chosen,
        source: fromEmblem && chosen.id === fromEmblem ? 'emblem' : 'auto',
        seed,
        emblemImage: emblemImg(s),
      });
    }

    // Helpers inherit their parent's color and pattern motif (with a slight seed offset).
    for (const h of helpers) {
      const parent = h.helperOf ? result.get(h.helperOf) : undefined;
      if (parent) {
        result.set(h.key, {
          color: parent.color,
          source: 'inherited',
          seed: hashString(h.key),
          emblemImage: emblemImg(h) ?? parent.emblemImage,
        });
      } else {
        const seed = hashString(h.key);
        result.set(h.key, {
          color: SESSION_PALETTE[seed % SESSION_PALETTE.length],
          source: 'auto',
          seed,
          emblemImage: emblemImg(h),
        });
      }
    }
  }

  return result;
}

/** CSS custom properties that drive a themed surface (header, conversation, or list row). */
export function themeCssVars(color: SessionThemeColor, intensity: SessionThemeIntensity): CSSProperties {
  const scale = SESSION_INTENSITIES.find((i) => i.id === intensity)?.scale ?? 1;
  return {
    '--st-h': String(color.hue),
    '--st-h2': String(color.hue2),
    '--st-c': String(color.chroma),
    '--st-scale': String(scale),
  } as CSSProperties;
}

// ── Shared reactive store across components and windows (through stored.ts) ────────────────────

const listeners = new Set<() => void>();
function notifyAll() {
  for (const fn of listeners) fn();
}

export function subscribeThemeStore(fn: () => void): () => void {
  listeners.add(fn);
  return () => {
    listeners.delete(fn);
  };
}

export function readThemePrefs(): SessionThemePrefs {
  return readStored(PREFS_KEY, prefsCodec, DEFAULT_PREFS);
}

export function writeThemePrefs(next: SessionThemePrefs): void {
  writeStored(PREFS_KEY, prefsCodec, next);
  notifyAll();
}

export function readColorPicks(): Record<string, SessionColorId> {
  const raw = readStored(PICKS_KEY, textByName, {});
  const out: Record<string, SessionColorId> = {};
  for (const [k, v] of Object.entries(raw)) {
    if (COLOR_IDS.has(v)) out[k] = v as SessionColorId;
  }
  return out;
}

export function writeColorPick(sessionKey: string, color: SessionColorId | null): void {
  const next = { ...readColorPicks() };
  if (color === null) delete next[sessionKey];
  else next[sessionKey] = color;
  writeStored(PICKS_KEY, textByName, next);
  notifyAll();
}

