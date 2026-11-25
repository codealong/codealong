// Experimental session themes (`[experimental] session_themes`): collision-free, emblem-aware
// multi-color curated chords and grainy mesh backdrop styles for session windows and optionally
// Sessions list rows.
//
// Each theme is a 4-color chord hand-tuned separately for light and dark themes (warm + cool
// tension, never flat monochrome, clear of Codealong's semantic amber and alarm red). Assignment
// is deterministic and collision-free per project: active sessions claim their emblem's matching
// mood when free, or the closest free chord on the wheel so colors stay stable as sessions update.

import type { CSSProperties } from 'react';
import { imageUrl } from './api';
import { shortOf } from './devices';
import { readStored, textByName, writeStored, type Codec } from './stored';
import type { Emblem, SessionBackdrop } from './types';

export type SessionColorId =
  | 'dusk'
  | 'alabaster'
  | 'aurora'
  | 'kyoto'
  | 'coastal'
  | 'botanical'
  | 'blossom'
  | 'glacier'
  | 'terracotta'
  | 'twilight'
  | 'meadow'
  | 'silver';

export type SessionThemeVibe = 'grain-mesh' | 'horizon' | 'velvet' | 'prism' | 'paper';

export type SessionThemeSurface = 'full' | 'header';

export type SessionThemeIntensity = 'subtle' | 'medium' | 'vivid';

export interface ChordStopSet {
  /** Subtle tinted ground behind the mesh stops. */
  base: string;
  /** Four harmonized mesh control-node colors. */
  stops: readonly [string, string, string, string];
  /** Primary accent used for swatch dots, list rails, and subtle borders. */
  accent: string;
  /** Secondary accent for dual-tone swatch dots and list rails. */
  accent2: string;
}

export interface SessionThemeColor {
  id: SessionColorId;
  label: string;
  /** Representative hue in degrees for nearest-hue matching (emblem images & collision spread). */
  hue: number;
  light: ChordStopSet;
  dark: ChordStopSet;
}

export interface SessionThemePrefs {
  vibe: SessionThemeVibe;
  surface: SessionThemeSurface;
  intensity: SessionThemeIntensity;
  listRows: boolean;
}

export interface AssignedSessionTheme {
  color: SessionThemeColor;
  /** Where the chord came from: picked by hand, extracted from the session's AI backdrop, matched
   *  from its emblem, inherited from its parent helper session, or chosen automatically to avoid
   *  collisions. */
  source: 'picked' | 'backdrop' | 'emblem' | 'inherited' | 'auto';
  /** Stable 32-bit seed for deterministic mesh warping and glow positions. */
  seed: number;
  /** Drawn emblem picture URL when available. */
  emblemImage: string | null;
  /** Experimental AI-generated backdrop metadata when drawn for this session. */
  backdrop?: SessionBackdrop | null;
  /** Resolved URL for the AI-generated backdrop image when available. */
  backdropImage?: string | null;
  /** Extracted hex accent from the AI backdrop when harmonized automatically. */
  accentOverride?: string | null;
}

/** Twelve hand-tuned 4-color chords for light and dark themes, with warm/cool tension and clear
 *  separation from Codealong's semantic amber (needs you) and bright red (errors). */
