// The characters a terminal draws itself rather than from the font, as Ghostty does: box drawing,
// block elements, braille and the powerline separators. A font's own leave gaps between rows
// (their lines stop at the font's height, not the cell's) and rarely meet their neighbours, so
// tables and prompts drawn with them come apart. Each is drawn here to fill its cell exactly.

type Ctx = CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D;

/** Each box-drawing character's arms, up, right, down, left: 0 none, 1 light, 2 heavy, 3 double. */
const BOX: Record<number, string> = {
  0x2500: '0101', 0x2501: '0202', 0x2502: '1010', 0x2503: '2020',
  0x250c: '0110', 0x250d: '0210', 0x250e: '0120', 0x250f: '0220',
  0x2510: '0011', 0x2511: '0012', 0x2512: '0021', 0x2513: '0022',
  0x2514: '1100', 0x2515: '1200', 0x2516: '2100', 0x2517: '2200',
  0x2518: '1001', 0x2519: '1002', 0x251a: '2001', 0x251b: '2002',
  0x251c: '1110', 0x251d: '1210', 0x251e: '2110', 0x251f: '1120', 0x2520: '2120', 0x2521: '2210', 0x2522: '1220', 0x2523: '2220',
  0x2524: '1011', 0x2525: '1012', 0x2526: '2011', 0x2527: '1021', 0x2528: '2021', 0x2529: '2012', 0x252a: '1022', 0x252b: '2022',
  0x252c: '0111', 0x252d: '0112', 0x252e: '0211', 0x252f: '0212', 0x2530: '0121', 0x2531: '0122', 0x2532: '0221', 0x2533: '0222',
  0x2534: '1101', 0x2535: '1102', 0x2536: '1201', 0x2537: '1202', 0x2538: '2101', 0x2539: '2102', 0x253a: '2201', 0x253b: '2202',
  0x253c: '1111', 0x253d: '1112', 0x253e: '1211', 0x253f: '1212', 0x2540: '2111', 0x2541: '1121', 0x2542: '2121', 0x2543: '2112',
  0x2544: '2211', 0x2545: '1122', 0x2546: '1221', 0x2547: '2212', 0x2548: '1222', 0x2549: '2122', 0x254a: '2221', 0x254b: '2222',
  0x2550: '0303', 0x2551: '3030', 0x2552: '0310', 0x2553: '0130', 0x2554: '0330', 0x2555: '0013', 0x2556: '0031', 0x2557: '0033',
  0x2558: '1300', 0x2559: '3100', 0x255a: '3300', 0x255b: '1003', 0x255c: '3001', 0x255d: '3003',
  0x255e: '1310', 0x255f: '3130', 0x2560: '3330', 0x2561: '1013', 0x2562: '3031', 0x2563: '3033',
  0x2564: '0313', 0x2565: '0131', 0x2566: '0333', 0x2567: '1303', 0x2568: '3101', 0x2569: '3303', 0x256a: '1313', 0x256b: '3131', 0x256c: '3333',
  0x2574: '0001', 0x2575: '1000', 0x2576: '0100', 0x2577: '0010', 0x2578: '0002', 0x2579: '2000', 0x257a: '0200', 0x257b: '0020',
  0x257c: '0201', 0x257d: '1020', 0x257e: '0102', 0x257f: '2010',
};

/** Dashed lines: how many dashes, heavy, vertical. */
const DASHES: Record<number, [number, boolean, boolean]> = {
  0x2504: [3, false, false], 0x2505: [3, true, false], 0x2506: [3, false, true], 0x2507: [3, true, true],
  0x2508: [4, false, false], 0x2509: [4, true, false], 0x250a: [4, false, true], 0x250b: [4, true, true],
  0x254c: [2, false, false], 0x254d: [2, true, false], 0x254e: [2, false, true], 0x254f: [2, true, true],
};

/** Whether the terminal draws `text` itself. */
export function isShape(text: string): boolean {
  if (text.length !== 1) return false;
  const c = text.charCodeAt(0);
  return (c >= 0x2500 && c <= 0x259f) || (c >= 0x2800 && c <= 0x28ff) || (c >= 0xe0b0 && c <= 0xe0bf);
}

/** Draws `text` filling the cell at (x, y), `w` by `h` device pixels, in the context's fill style.
 *  False for a character it doesn't draw. */
