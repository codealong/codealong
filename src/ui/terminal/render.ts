// Draws a terminal's scene (scene.ts) on a canvas: with WebGL2 where the window has it, every
// glyph from an atlas the browser's own text drawing fills (so the font and its fallbacks for
// emoji and other scripts are the system's, and text looks as it does elsewhere in the app), each
// tinted to its color; with Canvas 2D where it hasn't (a Linux desktop without GPU drivers),
// drawing the same text directly. The characters the terminal draws itself (shapes.ts) are drawn
// as shapes either way.

import { drawShape, isShape } from './shapes';
import type { Scene, SceneGlyph, SceneRect } from './scene';

/** The font the cells are drawn in: its family list and size in device pixels. */
export interface Font {
  family: string;
  size: number;
}

export interface Renderer {
  readonly kind: 'webgl2' | 'canvas2d';
  /** The canvas's size in device pixels, and the cell's. */
  resize(width: number, height: number, cellW: number, cellH: number, baseline: number, font: Font): void;
  draw(scene: Scene): void;
  dispose(): void;
}

/** Emoji drawn in their own colors rather than tinted. */
const COLORED = /\p{Extended_Pictographic}/u;

function fontOf(f: Font, bold: boolean, italic: boolean): string {
  return `${italic ? 'italic ' : ''}${bold ? '700' : '400'} ${f.size}px ${f.family}`;
}

function css(color: number, alpha = 1): string {
  const r = (color >> 16) & 255;
  const g = (color >> 8) & 255;
  const b = color & 255;
  return alpha >= 1 ? `rgb(${r},${g},${b})` : `rgba(${r},${g},${b},${alpha})`;
}

/** Draws one glyph with a 2D context: a shape, or the text, squeezed to its cells when the font's
 *  is wider (a symbol from a fallback font). */
function glyphTo(ctx: CanvasRenderingContext2D, g: { text: string; span: number; bold: boolean; italic: boolean }, x: number, y: number, cellW: number, cellH: number, baseline: number, font: Font) {
  const w = cellW * g.span;
  if (isShape(g.text) && drawShape(ctx, g.text, x, y, w, cellH)) return;
  ctx.font = fontOf(font, g.bold, g.italic);
  const width = ctx.measureText(g.text).width;
  if (width > w * 1.05) {
    ctx.save();
    ctx.translate(x, 0);
    ctx.scale(w / width, 1);
    ctx.fillText(g.text, 0, y + baseline);
    ctx.restore();
  } else ctx.fillText(g.text, x, y + baseline);
}

/** A renderer for the canvas: WebGL2 if the window has it, else Canvas 2D. */
export function makeRenderer(canvas: HTMLCanvasElement): Renderer {
  return webgl(canvas) ?? canvas2d(canvas);
}

function canvas2d(canvas: HTMLCanvasElement): Renderer {
  const ctx = canvas.getContext('2d', { alpha: false });
  if (!ctx) throw new Error('This window can’t draw a terminal.');
  let cell = { w: 1, h: 1, baseline: 1 };
  let font: Font = { family: 'monospace', size: 12 };
  const rect = (r: SceneRect) => {
    if (r.outline) {
      ctx.strokeStyle = css(r.color, r.alpha);
      ctx.lineWidth = 1;
      ctx.strokeRect(r.x + 0.5, r.y + 0.5, r.w - 1, r.h - 1);
    } else {
      ctx.fillStyle = css(r.color, r.alpha);
      ctx.fillRect(r.x, r.y, r.w, r.h);
    }
  };
  const glyph = (g: SceneGlyph) => {
    ctx.fillStyle = css(g.color, g.alpha);
    glyphTo(ctx, g, g.x, g.y, cell.w, cell.h, cell.baseline, font);
  };
  return {
    kind: 'canvas2d',
    resize(width, height, cellW, cellH, baseline, f) {
      canvas.width = width;
      canvas.height = height;
      cell = { w: cellW, h: cellH, baseline };
      font = f;
    },
    draw(scene) {
      ctx.fillStyle = css(scene.bg);
      ctx.fillRect(0, 0, canvas.width, canvas.height);
      ctx.textBaseline = 'alphabetic';
      scene.rects.forEach(rect);
      scene.glyphs.forEach(glyph);
      scene.over.forEach(rect);
      scene.overGlyphs.forEach(glyph);
    },
    dispose() {},
  };
}