export const SESSION_PALETTE: readonly SessionThemeColor[] = [
  {
    id: 'dusk',
    label: 'Velvet Dusk',
    hue: 278,
    light: {
      base: '#f6f4fa',
      stops: ['#c8bef4', '#efc2d4', '#b4d2f4', '#f2d6c8'],
      accent: '#7159c6',
      accent2: '#c46489',
    },
    dark: {
      base: '#12131b',
      stops: ['#4e46a8', '#9e466e', '#2d5c9c', '#885148'],
      accent: '#9c8df2',
      accent2: '#e288ad',
    },
  },
  {
    id: 'aurora',
    label: 'Nordic Aurora',
    hue: 186,
    light: {
      base: '#f3f8f8',
      stops: ['#9fdfd2', '#c8bcf5', '#a6d2f4', '#b9e7d6'],
      accent: '#1e8682',
      accent2: '#7562c9',
    },
    dark: {
      base: '#0f161a',
      stops: ['#1d7d7a', '#5d47a8', '#22659a', '#26785f'],
      accent: '#5ad1c9',
      accent2: '#a493f2',
    },
  },
  {
    id: 'alabaster',
    label: 'Warm Alabaster',
    hue: 138,
    light: {
      base: '#f8f6f1',
      stops: ['#e8d1ba', '#bdded0', '#d5e0c2', '#efdfcc'],
      accent: '#3e846a',
      accent2: '#a87c54',
    },
    dark: {
      base: '#141514',
      stops: ['#7d5d3f', '#2c6b58', '#4c6943', '#674e3b'],
      accent: '#75c4a6',
      accent2: '#cfa47a',
    },
  },
  {
    id: 'kyoto',
    label: 'Kyoto Mist',
    hue: 306,
    light: {
      base: '#f7f5f8',
      stops: ['#d9bce8', '#b2ded5', '#eddac7', '#c5c7f3'],
      accent: '#8a58ad',
      accent2: '#398b7f',
    },
    dark: {
      base: '#15131a',
      stops: ['#784591', '#2a7268', '#7d5c45', '#4c5196'],
      accent: '#c592e6',
      accent2: '#6bc4b7',
    },
  },
  {
    id: 'coastal',
    label: 'Coastal Fog',
    hue: 214,
    light: {
      base: '#f3f6fa',
      stops: ['#adc9e8', '#efcad3', '#b1dde2', '#c7d4ea'],
      accent: '#3b73ad',
      accent2: '#b8687f',
    },
    dark: {
      base: '#10151b',
      stops: ['#2c598a', '#8c4b62', '#266b7a', '#3b4e78'],
      accent: '#78b2eb',
      accent2: '#d98da4',
    },
  },
  {
    id: 'botanical',
    label: 'Botanical Dew',
    hue: 154,
    light: {
      base: '#f3f8f4',
      stops: ['#aedec0', '#b5d6f0', '#d2e2b5', '#b3e1d4'],
      accent: '#2c8456',
      accent2: '#467eb0',
    },
    dark: {
      base: '#0f1713',
      stops: ['#2a744e', '#2b5e8a', '#4e7036', '#246e66'],
      accent: '#66c993',
      accent2: '#75b1e3',
    },
  },
  {
    id: 'blossom',
    label: 'Orchid Mist',
    hue: 334,
    light: {
      base: '#f9f5f8',
      stops: ['#ebb8df', '#bcc6f6', '#f4c2c8', '#d3bff2'],
      accent: '#b44d98',
      accent2: '#5f6fd4',
    },
    dark: {
      base: '#18131a',
      stops: ['#944382', '#4554a6', '#994858', '#6b4799'],
      accent: '#e68acf',
      accent2: '#8fa0f5',
    },
  },
  {
    id: 'glacier',
    label: 'Glacier Prism',
    hue: 202,
    light: {
      base: '#f2f7fb',
      stops: ['#a4d2f5', '#b0e5d7', '#d0c2f4', '#bed6ee'],
      accent: '#247bb5',
      accent2: '#339482',
    },
    dark: {
      base: '#0f151c',
      stops: ['#22679e', '#237c6c', '#5b489c', '#33597d'],
      accent: '#68b9f0',
      accent2: '#66cfb9',
    },
  },
  {
    id: 'terracotta',
    label: 'Desert Clay',
    hue: 40,
    light: {
      base: '#f9f5f2',
      stops: ['#efbfae', '#dcc2dd', '#edd6bc', '#c4ded2'],
      accent: '#b45f46',
      accent2: '#895f8d',
    },
    dark: {
      base: '#191413',
      stops: ['#96503b', '#734873', '#805f40', '#326658'],
      accent: '#e0957d',
      accent2: '#bd8ebf',
    },
  },
  {
    id: 'twilight',
    label: 'Cobalt Twilight',
    hue: 252,
    light: {
      base: '#f3f5fb',
      stops: ['#b1c5f7', '#a8deea', '#e3bbdd', '#bfc0f5'],
      accent: '#4060c4',
      accent2: '#9c5694',
    },
    dark: {
      base: '#0f121d',
      stops: ['#3054ab', '#1f738a', '#82427a', '#4a43a3'],
      accent: '#829ff5',
      accent2: '#d48ac9',
    },
  },
  {
    id: 'meadow',
    label: 'Moss & Linen',
    hue: 118,
    light: {
      base: '#f6f7f2',
      stops: ['#cddab0', '#e9d6be', '#b8dbd8', '#c6d1df'],
      accent: '#5e7e36',
      accent2: '#3a8480',
    },
    dark: {
      base: '#131612',
      stops: ['#566e35', '#755b3c', '#2c6b68', '#43556b'],
      accent: '#9dc06b',
      accent2: '#6fc0ba',
    },
  },
  {
    id: 'silver',
    label: 'Graphite Silk',
    hue: 234,
    light: {
      base: '#f5f6f8',
      stops: ['#c4cedd', '#cccde9', '#e2dbd3', '#bed9de'],
      accent: '#5a6b84',
      accent2: '#6b6799',
    },
    dark: {
      base: '#131519',
      stops: ['#455368', '#52507f', '#645b51', '#346169'],
      accent: '#93a6c2',
      accent2: '#a4a1d4',
    },
  },
] as const;