export function drawShape(ctx: Ctx, text: string, x: number, y: number, w: number, h: number): boolean {
  if (!isShape(text)) return false;
  const c = text.charCodeAt(0);
  const light = Math.max(1, Math.round(h / 16));
  const heavy = light * 2;
  const thick = (k: number) => (k === 2 ? heavy : light);
  const rect = (rx: number, ry: number, rw: number, rh: number) => ctx.fillRect(Math.round(rx), Math.round(ry), Math.max(1, Math.round(rw)), Math.max(1, Math.round(rh)));
  const cx = x + w / 2;
  const cy = y + h / 2;
  const arms = BOX[c];
  if (arms) {
    const [up, right, down, left] = [...arms].map(Number);
    // A double line is two light ones, `gap` apart either side of the middle.
    const gap = light * 1.5;
    const vertical = (k: number, from: number, to: number) => {
      if (k === 3) {
        rect(cx - gap - light / 2, from, light, to - from);
        rect(cx + gap - light / 2, from, light, to - from);
      } else rect(cx - thick(k) / 2, from, thick(k), to - from);
    };
    const horizontal = (k: number, from: number, to: number) => {
      if (k === 3) {
        rect(from, cy - gap - light / 2, to - from, light);
        rect(from, cy + gap - light / 2, to - from, light);
      } else rect(from, cy - thick(k) / 2, to - from, thick(k));
    };
    // Each arm reaches past the middle by half the widest crossing line, so corners meet square.
    const reach = Math.max(...[up, right, down, left].map((k) => (k === 3 ? gap + light / 2 : k ? thick(k) / 2 : 0)));
    if (up) vertical(up, y, cy + reach);
    if (down) vertical(down, cy - reach, y + h);
    if (left) horizontal(left, x, cx + reach);
    if (right) horizontal(right, cx - reach, x + w);
    return true;
  }
  const dash = DASHES[c];
  if (dash) {
    const [n, isHeavy, isVertical] = dash;
    const t = isHeavy ? heavy : light;
    const span = isVertical ? h : w;
    const step = span / n;
    for (let i = 0; i < n; i++) {
      const at = (isVertical ? y : x) + i * step + step * 0.15;
      if (isVertical) rect(cx - t / 2, at, t, step * 0.7);
      else rect(at, cy - t / 2, step * 0.7, t);
    }
    return true;
  }
  if (c >= 0x256d && c <= 0x2570) {
    // Rounded corners: a quarter circle from the middle of one edge to the middle of another.
    ctx.save();
    ctx.lineWidth = light;
    ctx.strokeStyle = ctx.fillStyle;
    ctx.beginPath();
    const r = Math.min(w, h) / 2;
    if (c === 0x256d) arc(ctx, cx, y + h, cx, cy + r, cx + r, cy, x + w, cy);
    if (c === 0x256e) arc(ctx, cx, y + h, cx, cy + r, cx - r, cy, x, cy);
    if (c === 0x256f) arc(ctx, cx, y, cx, cy - r, cx - r, cy, x, cy);
    if (c === 0x2570) arc(ctx, cx, y, cx, cy - r, cx + r, cy, x + w, cy);
    ctx.stroke();
    ctx.restore();
    return true;
  }
  if (c >= 0x2571 && c <= 0x2573) {
    ctx.save();
    ctx.lineWidth = light;
    ctx.strokeStyle = ctx.fillStyle;
    ctx.beginPath();
    if (c !== 0x2572) {
      ctx.moveTo(x, y + h);
      ctx.lineTo(x + w, y);
    }
    if (c !== 0x2571) {
      ctx.moveTo(x, y);
      ctx.lineTo(x + w, y + h);
    }
    ctx.stroke();
    ctx.restore();
    return true;
  }
  if (c >= 0x2580 && c <= 0x259f) {
    block(ctx, c, x, y, w, h, rect);
    return true;
  }
  if (c >= 0x2800 && c <= 0x28ff) {
    braille(c - 0x2800, x, y, w, h, rect);
    return true;
  }
  return powerline(ctx, c, x, y, w, h, light);
}

/** A rounded corner: from the edge at (x0, y0) straight to (x1, y1), round the cell's middle
 *  (x1, y2) to (x2, y2), and straight on to the edge at (x3, y3). */
function arc(ctx: Ctx, x0: number, y0: number, x1: number, y1: number, x2: number, y2: number, x3: number, y3: number) {
  ctx.moveTo(x0, y0);
  ctx.lineTo(x1, y1);
  ctx.quadraticCurveTo(x1, y2, x2, y2);
  ctx.lineTo(x3, y3);
}

type Rect = (x: number, y: number, w: number, h: number) => void;

