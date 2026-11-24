// React context for experimental session themes (src/sessionTheme.ts): computes collision-free,
// emblem-aware colors for every session on the board, samples dominant hues from drawn emblem
// pictures once they load, and syncs vibe/surface/intensity preferences and manual color picks
// across every open window (main, peek, kept desktop windows, waiting, notch).

import { createContext, useContext, useEffect, useMemo, useState, type ReactNode } from 'react';
import { imageUrl } from '../api';
import { shortOf } from '../devices';
import {
  assignSessionColors,
  DEFAULT_PREFS,
  emblemPreferredColor,
  hashString,
  nearestPaletteColor,
  PALETTE_BY_ID,
  PICKS_KEY,
  PREFS_KEY,
  readColorPicks,
  readThemePrefs,
  SESSION_PALETTE,
  subscribeThemeStore,
  writeColorPick,
  writeThemePrefs,
  type AssignedSessionTheme,
  type SessionColorId,
  type SessionColorInput,
  type SessionThemePrefs,
} from '../sessionTheme';
import type { Board, Lane, Session } from '../types';

export interface SessionThemeContextValue {
  enabled: boolean;
  prefs: SessionThemePrefs;
  setPrefs: (patch: Partial<SessionThemePrefs>) => void;
  themes: ReadonlyMap<string, AssignedSessionTheme>;
  picks: Readonly<Record<string, SessionColorId>>;
  pickColor: (sessionKey: string, color: SessionColorId | null) => void;
}

const OFF_CONTEXT: SessionThemeContextValue = {
  enabled: false,
  prefs: DEFAULT_PREFS,
  setPrefs: () => {},
  themes: new Map(),
  picks: {},
  pickColor: () => {},
};

const SessionThemeContext = createContext<SessionThemeContextValue>(OFF_CONTEXT);

// Cached dominant palette color per drawn emblem filename so we only decode each image once.
const sampledHueCache = new Map<string, SessionColorId | null>();
const samplingInFlight = new Set<string>();

function sampleDrawnEmblemHue(pictureKey: string, url: string, onDone: (color: SessionColorId | null) => void): void {
  if (sampledHueCache.has(pictureKey)) {
    onDone(sampledHueCache.get(pictureKey) ?? null);
    return;
  }
  if (samplingInFlight.has(pictureKey) || typeof Image === 'undefined' || typeof document === 'undefined') return;
  samplingInFlight.add(pictureKey);
  const img = new Image();
  img.crossOrigin = 'anonymous';
  img.onload = () => {
    samplingInFlight.delete(pictureKey);
    try {
      const size = 16;
      const canvas = document.createElement('canvas');
      canvas.width = size;
      canvas.height = size;
      const ctx = canvas.getContext('2d', { willReadFrequently: true });
      if (!ctx) {
        sampledHueCache.set(pictureKey, null);
        return;
      }
      ctx.drawImage(img, 0, 0, size, size);
      const data = ctx.getImageData(0, 0, size, size).data;
      let sumCos = 0;
      let sumSin = 0;
      let weightTotal = 0;
      for (let i = 0; i < data.length; i += 4) {
        const a = data[i + 3] / 255;
        if (a < 0.4) continue;
        const r = data[i] / 255;
        const g = data[i + 1] / 255;
        const b = data[i + 2] / 255;
        const max = Math.max(r, g, b);
        const min = Math.min(r, g, b);
        const delta = max - min;
        const sat = max === 0 ? 0 : delta / max;
        if (sat < 0.16 || max < 0.14 || min > 0.92) continue;
        let hue = 0;
        if (delta > 0) {
          if (max === r) hue = 60 * (((g - b) / delta) % 6);
          else if (max === g) hue = 60 * ((b - r) / delta + 2);
          else hue = 60 * ((r - g) / delta + 4);
        }
        if (hue < 0) hue += 360;
        const w = sat * a;
        const rad = (hue * Math.PI) / 180;
        sumCos += Math.cos(rad) * w;
        sumSin += Math.sin(rad) * w;
        weightTotal += w;
      }
      if (weightTotal < 0.1) {
        sampledHueCache.set(pictureKey, null);
        onDone(null);
        return;
      }
      let meanDeg = (Math.atan2(sumSin, sumCos) * 180) / Math.PI;
      if (meanDeg < 0) meanDeg += 360;
      const picked = nearestPaletteColor(meanDeg);
      sampledHueCache.set(pictureKey, picked);
      onDone(picked);
    } catch {
      // Cross-origin taint or unavailable canvas falls back to name/hash matching.
      sampledHueCache.set(pictureKey, null);
      onDone(null);
    }
  };
  img.onerror = () => {
    samplingInFlight.delete(pictureKey);
  };
  img.src = url;
}