export const PALETTE_BY_ID: Record<SessionColorId, SessionThemeColor> = Object.fromEntries(
  SESSION_PALETTE.map((c) => [c.id, c]),
) as Record<SessionColorId, SessionThemeColor>;

export const SESSION_VIBES: readonly { id: SessionThemeVibe; label: string; says: string }[] = [
  { id: 'grain-mesh', label: 'Grainy Mesh', says: 'Domain-warped 4-color mesh with fine analog film grain flowing seamlessly across the window.' },
  { id: 'horizon', label: 'Horizon Glow', says: 'Velvety multi-color light bloom spilling down from the top edge, fading cleanly into neutral ground.' },
  { id: 'velvet', label: 'Corner Bloom', says: 'Opposing soft ambient glows tucked into the corners with micro-grain, keeping the reading column pure.' },
  { id: 'prism', label: 'Frosted Prism', says: 'Sweeping blurred color ribbons with translucent frosted surfaces so the palette breathes through.' },
  { id: 'paper', label: 'Editorial Paper', says: 'Low-saturation atmospheric gradient with tactile fine-tooth paper grain for calm reading.' },
] as const;

export const SESSION_SURFACES: readonly { id: SessionThemeSurface; label: string; says: string }[] = [
  { id: 'full', label: 'Whole panel', says: 'Seamless atmosphere behind both the header and conversation with no harsh seam between them.' },
  { id: 'header', label: 'Header only', says: 'Confines the mesh to the session header while leaving the conversation on plain neutral ground.' },
] as const;

export const SESSION_INTENSITIES: readonly { id: SessionThemeIntensity; label: string; scale: number }[] = [
  { id: 'subtle', label: 'Subtle', scale: 0.65 },
  { id: 'medium', label: 'Medium', scale: 1.0 },
  { id: 'vivid', label: 'Vivid', scale: 1.4 },
] as const;