function block(ctx: Ctx, c: number, x: number, y: number, w: number, h: number, rect: Rect) {
  const eighthH = h / 8;
  const eighthW = w / 8;
  if (c === 0x2580) return rect(x, y, w, h / 2);
  if (c >= 0x2581 && c <= 0x2588) {
    const n = c - 0x2580;
    return rect(x, y + h - n * eighthH, w, n * eighthH);
  }
  if (c >= 0x2589 && c <= 0x258f) {
    const n = 0x2590 - c;
    return rect(x, y, n * eighthW, h);
  }
  if (c === 0x2590) return rect(x + w / 2, y, w / 2, h);
  if (c >= 0x2591 && c <= 0x2593) {
    // Shades: the cell filled at a quarter, a half and three quarters.
    ctx.save();
    ctx.globalAlpha = (c - 0x2590) * 0.25;
    rect(x, y, w, h);
    ctx.restore();
    return;
  }
  if (c === 0x2594) return rect(x, y, w, eighthH);
  if (c === 0x2595) return rect(x + w - eighthW, y, eighthW, h);
  // Quadrants: upper left, upper right, lower left, lower right.
  const quads: Record<number, [boolean, boolean, boolean, boolean]> = {
    0x2596: [false, false, true, false],
    0x2597: [false, false, false, true],
    0x2598: [true, false, false, false],
    0x2599: [true, false, true, true],
    0x259a: [true, false, false, true],
    0x259b: [true, true, true, false],
    0x259c: [true, true, false, true],
    0x259d: [false, true, false, false],
    0x259e: [false, true, true, false],
    0x259f: [false, true, true, true],
  };
  const q = quads[c];
  if (!q) return;
  const [ul, ur, ll, lr] = q;
  if (ul) rect(x, y, w / 2, h / 2);
  if (ur) rect(x + w / 2, y, w / 2, h / 2);
  if (ll) rect(x, y + h / 2, w / 2, h / 2);
  if (lr) rect(x + w / 2, y + h / 2, w / 2, h / 2);
}

/** Braille's eight dots, by bit: down the left column, down the right, then the bottom row. */
const DOTS: [number, number][] = [
  [0, 0],
  [0, 1],
  [0, 2],
  [1, 0],
  [1, 1],
  [1, 2],
  [0, 3],
  [1, 3],
];

function braille(bits: number, x: number, y: number, w: number, h: number, rect: Rect) {
  const dot = Math.max(1, Math.round(Math.min(w / 4, h / 8)));
  DOTS.forEach(([col, row], bit) => {
    if (!(bits & (1 << bit))) return;
    const dx = x + (w / 4) * (col * 2 + 1) - dot / 2;
    const dy = y + (h / 8) * (row * 2 + 1) - dot / 2;
    rect(dx, dy, dot, dot);
  });
}

function powerline(ctx: Ctx, c: number, x: number, y: number, w: number, h: number, light: number): boolean {
  ctx.save();
  ctx.beginPath();
  const fillOrStroke = (solid: boolean) => {
    if (solid) ctx.fill();
    else {
      ctx.lineWidth = light;
      ctx.strokeStyle = ctx.fillStyle;
      ctx.stroke();
    }
  };
  let drawn = true;
  switch (c) {
    case 0xe0b0:
    case 0xe0b1:
      ctx.moveTo(x, y);
      ctx.lineTo(x + w, y + h / 2);
      ctx.lineTo(x, y + h);
      if (c === 0xe0b0) ctx.closePath();
      fillOrStroke(c === 0xe0b0);
      break;
    case 0xe0b2:
    case 0xe0b3:
      ctx.moveTo(x + w, y);
      ctx.lineTo(x, y + h / 2);
      ctx.lineTo(x + w, y + h);
      if (c === 0xe0b2) ctx.closePath();
      fillOrStroke(c === 0xe0b2);
      break;
    case 0xe0b4:
    case 0xe0b5:
      ctx.ellipse(x, y + h / 2, w, h / 2, 0, -Math.PI / 2, Math.PI / 2);
      fillOrStroke(c === 0xe0b4);
      break;
    case 0xe0b6:
    case 0xe0b7:
      ctx.ellipse(x + w, y + h / 2, w, h / 2, 0, Math.PI / 2, (3 * Math.PI) / 2);
      fillOrStroke(c === 0xe0b6);
      break;
    case 0xe0b8:
      ctx.moveTo(x, y);
      ctx.lineTo(x + w, y + h);
      ctx.lineTo(x, y + h);
      ctx.closePath();
      ctx.fill();
      break;
    case 0xe0ba:
      ctx.moveTo(x + w, y);
      ctx.lineTo(x + w, y + h);
      ctx.lineTo(x, y + h);
      ctx.closePath();
      ctx.fill();
      break;
    case 0xe0bc:
      ctx.moveTo(x, y);
      ctx.lineTo(x + w, y);
      ctx.lineTo(x, y + h);
      ctx.closePath();
      ctx.fill();
      break;
    case 0xe0be:
      ctx.moveTo(x, y);
      ctx.lineTo(x + w, y);
      ctx.lineTo(x + w, y + h);
      ctx.closePath();
      ctx.fill();
      break;
    default:
      // The thin diagonals (E0B9, E0BB, E0BD, E0BF) and anything else: the font's.
      drawn = false;
  }
  ctx.restore();
  return drawn;
}