export function useSessionThemePrefs(): [SessionThemePrefs, (patch: Partial<SessionThemePrefs>) => void] {
  const [prefs, setPrefs] = useState<SessionThemePrefs>(readThemePrefs);
  useEffect(() => {
    const sync = () => setPrefs(readThemePrefs());
    const unsub = subscribeThemeStore(sync);
    const onStorage = (e: StorageEvent) => {
      if (e.key === PREFS_KEY || e.key === null) sync();
    };
    window.addEventListener('storage', onStorage);
    return () => {
      unsub();
      window.removeEventListener('storage', onStorage);
    };
  }, []);
  const update = (patch: Partial<SessionThemePrefs>) => {
    const merged = { ...readThemePrefs(), ...patch };
    setPrefs(merged);
    writeThemePrefs(merged);
  };
  return [prefs, update];
}

export type ThemeBoard = Pick<Board, 'lanes' | 'experimental'>;

export function SessionThemeProvider({ board, extraLanes, children }: { board: ThemeBoard | null | undefined; extraLanes?: readonly Lane[]; children: ReactNode }) {
  const enabled = !!board?.experimental?.session_themes;
  const [prefs, setPrefs] = useSessionThemePrefs();
  const [picks, setPicks] = useState<Record<string, SessionColorId>>(readColorPicks);
  const [imageHues, setImageHues] = useState<Record<string, SessionColorId>>(() => {
    const init: Record<string, SessionColorId> = {};
    for (const [k, v] of sampledHueCache) if (v) init[k] = v;
    return init;
  });

  useEffect(() => {
    const sync = () => setPicks(readColorPicks());
    const unsub = subscribeThemeStore(sync);
    const onStorage = (e: StorageEvent) => {
      if (e.key === PICKS_KEY || e.key === null) sync();
    };
    window.addEventListener('storage', onStorage);
    return () => {
      unsub();
      window.removeEventListener('storage', onStorage);
    };
  }, []);

  const inputs = useMemo<SessionColorInput[]>(() => {
    if (!enabled) return [];
    const seen = new Set<string>();
    const out: SessionColorInput[] = [];
    const collect = (lanes: readonly Lane[] | undefined) => {
      if (!lanes) return;
      for (const lane of lanes) {
        for (const s of lane.sessions) {
          if (s.visitor || seen.has(s.key)) continue;
          seen.add(s.key);
          out.push({
            key: s.key,
            project: lane.project,
            startedAt: s.startedAt ?? s.lastActivity,
            emblem: s.emblem,
            helperOf: s.helper ? (s.origin?.key ?? null) : null,
            archived: s.archived,
          });
        }
      }
    };
    collect(board?.lanes);
    collect(extraLanes);
    return out;
  }, [enabled, board?.lanes, extraLanes]);

  useEffect(() => {
    if (!enabled || inputs.length === 0) return;
    let alive = true;
    for (const s of inputs) {
      const pic = s.emblem?.picture;
      if (!pic || sampledHueCache.has(pic)) continue;
      sampleDrawnEmblemHue(pic, imageUrl(pic, shortOf(s.key)), (color) => {
        if (!alive || !color) return;
        setImageHues((prev) => (prev[pic] === color ? prev : { ...prev, [pic]: color }));
      });
    }
    return () => {
      alive = false;
    };
  }, [enabled, inputs]);

  const themes = useMemo(() => (enabled ? assignSessionColors(inputs, picks, imageHues) : new Map<string, AssignedSessionTheme>()), [enabled, inputs, picks, imageHues]);

  const value = useMemo<SessionThemeContextValue>(
    () => ({
      enabled,
      prefs,
      setPrefs,
      themes,
      picks,
      pickColor: (sessionKey, color) => {
        writeColorPick(sessionKey, color);
        setPicks(readColorPicks());
      },
    }),
    [enabled, prefs, setPrefs, themes, picks],
  );

  return <SessionThemeContext.Provider value={value}>{children}</SessionThemeContext.Provider>;
}

export interface ActiveSessionTheme {
  assigned: AssignedSessionTheme;
  prefs: SessionThemePrefs;
  setPrefs: (patch: Partial<SessionThemePrefs>) => void;
  picked: SessionColorId | null;
  pickColor: (color: SessionColorId | null) => void;
}

/** Reads the active session theme for `session`, or null when the experiment is off. */
export function useSessionTheme(session: Pick<Session, 'key' | 'emblem' | 'helper' | 'origin'> | null | undefined): ActiveSessionTheme | null {
  const ctx = useContext(SessionThemeContext);
  if (!ctx.enabled || !session) return null;
  const fromMap = ctx.themes.get(session.key);
  const assigned: AssignedSessionTheme = fromMap ?? {
    color: PALETTE_BY_ID[emblemPreferredColor(session.emblem) ?? SESSION_PALETTE[hashString(session.key) % SESSION_PALETTE.length].id],
    source: emblemPreferredColor(session.emblem) ? 'emblem' : 'auto',
    seed: hashString(session.key),
    emblemImage: session.emblem?.picture ? imageUrl(session.emblem.picture, shortOf(session.key)) : null,
  };
  return {
    assigned,
    prefs: ctx.prefs,
    setPrefs: ctx.setPrefs,
    picked: ctx.picks[session.key] ?? null,
    pickColor: (color) => ctx.pickColor(session.key, color),
  };
}