export const DEFAULT_PREFS: SessionThemePrefs = {
  vibe: 'grain-mesh',
  surface: 'full',
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

/** Deterministic FNV-1a 32-bit hash for stable seeds and fallback chord picks. */
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

/** Closest curated chord to an arbitrary hue angle (e.g. sampled from a drawn emblem). */
export function nearestPaletteColor(hueDeg: number): SessionColorId {
  let best = SESSION_PALETTE[0];
  let bestDist = Infinity;
  for (const c of SESSION_PALETTE) {
    const penalty = c.id === 'silver' ? 18 : 0;
    const d = hueDistance(hueDeg, c.hue) + penalty;
    if (d < bestDist) {
      bestDist = d;
      best = c;
    }
  }
  return best.id;
}

// Emblem name/emoji keywords mapped to curated multi-color moods so a session's default palette
// harmonizes with its emoji or drawn emblem subject.
const EMBLEM_HINTS: readonly [SessionColorId, readonly string[]][] = [
  ['botanical', ['bug', 'test-tube', 'leaf', 'leaves', 'plant', 'cactus', 'tree', 'dinosaur', 'dino', 'frog', 'clover', 'sprout', 'garden', 'checkmark', 'chart-up', 'money-bag', 'forest', 'pine', 'turtle', '🐛', '🧪', '🌿', '🌵', '🌳', '🦕', '🦖', '🐸', '🍀', '🌱', '✅', '📈', '💰', '🌲', '🐢']],
  ['meadow', ['avocado', 'olive', 'kiwi', 'pear', 'tent', 'owl', 'acorn', 'moss', 'bamboo', '🏕', '🦉', '🍐', '🥝', '🎋']],
  ['alabaster', ['compass', 'map', 'book', 'books', 'memo', 'scroll', 'sand', 'linen', 'coffee', 'bread', '🧭', '🗺', '📖', '📚', '📝', '☕']],
  ['aurora', ['ocean', 'wave', 'whale', 'dolphin', 'fish', 'water', 'plug', 'link', 'hook', 'microscope', '🌊', '🐳', '🐬', '🐟', '🔌', '🔗', '🪝', '🔬']],
  ['glacier', ['ice', 'snowflake', 'cloud', 'sky', 'bird', 'feather', 'wind', 'plane', 'globe', 'satellite', 'speech', 'mailbox', 'envelope', '💧', '🧊', '☁', '🐦', '✈', '🌐', '📡', '💬', '📬', '✉', '💎']],
  ['coastal', ['anchor', 'sailboat', 'shield', 'passport', 'lock', 'detective', 'magnifier', 'calendar', 'bar-chart', '⚓', '⛵', '🛡', '🛂', '🔒', '🕵', '🔍', '📅', '📊']],
  ['twilight', ['credit-card', 'night', 'galaxy', 'telescope', 'planet', 'comet', 'alien', '💳', '🌌', '🔭', '🪐', '☄', '👾']],
  ['dusk', ['rocket', 'space', 'moon', 'star', 'sparkles', 'crystal-ball', '🚀', '🌙', '✨', '🔮']],
  ['kyoto', ['brain', 'octopus', 'jellyfish', 'grape', 'umbrella', 'magic', 'puzzle', 'dice', 'music', 'movie', '🧠', '🐙', '🍇', '☂', '🧩', '🎲', '🎵', '🎬']],
  ['blossom', ['palette', 'paintbrush', 'flower', 'blossom', 'hibiscus', 'butterfly', 'flamingo', 'unicorn', 'ribbon', 'gift', 'megaphone', 'ladybug', 'strawberry', 'rose', 'tulip', 'crayon', 'microphone', 'bell', '🎨', '🖌', '🌸', '🌺', '🦋', '🦩', '🦄', '🎀', '🎁', '📣', '🐞', '🍓', '🌹', '🌷', '🖍', '🎤', '🔔']],
  ['terracotta', ['fire', 'fox', 'crab', 'brick', 'package', 'hammer', 'wrench', 'toolbox', 'construction', 'orange', 'pumpkin', 'volcano', 'tiger', 'lion', '🔥', '🦊', '🦀', '🧱', '📦', '🔨', '🔧', '🧰', '🚧', '🍊', '🎃', '🦁']],
  ['silver', ['gear', 'robot', 'nut-bolt', 'key', 'old-key', 'clipboard', 'pencil', 'newspaper', 'receipt', 'abacus', 'stopwatch', 'hourglass', 'alarm-clock', 'bank', 'camera', 'frame', 'chain', '⚙', '🤖', '🔩', '🔑', '🗝', '📋', '✏', '📰', '🧾', '🧮', '⏱', '⏳', '⏰', '🏦', '📸', '🖼', '⛓']],
];

/** Infers a natural curated chord from an emblem's emoji or subject name, when recognizable. */
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
  backdrop?: SessionBackdrop;
  /** Parent session key for helpers so they inherit their parent's chord. */
  helperOf?: string | null;
  archived?: boolean;
}