/** Where a glyph is in the atlas, in device pixels, and whether it keeps its own colors. */
interface Slot {
  x: number;
  y: number;
  w: number;
  h: number;
  colored: boolean;
}

/** The glyphs drawn so far, white on transparent (but colored emoji), in a canvas that becomes a
 *  texture. Each slot has `PAD` around its cells so what reaches past them (an italic's tail) is
 *  kept. Full, it starts again. */
class Atlas {
  static readonly SIZE = 2048;
  static readonly PAD = 4;
  readonly canvas: HTMLCanvasElement;
  private readonly ctx: CanvasRenderingContext2D;
  private slots = new Map<string, Slot>();
  private x = 0;
  private y = 0;
  private rowH = 0;
  /** Changed since it was last sent to the GPU. */
  dirty = true;
  /** Counts the times it started again, which moves every glyph: a draw that saw it change
   *  places its glyphs again. */
  generation = 0;

  constructor(
    private cellW: number,
    private cellH: number,
    private baseline: number,
    private font: Font,
  ) {
    this.canvas = document.createElement('canvas');
    this.canvas.width = Atlas.SIZE;
    this.canvas.height = Atlas.SIZE;
    const ctx = this.canvas.getContext('2d', { willReadFrequently: false });
    if (!ctx) throw new Error('No 2D context for the glyph atlas.');
    this.ctx = ctx;
  }

  /** Starts again for another cell size or font. */
  reset(cellW: number, cellH: number, baseline: number, font: Font) {
    Object.assign(this, { cellW, cellH, baseline, font });
    this.clear();
  }

  private clear() {
    this.ctx.clearRect(0, 0, Atlas.SIZE, Atlas.SIZE);
    this.slots.clear();
    this.x = 0;
    this.y = 0;
    this.rowH = 0;
    this.dirty = true;
    this.generation += 1;
  }

  get(g: SceneGlyph): Slot {
    const key = `${g.bold ? 'b' : ''}${g.italic ? 'i' : ''}${g.span}|${g.text}`;
    const had = this.slots.get(key);
    if (had) return had;
    const w = this.cellW * g.span + Atlas.PAD * 2;
    const h = this.cellH + Atlas.PAD * 2;
    if (this.x + w > Atlas.SIZE) {
      this.x = 0;
      this.y += this.rowH;
      this.rowH = 0;
    }
    if (this.y + h > Atlas.SIZE) this.clear();
    const slot: Slot = { x: this.x, y: this.y, w, h, colored: COLORED.test(g.text) && !isShape(g.text) };
    this.x += w;
    this.rowH = Math.max(this.rowH, h);
    const ctx = this.ctx;
    ctx.save();
    ctx.beginPath();
    ctx.rect(slot.x, slot.y, w, h);
    ctx.clip();
    ctx.fillStyle = '#ffffff';
    ctx.textBaseline = 'alphabetic';
    glyphTo(ctx, g, slot.x + Atlas.PAD, slot.y + Atlas.PAD, this.cellW, this.cellH, this.baseline, this.font);
    ctx.restore();
    this.slots.set(key, slot);
    this.dirty = true;
    return slot;
  }
}

const QUAD = new Float32Array([0, 0, 1, 0, 0, 1, 1, 1]);

const RECT_VS = `#version 300 es
in vec2 corner;
in vec4 rect;
in vec4 color;
uniform vec2 size;
out vec4 v_color;
void main() {
  vec2 p = rect.xy + corner * rect.zw;
  gl_Position = vec4(p / size * 2.0 - 1.0, 0.0, 1.0) * vec4(1.0, -1.0, 1.0, 1.0);
  v_color = color;
}`;

const RECT_FS = `#version 300 es
precision mediump float;
in vec4 v_color;
out vec4 out_color;
void main() { out_color = vec4(v_color.rgb * v_color.a, v_color.a); }`;

