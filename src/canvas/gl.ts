// The Canvas view's GPU layer, in WebGL2: the dot grid, the lines between related cards, and,
// zoomed out past reading, every card as a block in its status color. Cards you can read are
// HTML on top of this (CanvasView.tsx), since only a few dozen fit on screen at that size; the
// GPU carries what grows with the canvas. It draws only when asked, never on a loop.

import type { Camera } from './model';

export type Rgba = [number, number, number, number];

/** A block: a rounded rect in canvas coordinates. */
export interface Block {
  x: number;
  y: number;
  w: number;
  h: number;
  color: Rgba;
}

/** A line between two points in canvas coordinates, `width` in screen pixels. */
export interface Line {
  x1: number;
  y1: number;
  x2: number;
  y2: number;
  color: Rgba;
  width: number;
}

const GRID_VS = `#version 300 es
in vec2 a_pos;
void main() { gl_Position = vec4(a_pos, 0.0, 1.0); }`;

// Dots every 24 canvas units, doubling the step while they'd sit closer than 14 pixels, and
// fading in as a step's dots spread out, so zooming never pops.
const GRID_FS = `#version 300 es
precision highp float;
uniform vec3 u_cam;     // x, y, zoom
uniform float u_dpr;
uniform float u_height;
uniform vec4 u_color;
out vec4 o;
float dots(vec2 world, float step, float zoom) {
  vec2 g = world / step;
  vec2 d = (g - round(g)) * step * zoom; // screen px from the nearest dot
  float r = length(d);
  return 1.0 - smoothstep(0.6, 1.3, r);
}
void main() {
  vec2 px = vec2(gl_FragCoord.x, u_height - gl_FragCoord.y) / u_dpr;
  vec2 world = (px - u_cam.xy) / u_cam.z;
  float step = 24.0;
  float screen = step * u_cam.z;
  for (int i = 0; i < 12; i++) { if (screen >= 14.0) break; step *= 2.0; screen *= 2.0; }
  float fade = smoothstep(14.0, 28.0, screen);
  float a = max(dots(world, step * 2.0, u_cam.z), dots(world, step, u_cam.z) * fade);
  o = vec4(u_color.rgb, u_color.a * a);
  o.rgb *= o.a;
}`;

const BLOCK_VS = `#version 300 es
in vec2 a_corner;
in vec4 a_rect;
in vec4 a_color;
uniform vec3 u_cam;
uniform vec2 u_size;
out vec2 v_local;
out vec2 v_half;
out vec4 v_color;
void main() {
  vec2 world = a_rect.xy + a_corner * a_rect.zw;
  vec2 px = world * u_cam.z + u_cam.xy;
  gl_Position = vec4(px / u_size * 2.0 - 1.0, 0.0, 1.0);
  gl_Position.y = -gl_Position.y;
  v_half = a_rect.zw * u_cam.z * 0.5;
  v_local = (a_corner - 0.5) * a_rect.zw * u_cam.z;
  v_color = a_color;
}`;

const BLOCK_FS = `#version 300 es
precision highp float;
in vec2 v_local;
in vec2 v_half;
in vec4 v_color;
out vec4 o;
void main() {
  float r = min(6.0, min(v_half.x, v_half.y));
  vec2 q = abs(v_local) - v_half + r;
  float d = length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - r;
  float a = v_color.a * (1.0 - smoothstep(-0.5, 0.5, d));
  o = vec4(v_color.rgb * a, a);
}`;

const LINE_VS = `#version 300 es
in vec2 a_corner;
in vec4 a_ends;
in vec4 a_color;
in float a_width;
uniform vec3 u_cam;
uniform vec2 u_size;
out vec4 v_color;
out float v_across;
out float v_width;
void main() {
  vec2 p1 = a_ends.xy * u_cam.z + u_cam.xy;
  vec2 p2 = a_ends.zw * u_cam.z + u_cam.xy;
  vec2 dir = p2 - p1;
  float len = max(length(dir), 0.0001);
  vec2 n = vec2(-dir.y, dir.x) / len;
  float w = a_width + 2.0;
  vec2 px = mix(p1, p2, a_corner.x) + n * (a_corner.y - 0.5) * w;
  gl_Position = vec4(px / u_size * 2.0 - 1.0, 0.0, 1.0);
  gl_Position.y = -gl_Position.y;
  v_color = a_color;
  v_across = (a_corner.y - 0.5) * w;
  v_width = a_width;
}`;