/** Extracts hue in degrees `[0, 360)` from a `#rrggbb` hex string, or null if neutral/invalid. */
export function hexHue(hex: string | null | undefined): number | null {
  if (!hex) return null;
  const m = /^#?([0-9a-f]{6})$/i.exec(hex.trim());
  if (!m) return null;
  const n = parseInt(m[1], 16);
  const r = ((n >> 16) & 255) / 255;
  const g = ((n >> 8) & 255) / 255;
  const b = (n & 255) / 255;
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const d = max - min;
  if (d < 0.04) return null;
  let h = 0;
  if (max === r) h = 60 * (((g - b) / d) % 6);
  else if (max === g) h = 60 * ((b - r) / d + 2);
  else h = 60 * ((r - g) / d + 4);
  return h < 0 ? h + 360 : h;
}

/**
 * Assigns a deterministic, collision-free multi-color chord to every session on the board.
 *
 * Within each project, manual picks reserve their chord first. Active top-level sessions are then
 * assigned in stable creation order (`startedAt`, then `key`), taking their emblem-derived or
 * hashed preferred chord when free, or the closest unused chord on the wheel so active sessions
 * side by side read apart without shuffling when status or activity changes. Helpers inherit their
 * parent's chord when the parent is present.
 */
export function assignSessionColors(
  sessions: readonly SessionColorInput[],
  manual: Readonly<Record<string, SessionColorId>> = {},
  imageHues: Readonly<Record<string, SessionColorId>> = {},
  aiOnly = false,
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
  const backdropImg = (s: SessionColorInput): string | null => {
    const pic = s.backdrop?.picture;
    if (!pic) return null;
    if (pic.startsWith('data:') || pic.startsWith('http://') || pic.startsWith('https://') || pic.startsWith('blob:')) return pic;
    return imageUrl(pic, shortOf(s.key));
  };

  for (const group of byProject.values()) {
    const counts = new Map<SessionColorId, number>(SESSION_PALETTE.map((c) => [c.id, 0]));
    const helpers: SessionColorInput[] = [];
    const autoTop: SessionColorInput[] = [];

    for (const s of group) {
      const picked = aiOnly ? undefined : manual[s.key];
      if (picked && PALETTE_BY_ID[picked]) {
        result.set(s.key, {
          color: PALETTE_BY_ID[picked],
          source: 'picked',
          seed: hashString(s.key),
          emblemImage: emblemImg(s),
          backdrop: s.backdrop ?? null,
          backdropImage: backdropImg(s),
          accentOverride: null,
        });
        if (!s.archived) counts.set(picked, (counts.get(picked) ?? 0) + 1);
      } else if (s.helperOf && byKey.has(s.helperOf)) {
        helpers.push(s);
      } else {
        autoTop.push(s);
      }
    }

    autoTop.sort((a, b) => {
      if (!!a.archived !== !!b.archived) return a.archived ? 1 : -1;
      return (a.startedAt || 0) - (b.startedAt || 0) || a.key.localeCompare(b.key);
    });

    for (const s of autoTop) {
      const backdropAccent = s.backdrop?.accent ?? null;
      const backdropHue = hexHue(backdropAccent);
      const fromBackdrop = backdropHue !== null ? nearestPaletteColor(backdropHue) : undefined;
      const fromImage = s.emblem?.picture ? imageHues[s.emblem.picture] : undefined;
      const fromEmblem = fromImage ?? emblemPreferredColor(s.emblem);
      const seed = hashString(s.key);
      const fallback = SESSION_PALETTE[seed % SESSION_PALETTE.length].id;
      const preferredId = fromBackdrop ?? fromEmblem ?? fallback;
      const preferredColor = PALETTE_BY_ID[preferredId];

      let chosen = preferredColor;
      if (fromBackdrop) {
        // When an AI backdrop supplied its own accent hue, harmonize the chord directly with it.
        chosen = preferredColor;
        if (!s.archived) counts.set(chosen.id, (counts.get(chosen.id) ?? 0) + 1);
      } else if (!s.archived) {
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
        source: fromBackdrop ? 'backdrop' : fromEmblem && chosen.id === fromEmblem ? 'emblem' : 'auto',
        seed,
        emblemImage: emblemImg(s),
        backdrop: s.backdrop ?? null,
        backdropImage: backdropImg(s),
        accentOverride: backdropAccent,
      });
    }

    for (const h of helpers) {
      const parent = h.helperOf ? result.get(h.helperOf) : undefined;
      if (parent) {
        result.set(h.key, {
          color: parent.color,
          source: 'inherited',
          seed: hashString(h.key),
          emblemImage: emblemImg(h) ?? parent.emblemImage,
          backdrop: null,
          backdropImage: null,
          accentOverride: parent.accentOverride ?? null,
        });
      } else {
        const seed = hashString(h.key);
        result.set(h.key, {
          color: SESSION_PALETTE[seed % SESSION_PALETTE.length],
          source: 'auto',
          seed,
          emblemImage: emblemImg(h),
          backdrop: null,
          backdropImage: null,
          accentOverride: null,
        });
      }
    }
  }

  return result;
}