const GLYPH_VS = `#version 300 es
in vec2 corner;
in vec4 rect;
in vec4 uv;
in vec4 color;
in float colored;
uniform vec2 size;
uniform float atlas;
out vec2 v_uv;
out vec4 v_color;
out float v_colored;
void main() {
  vec2 p = rect.xy + corner * rect.zw;
  gl_Position = vec4(p / size * 2.0 - 1.0, 0.0, 1.0) * vec4(1.0, -1.0, 1.0, 1.0);
  v_uv = (uv.xy + corner * uv.zw) / atlas;
  v_color = color;
  v_colored = colored;
}`;

const GLYPH_FS = `#version 300 es
precision mediump float;
uniform sampler2D tex;
in vec2 v_uv;
in vec4 v_color;
in float v_colored;
out vec4 out_color;
void main() {
  vec4 t = texture(tex, v_uv);
  out_color = v_colored > 0.5 ? t * v_color.a : vec4(v_color.rgb * t.a, t.a) * v_color.a;
}`;

function compile(gl: WebGL2RenderingContext, vs: string, fs: string): WebGLProgram {
  const shader = (type: number, src: string) => {
    const s = gl.createShader(type);
    if (!s) throw new Error('No shader.');
    gl.shaderSource(s, src);
    gl.compileShader(s);
    if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(s) ?? 'A shader didn’t compile.');
    return s;
  };
  const p = gl.createProgram();
  if (!p) throw new Error('No program.');
  gl.attachShader(p, shader(gl.VERTEX_SHADER, vs));
  gl.attachShader(p, shader(gl.FRAGMENT_SHADER, fs));
  gl.linkProgram(p);
  if (!gl.getProgramParameter(p, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(p) ?? 'A program didn’t link.');
  return p;
}

/** A program drawn as instanced quads: its per-instance attributes, `floats` a quad. */
interface Batch {
  program: WebGLProgram;
  vao: WebGLVertexArrayObject;
  buffer: WebGLBuffer;
  floats: number;
  size: WebGLUniformLocation | null;
}

function batch(gl: WebGL2RenderingContext, program: WebGLProgram, quad: WebGLBuffer, attrs: [string, number][]): Batch {
  const vao = gl.createVertexArray();
  const buffer = gl.createBuffer();
  if (!vao || !buffer) throw new Error('No buffers.');
  gl.bindVertexArray(vao);
  gl.bindBuffer(gl.ARRAY_BUFFER, quad);
  const corner = gl.getAttribLocation(program, 'corner');
  gl.enableVertexAttribArray(corner);
  gl.vertexAttribPointer(corner, 2, gl.FLOAT, false, 0, 0);
  gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
  const floats = attrs.reduce((n, [, size]) => n + size, 0);
  let offset = 0;
  for (const [name, size] of attrs) {
    const at = gl.getAttribLocation(program, name);
    gl.enableVertexAttribArray(at);
    gl.vertexAttribPointer(at, size, gl.FLOAT, false, floats * 4, offset * 4);
    gl.vertexAttribDivisor(at, 1);
    offset += size;
  }
  gl.bindVertexArray(null);
  return { program, vao, buffer, floats, size: gl.getUniformLocation(program, 'size') };
}

