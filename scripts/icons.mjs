// Renders design/app-icon.svg to src-tauri/app-icon.png with headless Chrome, then runs
// `tauri icon` to produce every size the bundle needs.
import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

const root = resolve(import.meta.dirname, '..');
const svg = readFileSync(join(root, 'design/app-icon.svg'), 'utf8');
const dir = mkdtempSync(join(tmpdir(), 'codealong-icon-'));
const html = join(dir, 'icon.html');
writeFileSync(html, `<!doctype html><html><body style="margin:0;background:transparent">${svg.replace('<svg ', '<svg style="display:block;width:1024px;height:1024px" ')}</body></html>`);
const png = join(root, 'src-tauri/app-icon.png');
execFileSync('/Applications/Google Chrome.app/Contents/MacOS/Google Chrome', [
  '--headless=new', '--disable-gpu', '--hide-scrollbars', '--force-device-scale-factor=1',
  '--default-background-color=00000000', '--window-size=1024,1024', `--screenshot=${png}`, `file://${html}`,
], { stdio: 'ignore' });
execFileSync('npx', ['tauri', 'icon', png], { cwd: root, stdio: 'inherit' });

// Keep only what the macOS (and a future Windows) bundle uses.
import { rmSync, readdirSync } from 'node:fs';
const icons = join(root, 'src-tauri/icons');
for (const name of readdirSync(icons)) {
  if (name === 'android' || name === 'ios' || name.startsWith('Square') || name === 'StoreLogo.png') {
    rmSync(join(icons, name), { recursive: true, force: true });
  }
}