/** CSS custom properties exposing the chord's light + dark stops and intensity scale.
 *  When `accentOverride` comes from an AI background image, every tint and accent stop is derived
 *  directly from that image color in OKLCH so UI surfaces purely complement the artwork without
 *  bleeding in colors from preset mesh chords. */
export function themeCssVars(color: SessionThemeColor, intensity: SessionThemeIntensity, accentOverride?: string | null): CSSProperties {
  const scale = SESSION_INTENSITIES.find((i) => i.id === intensity)?.scale ?? 1;
  if (accentOverride) {
    return {
      '--st-l-base': `color-mix(in oklch, ${accentOverride} 6%, #ffffff)`,
      '--st-l-1': `color-mix(in oklch, ${accentOverride} 24%, #ffffff)`,
      '--st-l-2': `color-mix(in oklch, ${accentOverride} 12%, #ffffff)`,
      '--st-l-3': `color-mix(in oklch, ${accentOverride} 18%, #ffffff)`,
      '--st-l-4': `color-mix(in oklch, ${accentOverride} 10%, #ffffff)`,
      '--st-l-accent': `color-mix(in oklch, ${accentOverride} 86%, #111111)`,
      '--st-l-accent2': `color-mix(in oklch, ${accentOverride} 62%, #ffffff)`,
      '--st-d-base': `color-mix(in oklch, ${accentOverride} 10%, #121318)`,
      '--st-d-1': `color-mix(in oklch, ${accentOverride} 34%, #121318)`,
      '--st-d-2': `color-mix(in oklch, ${accentOverride} 18%, #121318)`,
      '--st-d-3': `color-mix(in oklch, ${accentOverride} 26%, #121318)`,
      '--st-d-4': `color-mix(in oklch, ${accentOverride} 14%, #121318)`,
      '--st-d-accent': `color-mix(in oklch, ${accentOverride} 78%, #ffffff)`,
      '--st-d-accent2': `color-mix(in oklch, ${accentOverride} 52%, #121318)`,
      '--st-scale': String(scale),
    } as CSSProperties;
  }
  return {
    '--st-l-base': color.light.base,
    '--st-l-1': color.light.stops[0],
    '--st-l-2': color.light.stops[1],
    '--st-l-3': color.light.stops[2],
    '--st-l-4': color.light.stops[3],
    '--st-l-accent': color.light.accent,
    '--st-l-accent2': color.light.accent2,
    '--st-d-base': color.dark.base,
    '--st-d-1': color.dark.stops[0],
    '--st-d-2': color.dark.stops[1],
    '--st-d-3': color.dark.stops[2],
    '--st-d-4': color.dark.stops[3],
    '--st-d-accent': color.dark.accent,
    '--st-d-accent2': color.dark.accent2,
    '--st-scale': String(scale),
  } as CSSProperties;
}

// ── Shared reactive store across components and windows (through stored.ts) ────────────────────

export interface SessionThemeMode {
  enabled: boolean;
  aiEnabled: boolean;
}

let activeThemeMode: SessionThemeMode = { enabled: false, aiEnabled: false };

/** Current board-level session-theme mode, synced by SessionThemeProvider for context menus. */
export function sessionThemeMode(): SessionThemeMode {
  return activeThemeMode;
}

export function setSessionThemeMode(next: SessionThemeMode): void {
  activeThemeMode = next;
}

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