function webgl(canvas: HTMLCanvasElement): Renderer | null {
  const gl = canvas.getContext('webgl2', { alpha: false, antialias: false, premultipliedAlpha: true, preserveDrawingBuffer: false });
  if (!gl) return null;
  let rects: Batch;
  let glyphs: Batch;
  let texture: WebGLTexture | null;
  try {
    const quad = gl.createBuffer();
    if (!quad) return null;
    gl.bindBuffer(gl.ARRAY_BUFFER, quad);
    gl.bufferData(gl.ARRAY_BUFFER, QUAD, gl.STATIC_DRAW);
    rects = batch(gl, compile(gl, RECT_VS, RECT_FS), quad, [
      ['rect', 4],
      ['color', 4],
    ]);
    glyphs = batch(gl, compile(gl, GLYPH_VS, GLYPH_FS), quad, [
      ['rect', 4],
      ['uv', 4],
      ['color', 4],
      ['colored', 1],
    ]);
    texture = gl.createTexture();
  } catch (e) {
    console.warn('WebGL2 terminal renderer unavailable, drawing with Canvas 2D:', e);
    return null;
  }
  gl.bindTexture(gl.TEXTURE_2D, texture);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
  let atlas: Atlas | null = null;
  const rgb = (c: number): [number, number, number] => [((c >> 16) & 255) / 255, ((c >> 8) & 255) / 255, (c & 255) / 255];
  const pushRect = (out: number[], r: SceneRect) => {
    const [cr, cg, cb] = rgb(r.color);
    if (r.outline) {
      const t = 1;
      for (const [x, y, w, h] of [
        [r.x, r.y, r.w, t],
        [r.x, r.y + r.h - t, r.w, t],
        [r.x, r.y, t, r.h],
        [r.x + r.w - t, r.y, t, r.h],
      ])
        out.push(x, y, w, h, cr, cg, cb, r.alpha);
    } else out.push(r.x, r.y, r.w, r.h, cr, cg, cb, r.alpha);
  };
  const pushGlyph = (out: number[], g: SceneGlyph) => {
    if (!atlas) return;
    const s = atlas.get(g);
    const [cr, cg, cb] = rgb(g.color);
    out.push(g.x - Atlas.PAD, g.y - Atlas.PAD, s.w, s.h, s.x, s.y, s.w, s.h, cr, cg, cb, g.alpha, s.colored ? 1 : 0);
  };
  const drawBatch = (b: Batch, data: number[], extra?: () => void) => {
    if (!data.length) return;
    gl.useProgram(b.program);
    gl.uniform2f(b.size, canvas.width, canvas.height);
    extra?.();
    gl.bindVertexArray(b.vao);
    gl.bindBuffer(gl.ARRAY_BUFFER, b.buffer);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array(data), gl.STREAM_DRAW);
    gl.drawArraysInstanced(gl.TRIANGLE_STRIP, 0, 4, data.length / b.floats);
  };
  const withAtlas = () => {
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, texture);
    if (atlas?.dirty) {
      gl.pixelStorei(gl.UNPACK_PREMULTIPLY_ALPHA_WEBGL, true);
      gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, gl.RGBA, gl.UNSIGNED_BYTE, atlas.canvas);
      atlas.dirty = false;
    }
    gl.uniform1i(gl.getUniformLocation(glyphs.program, 'tex'), 0);
    gl.uniform1f(gl.getUniformLocation(glyphs.program, 'atlas'), Atlas.SIZE);
  };
  return {
    kind: 'webgl2',
    resize(width, height, cellW, cellH, baseline, font) {
      canvas.width = width;
      canvas.height = height;
      gl.viewport(0, 0, width, height);
      if (atlas) atlas.reset(cellW, cellH, baseline, font);
      else atlas = new Atlas(cellW, cellH, baseline, font);
    },
    draw(scene) {
      const [r, g, b] = rgb(scene.bg);
      gl.clearColor(r, g, b, 1);
      gl.clear(gl.COLOR_BUFFER_BIT);
      gl.enable(gl.BLEND);
      gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_ALPHA);
      const back: number[] = [];
      scene.rects.forEach((x) => pushRect(back, x));
      const over: number[] = [];
      scene.over.forEach((x) => pushRect(over, x));
      let text: number[] = [];
      let overText: number[] = [];
      const place = () => {
        text = [];
        overText = [];
        scene.glyphs.forEach((x) => pushGlyph(text, x));
        scene.overGlyphs.forEach((x) => pushGlyph(overText, x));
      };
      // The atlas filling up partway starts it again, which moves the glyphs placed before:
      // they're placed once more in the new one.
      const before = atlas?.generation;
      place();
      if (atlas && atlas.generation !== before) place();
      drawBatch(rects, back);
      drawBatch(glyphs, text, withAtlas);
      drawBatch(rects, over);
      drawBatch(glyphs, overText, withAtlas);
    },
    dispose() {
      gl.getExtension('WEBGL_lose_context')?.loseContext();
    },
  };
}
