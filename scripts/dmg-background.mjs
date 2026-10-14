// Draws the installer window's background from the design tokens with headless Chrome:
// src-tauri/dmg/background.png and background@2x.png, which dmgbuild joins into one image for
// ordinary and Retina screens. Rerun it after moving anything in src-tauri/dmg/settings.py.
import { execFileSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

const root = resolve(import.meta.dirname, '..');
const tokens = JSON.parse(readFileSync(join(root, 'design/tokens.json'), 'utf8')).color.tokens;
const color = (name) => tokens.find((token) => token.name === name).value.light;

// Chrome won't load a font file from a page opened as a file, so the fonts go in as data.
const font = (weight) => {
  const data = readFileSync(join(root, `src/fonts/Barlow-${weight}.woff2`)).toString('base64');
  return `@font-face{font-family:Barlow;font-weight:${weight};src:url(data:font/woff2;base64,${data}) format('woff2')}`;
};

// The window is 660 by 400 points with its title bar, so about 368 show below the bar (macOS 26;
// a little more before it), and the picture's last 32 points are never seen. settings.py centers
// the app's icon at (180, 156) and the Applications folder at (480, 156), both 128 points
// across, and Finder writes their names below them.
const [width, height] = [660, 400];
const page = `<!doctype html><html><head><meta charset="utf-8"><style>${font(500)}${font(600)}
html,body{margin:0;width:${width}px;height:${height}px;overflow:hidden;background:${color('ground')}}
svg{display:block}</style></head><body>
<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}">
<g fill="none" stroke="${color('line-strong')}" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
<path d="M278 156H382"/><path d="M370 144l13 12-13 12"/></g>
<text x="330" y="300" text-anchor="middle" font-family="Barlow" font-weight="600" font-size="17" fill="${color('ink')}">Drag Codealong into Applications</text>
<text x="330" y="324" text-anchor="middle" font-family="Barlow" font-weight="500" font-size="14" fill="${color('ink-3')}">Then open it from your Applications folder.</text>
</svg></body></html>`;

const out = join(root, 'src-tauri/dmg');
mkdirSync(out, { recursive: true });
const dir = mkdtempSync(join(tmpdir(), 'codealong-dmg-'));
try {
  const html = join(dir, 'background.html');
  writeFileSync(html, page);
  for (const [scale, name] of [[1, 'background.png'], [2, 'background@2x.png']]) {
    // Its profile goes with `dir`: given none, headless Chrome leaves one in Application Support.
    execFileSync('/Applications/Google Chrome.app/Contents/MacOS/Google Chrome', [
      '--headless=new', '--disable-gpu', '--hide-scrollbars', `--force-device-scale-factor=${scale}`, `--user-data-dir=${join(dir, 'profile')}`,
      `--window-size=${width},${height}`, '--virtual-time-budget=3000', `--screenshot=${join(out, name)}`, `file://${html}`,
    ], { stdio: 'ignore' });
  }
} finally {
  rmSync(dir, { recursive: true, force: true });
}
