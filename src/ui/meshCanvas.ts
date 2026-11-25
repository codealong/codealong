// Offscreen 2D domain-warped mesh renderer and cached fine film-grain texture generator for
// experimental session themes (SessionBackdrop.tsx).
//
// Renders a silky, non-circular 4-color fluid mesh onto a compact offscreen canvas (~140x100 px,
// <2ms once per session/mode/vibe) using 2-octave smooth value-noise domain warping and softmax
// radial-basis blending in linear light, so upscaling via GPU bilinear filtering stays completely
// smooth and band-free without WebGL context limits or scroll jank. Film grain is generated once
// globally as a crisp 128x128 monochrome tile and tiled at 1:1 Retina resolution.

import { seededRandom, type ChordStopSet, type SessionThemeVibe } from '../sessionTheme';

type RGB = [number, number, number];

function parseHex(hex: string): RGB {
  const clean = hex.replace('#', '');
  const n = parseInt(clean, 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

/** 2D smooth value noise with deterministic permutation table from `seed`. */
function makeNoise2D(seed: number) {
  const rand = seededRandom(seed ^ 0x5bf03635);
  const perm = new Uint8Array(512);
  const p = new Uint8Array(256);
  for (let i = 0; i < 256; i++) p[i] = i;
  for (let i = 255; i > 0; i--) {
    const j = Math.floor(rand() * (i + 1));
    const tmp = p[i];
    p[i] = p[j];
    p[j] = tmp;
  }
  for (let i = 0; i < 512; i++) perm[i] = p[i & 255];

  const fade = (t: number) => t * t * t * (t * (t * 6 - 15) + 10);
  const lerp = (a: number, b: number, t: number) => a + (b - a) * t;
  const val = (ix: number, iy: number) => perm[(perm[ix & 255] + (iy & 255)) & 255] / 255;

  return (x: number, y: number): number => {
    const ix = Math.floor(x);
    const iy = Math.floor(y);
    const fx = x - ix;
    const fy = y - iy;
    const u = fade(fx);
    const v = fade(fy);
    const a = val(ix, iy);
    const b = val(ix + 1, iy);
    const c = val(ix, iy + 1);
    const d = val(ix + 1, iy + 1);
    return lerp(lerp(a, b, u), lerp(c, d, u), v) * 2 - 1;
  };
}

interface MeshNode {
  x: number;
  y: number;
  rgb: RGB;
  sharpness: number;
  weight: number;
}

const meshCache = new Map<string, string>();
const MAX_CACHE = 96;

/** Renders a domain-warped 4-color mesh data URL, cached by chord + seed + vibe + mode. */
export function renderMeshDataUrl(chord: ChordStopSet, seed: number, vibe: SessionThemeVibe, mode: 'light' | 'dark'): string | null {
  if (typeof document === 'undefined') return null;
  const cacheKey = `${mode}:${vibe}:${seed}:${chord.stops.join(',')}`;
  const hit = meshCache.get(cacheKey);
  if (hit !== undefined) return hit;

  const w = 136;
  const h = 104;
  const canvas = document.createElement('canvas');
  canvas.width = w;
  canvas.height = h;
  let ctx: CanvasRenderingContext2D | null = null;
  try {
    ctx = canvas.getContext('2d');
  } catch {
    return null;
  }
  if (!ctx) return null;

  const rand = seededRandom(seed ^ 0x9e3779b9);
  const noiseA = makeNoise2D(seed ^ 0x27d4eb2f);
  const noiseB = makeNoise2D(seed ^ 0x85ebca6b);

  const base = parseHex(chord.base);
  const stops = chord.stops.map(parseHex);

  // Place 5 organic control nodes (the 4 chord stops + base anchor) tailored to each vibe's composition.
  const nodes: MeshNode[] =
    vibe === 'horizon'
      ? [
          { x: 0.14 + rand() * 0.22, y: 0.04 + rand() * 0.14, rgb: stops[0], sharpness: 3.1, weight: 1.25 },
          { x: 0.48 + rand() * 0.22, y: 0.02 + rand() * 0.16, rgb: stops[1], sharpness: 2.9, weight: 1.3 },
          { x: 0.78 + rand() * 0.18, y: 0.08 + rand() * 0.2, rgb: stops[2], sharpness: 2.8, weight: 1.2 },
          { x: 0.34 + rand() * 0.32, y: 0.24 + rand() * 0.18, rgb: stops[3], sharpness: 2.6, weight: 0.95 },
          { x: 0.5, y: 0.86, rgb: base, sharpness: 1.9, weight: 1.9 },
        ]
      : vibe === 'velvet'
        ? [
            { x: 0.82 + rand() * 0.16, y: 0.06 + rand() * 0.16, rgb: stops[0], sharpness: 3.6, weight: 1.35 },
            { x: 0.62 + rand() * 0.2, y: 0.18 + rand() * 0.16, rgb: stops[1], sharpness: 3.4, weight: 1.05 },
            { x: 0.06 + rand() * 0.18, y: 0.82 + rand() * 0.16, rgb: stops[2], sharpness: 3.5, weight: 1.25 },
            { x: 0.24 + rand() * 0.18, y: 0.74 + rand() * 0.16, rgb: stops[3], sharpness: 3.4, weight: 0.9 },
            { x: 0.48, y: 0.48, rgb: base, sharpness: 2.2, weight: 2.25 },
          ]
        : vibe === 'paper'
          ? [
              { x: 0.18 + rand() * 0.26, y: 0.12 + rand() * 0.24, rgb: stops[0], sharpness: 2.1, weight: 1.0 },
              { x: 0.72 + rand() * 0.22, y: 0.16 + rand() * 0.28, rgb: stops[1], sharpness: 2.0, weight: 1.05 },
              { x: 0.64 + rand() * 0.26, y: 0.72 + rand() * 0.22, rgb: stops[2], sharpness: 2.0, weight: 0.9 },
              { x: 0.14 + rand() * 0.26, y: 0.68 + rand() * 0.22, rgb: stops[3], sharpness: 2.1, weight: 0.85 },
              { x: 0.5, y: 0.5, rgb: base, sharpness: 1.6, weight: 1.6 },
            ]
          : // grain-mesh & prism: lush, full-bleed fluid Apple/Arc-style 4-color pool
            [
              { x: 0.12 + rand() * 0.28, y: 0.08 + rand() * 0.26, rgb: stops[0], sharpness: 2.7, weight: 1.2 },
              { x: 0.68 + rand() * 0.26, y: 0.08 + rand() * 0.26, rgb: stops[1], sharpness: 2.6, weight: 1.25 },
              { x: 0.74 + rand() * 0.22, y: 0.64 + rand() * 0.28, rgb: stops[2], sharpness: 2.6, weight: 1.15 },
              { x: 0.14 + rand() * 0.28, y: 0.66 + rand() * 0.26, rgb: stops[3], sharpness: 2.7, weight: 1.1 },
              { x: 0.44 + rand() * 0.14, y: 0.44 + rand() * 0.14, rgb: base, sharpness: 1.8, weight: 0.72 },
            ];

  const warpAmt = vibe === 'paper' ? 0.14 : vibe === 'velvet' ? 0.17 : 0.24;
  const freq = vibe === 'paper' ? 1.5 : 2.1;
  const img = ctx.createImageData(w, h);
  const data = img.data;
  // Tiny dither breaks up 8-bit quantization steps completely before the crisp grain tile overlays.
  let ditherState = (seed ^ 0x165667b1) >>> 0;

  for (let py = 0; py < h; py++) {
    const ny = py / (h - 1);
    for (let px = 0; px < w; px++) {
      const nx = px / (w - 1);
      // Two-octave domain warp creates silky ink-in-water flow instead of circular spots.
      const wx = nx + warpAmt * (noiseA(nx * freq, ny * freq) * 0.72 + noiseA(nx * freq * 2.1 + 4.3, ny * freq * 2.1 + 1.7) * 0.28);
      const wy = ny + warpAmt * (noiseB(nx * freq + 2.7, ny * freq + 5.1) * 0.72 + noiseB(nx * freq * 2.1 + 8.1, ny * freq * 2.1 + 3.4) * 0.28);

      let r = 0;
      let g = 0;
      let b = 0;
      let wSum = 0;
      for (let i = 0; i < nodes.length; i++) {
        const n = nodes[i];
        const dx = wx - n.x;
        const dy = wy - n.y;
        const weight = n.weight * Math.exp(-(dx * dx + dy * dy) * n.sharpness);
        wSum += weight;
        r += n.rgb[0] * weight;
        g += n.rgb[1] * weight;
        b += n.rgb[2] * weight;
      }
      const inv = wSum > 0 ? 1 / wSum : 1;
      ditherState = (Math.imul(ditherState, 1664525) + 1013904223) >>> 0;
      const dither = ((ditherState >>> 24) / 255 - 0.5) * 1.4;
      const idx = (py * w + px) * 4;
      data[idx] = Math.max(0, Math.min(255, Math.round(r * inv + dither)));
      data[idx + 1] = Math.max(0, Math.min(255, Math.round(g * inv + dither)));
      data[idx + 2] = Math.max(0, Math.min(255, Math.round(b * inv + dither)));
      data[idx + 3] = 255;
    }
  }

  ctx.putImageData(img, 0, 0);
  const url = canvas.toDataURL('image/png');
  if (meshCache.size >= MAX_CACHE) {
    const first = meshCache.keys().next().value;
    if (first !== undefined) meshCache.delete(first);
  }
  meshCache.set(cacheKey, url);
  return url;
}

let grainTileUrl: string | null | undefined;

/** Returns a cached 128x128 monochrome analog film-grain PNG data URL, rendered once per session. */
export function getGrainTileUrl(): string | null {
  if (grainTileUrl !== undefined) return grainTileUrl;
  if (typeof document === 'undefined') {
    grainTileUrl = null;
    return null;
  }
  const size = 128;
  const canvas = document.createElement('canvas');
  canvas.width = size;
  canvas.height = size;
  let ctx: CanvasRenderingContext2D | null = null;
  try {
    ctx = canvas.getContext('2d');
  } catch {
    grainTileUrl = null;
    return null;
  }
  if (!ctx) {
    grainTileUrl = null;
    return null;
  }
  const rand = seededRandom(0x4f1bbcdc);
  const img = ctx.createImageData(size, size);
  const data = img.data;
  for (let i = 0; i < data.length; i += 4) {
    // Gaussian-like triangular distribution centered on mid-gray for authentic analog film grain.
    const v = Math.round(((rand() + rand()) * 0.5) * 255);
    data[i] = v;
    data[i + 1] = v;
    data[i + 2] = v;
    data[i + 3] = 255;
  }
  ctx.putImageData(img, 0, 0);
  grainTileUrl = canvas.toDataURL('image/png');
  return grainTileUrl;
}