const LINE_FS = `#version 300 es
precision highp float;
in vec4 v_color;
in float v_across;
in float v_width;
out vec4 o;
void main() {
  float a = v_color.a * (1.0 - smoothstep(v_width * 0.5 - 0.5, v_width * 0.5 + 0.5, abs(v_across)));
  o = vec4(v_color.rgb * a, a);
}`;

function compile(gl: WebGL2RenderingContext, vs: string, fs: string): WebGLProgram {
  const p = gl.createProgram();
  if (!p) throw new Error('WebGL: no program');
  for (const [type, src] of [
    [gl.VERTEX_SHADER, vs],
    [gl.FRAGMENT_SHADER, fs],
  ] as const) {
    const s = gl.createShader(type);
    if (!s) throw new Error('WebGL: no shader');
    gl.shaderSource(s, src);
    gl.compileShader(s);
    if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) throw new Error(`WebGL: ${gl.getShaderInfoLog(s)}`);
    gl.attachShader(p, s);
  }
  gl.linkProgram(p);
  if (!gl.getProgramParameter(p, gl.LINK_STATUS)) throw new Error(`WebGL: ${gl.getProgramInfoLog(p)}`);
  return p;
}

/** A unit quad as two triangles: (0,0) to (1,1). */
const CORNERS = new Float32Array([0, 0, 1, 0, 0, 1, 0, 1, 1, 0, 1, 1]);

interface Instanced {
  vao: WebGLVertexArrayObject;
  buffer: WebGLBuffer;
  count: number;
}

export class CanvasGl {
  private gl: WebGL2RenderingContext;
  private grid: { prog: WebGLProgram; vao: WebGLVertexArrayObject };
  private blockProg: WebGLProgram;
  private lineProg: WebGLProgram;
  private blocks: Instanced;
  private lines: Instanced;
  private frame = 0;

  gridColor: Rgba = [0, 0, 0, 0.12];

  constructor(private canvas: HTMLCanvasElement) {
    const gl = canvas.getContext('webgl2', { antialias: false, premultipliedAlpha: true, alpha: true });
    if (!gl) throw new Error('WebGL2 is not available.');
    this.gl = gl;
    gl.enable(gl.BLEND);
    gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_ALPHA);

    const gridProg = compile(gl, GRID_VS, GRID_FS);
    const gridVao = gl.createVertexArray();
    gl.bindVertexArray(gridVao);
    const tri = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, tri);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 3, -1, -1, 3]), gl.STATIC_DRAW);
    const loc = gl.getAttribLocation(gridProg, 'a_pos');
    gl.enableVertexAttribArray(loc);
    gl.vertexAttribPointer(loc, 2, gl.FLOAT, false, 0, 0);
    this.grid = { prog: gridProg, vao: gridVao as WebGLVertexArrayObject };

    this.blockProg = compile(gl, BLOCK_VS, BLOCK_FS);
    this.blocks = this.instanced(this.blockProg, [
      ['a_rect', 4],
      ['a_color', 4],
    ]);
    this.lineProg = compile(gl, LINE_VS, LINE_FS);
    this.lines = this.instanced(this.lineProg, [
      ['a_ends', 4],
      ['a_color', 4],
      ['a_width', 1],
    ]);
    gl.bindVertexArray(null);
  }

  /** A vertex array drawing the unit quad once per instance, with per-instance attributes. */
  private instanced(prog: WebGLProgram, attrs: [string, number][]): Instanced {
    const gl = this.gl;
    const vao = gl.createVertexArray() as WebGLVertexArrayObject;
    gl.bindVertexArray(vao);
    const corners = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, corners);
    gl.bufferData(gl.ARRAY_BUFFER, CORNERS, gl.STATIC_DRAW);
    const c = gl.getAttribLocation(prog, 'a_corner');
    gl.enableVertexAttribArray(c);
    gl.vertexAttribPointer(c, 2, gl.FLOAT, false, 0, 0);
    const buffer = gl.createBuffer() as WebGLBuffer;
    gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
    const stride = attrs.reduce((n, [, k]) => n + k, 0) * 4;
    let offset = 0;
    for (const [name, size] of attrs) {
      const loc = gl.getAttribLocation(prog, name);
      gl.enableVertexAttribArray(loc);
      gl.vertexAttribPointer(loc, size, gl.FLOAT, false, stride, offset);
      gl.vertexAttribDivisor(loc, 1);
      offset += size * 4;
    }
    return { vao, buffer, count: 0 };
  }

  setBlocks(blocks: Block[]): void {
    const data = new Float32Array(blocks.length * 8);
    blocks.forEach((b, i) => data.set([b.x, b.y, b.w, b.h, ...b.color], i * 8));
    this.upload(this.blocks, data, blocks.length);
  }

  setLines(lines: Line[]): void {
    const data = new Float32Array(lines.length * 9);
    lines.forEach((l, i) => data.set([l.x1, l.y1, l.x2, l.y2, ...l.color, l.width], i * 9));
    this.upload(this.lines, data, lines.length);
  }

  private upload(target: Instanced, data: Float32Array, count: number): void {
    const gl = this.gl;
    gl.bindBuffer(gl.ARRAY_BUFFER, target.buffer);
    gl.bufferData(gl.ARRAY_BUFFER, data, gl.DYNAMIC_DRAW);
    target.count = count;
  }

  /** Draws on the next animation frame; several asks in one frame draw once. */
  request(camera: () => Camera, showBlocks: () => boolean): void {
    if (this.frame) return;
    this.frame = requestAnimationFrame(() => {
      this.frame = 0;
      this.draw(camera(), showBlocks());
    });
  }

  draw(cam: Camera, showBlocks: boolean): void {
    const gl = this.gl;
    const dpr = window.devicePixelRatio || 1;
    const w = this.canvas.clientWidth;
    const h = this.canvas.clientHeight;
    if (w === 0 || h === 0) return;
    const pw = Math.round(w * dpr);
    const ph = Math.round(h * dpr);
    if (this.canvas.width !== pw || this.canvas.height !== ph) {
      this.canvas.width = pw;
      this.canvas.height = ph;
    }
    gl.viewport(0, 0, pw, ph);
    gl.clearColor(0, 0, 0, 0);
    gl.clear(gl.COLOR_BUFFER_BIT);

    gl.useProgram(this.grid.prog);
    gl.uniform3f(gl.getUniformLocation(this.grid.prog, 'u_cam'), cam.x, cam.y, cam.zoom);
    gl.uniform1f(gl.getUniformLocation(this.grid.prog, 'u_dpr'), dpr);
    gl.uniform1f(gl.getUniformLocation(this.grid.prog, 'u_height'), ph);
    gl.uniform4f(gl.getUniformLocation(this.grid.prog, 'u_color'), ...this.gridColor);
    gl.bindVertexArray(this.grid.vao);
    gl.drawArrays(gl.TRIANGLES, 0, 3);

    for (const [prog, inst, on] of [
      [this.lineProg, this.lines, true],
      [this.blockProg, this.blocks, showBlocks],
    ] as const) {
      if (!on || inst.count === 0) continue;
      gl.useProgram(prog);
      gl.uniform3f(gl.getUniformLocation(prog, 'u_cam'), cam.x, cam.y, cam.zoom);
      gl.uniform2f(gl.getUniformLocation(prog, 'u_size'), w, h);
      gl.bindVertexArray(inst.vao);
      gl.drawArraysInstanced(gl.TRIANGLES, 0, 6, inst.count);
    }
    gl.bindVertexArray(null);
  }

  dispose(): void {
    if (this.frame) cancelAnimationFrame(this.frame);
    this.gl.getExtension('WEBGL_lose_context')?.loseContext();
  }
}

/** A CSS color as the GPU wants it: `#rgb`, `#rrggbb`, `#rrggbbaa` or `rgb(a)(…)`. */
export function parseColor(css: string, alpha = 1): Rgba {
  const s = css.trim();
  let m = /^#([0-9a-f]{3,8})$/i.exec(s);
  if (m) {
    let hex = m[1];
    if (hex.length <= 4) hex = [...hex].map((c) => c + c).join('');
    const n = (i: number) => parseInt(hex.slice(i, i + 2), 16) / 255;
    return [n(0), n(2), n(4), (hex.length === 8 ? n(6) : 1) * alpha];
  }
  m = /^rgba?\(([^)]+)\)$/i.exec(s);
  if (m) {
    const [r, g, b, a] = m[1].split(/[\s,/]+/).filter(Boolean).map(Number);
    return [r / 255, g / 255, b / 255, (Number.isFinite(a) ? a : 1) * alpha];
  }
  return [0.5, 0.5, 0.5, alpha];
}
