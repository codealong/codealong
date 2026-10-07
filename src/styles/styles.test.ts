// The stylesheets stay in order: each class has one home, nothing is styled that nothing renders,
// every file is in the index once, tokens.css is what its generator writes, a value a token
// holds is written as the token, and every font is a type token or a special listed below.
//
// Ownership, which the first test holds. Only `ca-` names are things; other class names (is-*,
// has-*) are states of a thing. A rule defines a class when it names no state and the class is
// the only thing it names, or the only one that isn't shared (`.ca-chip.ca-todorow-blocked`
// defines .ca-todorow-blocked); a shared class is one a file under src/styles/ defines. A class
// is defined in one file at most: two files defining one name are two components named one thing
// (the .ca-prcard clash of 2026-09-30). Its home is that file, or for a class never defined, the
// first file in the index that names it. Every rule in a component's file (src/ui/**/*.css,
// src/App.css) names a class at home there, or a state no other file names: so
// `.ca-peek .ca-thread { … }` lives in Peek.css, and a rule for `.ca-thread`, alone or grouped
// only with other files' classes, lives in SessionThread.css. The shared files may name any
// class: what several components share is what they're for.
// The project has no types for node (the tests are the only code that runs there).
// @ts-expect-error no @types/node
import { readdirSync, readFileSync, statSync } from 'node:fs';
// @ts-expect-error no @types/node
import { dirname, join, relative, resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import { tokensCss } from '../../scripts/tokens.mjs';

const root: string = resolve(new URL('../..', import.meta.url).pathname);
const src = join(root, 'src');
const index = join(src, 'styles/app.css');

// Class names built at run time, which no source file spells whole: each name, or prefix with a
// `*`, with where it's built.
const BUILT: Record<string, string> = {
  'ca-pr--*': "a PR's state (primitives.tsx PrChip, PrsView.tsx, PrLink.tsx, card/CardFace.tsx, card/CardPopover.tsx), and `${base}--button` for one that acts (primitives.tsx Chip)",
  'ca-prpill--*': 'a pill’s key (PrPills.tsx)',
  'ca-glyph--*': 'a status (primitives.tsx StatusGlyph)',
  'ca-bay--*': 'a bay (primitives.tsx Bay)',
  'ca-md-h*': 'a heading’s level (Markdown.tsx)',
  'ca-chip--button': 'a chip that acts, `${base}--button` (primitives.tsx Chip)',
  'ca-labelchip--button': 'a label chip that acts, `${base}--button` (primitives.tsx Chip)',
};

function walk(dir: string, keep: (f: string) => boolean, out: string[] = []): string[] {
  for (const f of readdirSync(dir)) {
    const p = join(dir, f);
    if (statSync(p).isDirectory()) walk(p, keep, out);
    else if (keep(p)) out.push(p);
  }
  return out;
}
const read = (p: string) => readFileSync(p, 'utf8');
const short = (p: string) => relative(root, p);

/** A stylesheet's style rules: each one's selectors and line, inside @media and its kin. */
interface Rule {
  selectors: string[];
  line: number;
}
function rules(css: string): Rule[] {
  // Comments become spaces, so lines still count.
  const text = css.replace(/\/\*[\s\S]*?\*\//g, (c) => c.replace(/[^\n]/g, ' '));
  const out: Rule[] = [];
  // What each open block is: a group of rules (@media…) or anything else (declarations,
  // @keyframes, @font-face), whose insides aren't rules.
  const open: ('group' | 'other')[] = [];
  let from = 0;
  let quote = '';
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (quote) {
      if (c === '\\') i++;
      else if (c === quote) quote = '';
    } else if (c === '"' || c === "'") quote = c;
    else if (c === '{') {
      const prelude = text.slice(from, i);
      const head = prelude.trim();
      const inRules = open.every((k) => k === 'group');
      if (inRules && !head.startsWith('@')) {
        const line = text.slice(0, from + prelude.length - prelude.trimStart().length).split('\n').length;
        out.push({ selectors: splitTop(head), line });
      }
      open.push(inRules && /^@(media|supports|container|layer)\b/.test(head) ? 'group' : 'other');
      from = i + 1;
    } else if (c === '}') {
      open.pop();
      from = i + 1;
    } else if (c === ';' && open.every((k) => k === 'group')) from = i + 1;
  }
  return out;
}
/** A selector list split at the commas outside brackets. */
function splitTop(list: string): string[] {
  const out: string[] = [];
  let depth = 0;
  let from = 0;
  for (let i = 0; i < list.length; i++) {
    if (list[i] === '(' || list[i] === '[') depth++;
    else if (list[i] === ')' || list[i] === ']') depth--;
    else if (list[i] === ',' && depth === 0) {
      out.push(list.slice(from, i).trim());
      from = i + 1;
    }
  }
  out.push(list.slice(from).trim());
  return out.filter(Boolean);
}
const classesIn = (selectors: string[]) => [...new Set(selectors.flatMap((s) => [...s.matchAll(/\.(-?[_a-zA-Z][\w-]*)/g)].map((m) => m[1])))];

const imported = [...read(index).matchAll(/@import\s+'([^']+)';/g)].map((m) => resolve(dirname(index), m[1]));
const sheets = walk(src, (f) => f.endsWith('.css') && f !== index).sort();
// In the index's order, which is the order "first names it" goes by.
const parsed = imported.filter((f) => sheets.includes(f)).map((file) => ({ file, rules: rules(read(file)) }));
const isShared = (file: string) => dirname(file) === join(src, 'styles');

describe('the stylesheets', () => {
  it('give each class one home, and style another file’s classes only beside their own', () => {
    const things = (r: Rule) => classesIn(r.selectors).filter((c) => c.startsWith('ca-'));
    const states = (r: Rule) => classesIn(r.selectors).filter((c) => !c.startsWith('ca-'));
    const shared = new Set<string>();
    for (const { file, rules } of parsed) if (isShared(file)) for (const r of rules) if (things(r).length === 1 && states(r).length === 0) shared.add(things(r)[0]);
    const defines = (r: Rule): string | null => {
      if (states(r).length) return null;
      const all = things(r);
      if (all.length === 1) return all[0];
      const own = all.filter((c) => !shared.has(c));
      return own.length === 1 ? own[0] : null;
    };
    const defined = new Map<string, Map<string, number>>();
    const firstNamed = new Map<string, string>();
    const stateFiles = new Map<string, Set<string>>();
    for (const { file, rules } of parsed) {
      for (const r of rules) {
        for (const c of things(r)) if (!firstNamed.has(c)) firstNamed.set(c, file);
        for (const s of states(r)) (stateFiles.get(s) ?? stateFiles.set(s, new Set()).get(s)!).add(file);
        const c = defines(r);
        if (!c) continue;
        const at = defined.get(c) ?? defined.set(c, new Map()).get(c)!;
        if (!at.has(file)) at.set(file, r.line);
      }
    }
    const problems: string[] = [];
    const home = new Map<string, string>();
    const clashing = new Set<string>();
    for (const [c, first] of firstNamed) {
      const at = defined.get(c);
      if (at && at.size > 1) {
        clashing.add(c);
        problems.push(
          `.${c} is defined in ${[...at].map(([f, line]) => `${short(f)}:${line}`).join(' and ')}: two components named one thing. Rename the newer one (scripts/preview/*.js select by names too), or if they're one thing, keep its rules in one file.`,
        );
      }
      home.set(c, at ? [...at.keys()][0] : first);
    }
    for (const { file, rules } of parsed) {
      if (isShared(file)) continue;
      for (const r of rules) {
        const named = things(r);
        if (!named.length || named.some((c) => home.get(c) === file || clashing.has(c)) || states(r).some((s) => stateFiles.get(s)!.size === 1)) continue;
        problems.push(
          `${short(file)}:${r.line} styles ${r.selectors.join(', ')}, which names only other files' classes: ${named.map((c) => `.${c} (${short(home.get(c)!)})`).join(', ')}. Move the rule to where the class lives, or qualify it with one of this file's own classes (\`.ca-peek .ca-thread\`) if it's this component's override.`,
        );
      }
    }
    expect(problems, problems.join('\n\n')).toEqual([]);
  });

  it('define no class that nothing renders', () => {
    const code = walk(src, (f) => /\.tsx?$/.test(f) && !/\.test\.tsx?$/.test(f) && !f.endsWith('.d.ts'))
      .concat([join(root, 'index.html')])
      .map(read)
      .join('\n');
    // A class is used where the code spells it whole.
    const words = new Set(code.match(/[\w-]+/g));
    const problems: string[] = [];
    const needed = new Set<string>();
    for (const { file, rules } of parsed) {
      const seen = new Set<string>();
      for (const r of rules) {
        for (const c of classesIn(r.selectors)) {
          if (!c.startsWith('ca-') || seen.has(c) || words.has(c)) continue;
          seen.add(c);
          const b = Object.keys(BUILT).find((k) => (k.endsWith('*') ? c.startsWith(k.slice(0, -1)) : c === k));
          if (b) needed.add(b);
          else problems.push(`.${c} (${short(file)}:${r.line})`);
        }
      }
    }
    // The list stays true: each entry names a class that's styled and that src/ doesn't spell.
    for (const [b, where] of Object.entries(BUILT)) if (!needed.has(b)) problems.push(`BUILT lists ${b} (${where}), but no class styled needs it: take it off the list.`);
    expect(
      problems,
      `Nothing in src/ or index.html spells these classes, so their rules do nothing: delete them. A name built at run time (\`ca-pr--\${state}\`) goes in BUILT at the top of this file, with where it's built.\n\n${problems.join('\n')}`,
    ).toEqual([]);
  });

  it('are each imported by the index once, and nowhere else', () => {
    const counts = new Map<string, number>();
    for (const f of imported) counts.set(f, (counts.get(f) ?? 0) + 1);
    const problems: string[] = [];
    for (const f of sheets) {
      const n = counts.get(f) ?? 0;
      if (n !== 1) problems.push(`${short(f)} is imported ${n === 0 ? 'nowhere' : `${n} times`}: src/styles/app.css imports every stylesheet once, at its place in the cascade (see its comment).`);
    }
    for (const f of imported) if (!sheets.includes(f)) problems.push(`src/styles/app.css imports ${short(f)}, which doesn't exist.`);
    if (imported[0] !== join(src, 'styles/tokens.css')) problems.push('src/styles/app.css imports tokens.css first: every other file reads its variables and fonts.');
    for (const f of sheets) if (/@import\b/.test(read(f))) problems.push(`${short(f)} has an @import: only src/styles/app.css imports, so the cascade's order is in one place.`);
    // A stylesheet imported from code would join the sheet in the order modules load.
    for (const f of walk(src, (p) => /\.tsx?$/.test(p) && !/\.test\.tsx?$/.test(p))) {
      const css = [...read(f).matchAll(/import\s+(?:[^'";]+\s+from\s+)?'([^']+\.css)'/g)].map((m) => m[1]);
      const want = f === join(src, 'main.tsx') ? ['./styles/app.css'] : [];
      if (css.join(' ') !== want.join(' ')) problems.push(`${short(f)} imports ${css.join(', ') || 'no stylesheet'}: only src/main.tsx imports one, ./styles/app.css; a stylesheet goes in that index.`);
    }
    expect(problems, problems.join('\n')).toEqual([]);
  });

  // Only where every such value already uses its token: a gap of one length that is a spacing
  // step. Values between the steps, and padding and margins with one, stay as they are for a
  // designer to decide.
  it('write a spacing step as its token', () => {
    const tokens = JSON.parse(read(join(root, 'design/tokens.json'))) as { spacing: { tokens: { name: string; value: string }[] } };
    const space = new Map(tokens.spacing.tokens.map((t) => [t.value, `--${t.name}`]));
    const problems: string[] = [];
    for (const file of sheets) {
      const text = read(file).replace(/\/\*[\s\S]*?\*\//g, (c: string) => c.replace(/[^\n]/g, ' '));
      for (const m of text.matchAll(/([{;]\s*)(gap|row-gap|column-gap)\s*:\s*([^;{}]*?)\s*(?:!important\s*)?(?=[;}])/g)) {
        const value = m[3].replace(/\s+/g, ' ');
        const token = space.get(value);
        if (!token) continue;
        const line = text.slice(0, m.index + m[1].length).split('\n').length;
        problems.push(`${short(file)}:${line} ${m[2]}: ${value} is ${token} in design/tokens.json: write ${m[2]}: var(${token}).`);
      }
    }
    expect(problems, problems.join('\n')).toEqual([]);
  });

  // Every font shorthand is a style of the type scale (font: var(--type-meta)), inherit, or one of
  // these, each a place whose numbers do a job no style of the scale does: a line height that is
  // its own box's height, so the glyph sits in the middle of a keycap or a badge, or a size or a
  // line set for one place. A rule is listed by its selector as written; a new style goes in
  // design/tokens.json (`npm run tokens`), named for what it's for, not here.
  const SPECIAL_FONTS: Record<string, { font: string; why: string }> = {
    '.ca-kbd': { font: '500 11px/1 var(--font-sans)', why: 'a keycap: on a line of 1, so its 18px box centers the key' },
    '.ca-kbd.is-mark': { font: '700 15px/1 var(--font-mono)', why: 'a keycap that shows a mark (⌘, ⏎) rather than a word, larger and heavier so the mark reads' },
    '.ca-hotkey kbd': { font: '500 10px/1 var(--font-sans)', why: "a hotkey's key, centered in its 16px box as a keycap is in its own" },
    '.ca-nav-badge': { font: '600 10px/16px var(--font-mono)', why: "a count in a round 16px badge, on the badge's height" },
    '.ca-projswitch-badge': { font: '600 10px/16px var(--font-mono)', why: "a count in a round 16px badge, on the badge's height (as .ca-nav-badge)" },
    '.ca-rail-badge': { font: '600 9px/14px var(--font-mono)', why: "a count in the rail's 14px badge, on the badge's height, over an icon" },
    '.ca-modelmenu-said': { font: '500 12px/26px var(--font-sans)', why: "a model that can't be switched from here, said on the 26px line of the chip it stands in for" },
    '.ca-meter-value': { font: '600 24px/24px var(--font-display)', why: "a usage meter's figure: numeral's face at the size the meter has room for" },
    '.ca-cv-callsign': { font: '600 15px/18px var(--font-display)', why: "a card's callsign: callsign's face on the card's 18px first line, smaller than a lane's" },
    '.ca-cv-farlabel': { font: '600 12px/16px var(--font-display)', why: "a card's name over a canvas zoomed far out, scaled with the zoom" },
    '.ca-cv-note': { font: '400 14px/20px var(--font-sans)', why: 'a note on the canvas, written larger than the UI around it so it still reads zoomed out' },
    '.ca-settings-editor': { font: '400 12px/18px var(--font-mono)', why: "config.toml in Settings: code's size, with room between the lines of a whole file you edit" },
    '.ca-ucard-name': { font: '600 14px/18px var(--font-mono)', why: "the usage card's heading is an account's name: heading's size, in mono as account names are set" },
    '.ca-quick-input textarea': { font: '400 17px/22px var(--font-sans)', why: "the quick panel's prompt, large as a launcher's field is" },
  };
  it('write every font as a type token, inherit, or a special listed here', () => {
    const tokens = JSON.parse(read(join(root, 'design/tokens.json'))) as {
      type: { groups: { family: string; styles: { name: string; fontSize: string; lineHeight: string; fontWeight: number }[] }[] };
    };
    const styles = tokens.type.groups.flatMap((g) => g.styles.map((t) => ({ token: `--type-${t.name}`, family: g.family, weight: t.fontWeight, size: parseFloat(t.fontSize), line: parseFloat(t.lineHeight), value: `${t.fontWeight} ${t.fontSize}/${t.lineHeight} var(--font-${g.family})` })));
    // The style a hand-written font is closest to: its family's, nearest in size, then line
    // height, then weight.
    const nearest = (value: string) => {
      const m = /^(\d+) ([\d.]+)px\/([\d.]+)(px)? var\(--font-(\w+)\)$/.exec(value);
      if (!m) return null;
      const [weight, size, line, family] = [Number(m[1]), parseFloat(m[2]), parseFloat(m[3]) * (m[4] ? 1 : parseFloat(m[2])), m[5]];
      const same = styles.filter((s) => s.family === family);
      const score = (s: (typeof styles)[number]) => Math.abs(s.size - size) * 100 + Math.abs(s.line - line) * 10 + Math.abs(s.weight - weight) / 100;
      return (same.length ? same : styles).reduce((a, b) => (score(b) < score(a) ? b : a));
    };
    const problems: string[] = [];
    const used = new Set<string>();
    for (const file of sheets) {
      if (file === join(src, 'styles/tokens.css')) continue;
      const text = read(file).replace(/\/\*[\s\S]*?\*\//g, (c: string) => c.replace(/[^\n]/g, ' '));
      for (const m of text.matchAll(/([{;]\s*)font\s*:\s*([^;{}]*?)\s*(?:!important\s*)?(?=[;}])/g)) {
        const value = m[2].replace(/\s+/g, ' ');
        const at = m.index + m[1].length;
        const line = text.slice(0, at).split('\n').length;
        const open = text.lastIndexOf('{', at);
        const selector = text
          .slice(Math.max(text.lastIndexOf('}', open), text.lastIndexOf(';', open), text.lastIndexOf('{', open - 1)) + 1, open)
          .trim()
          .replace(/\s+/g, ' ');
        const where = `${short(file)}:${line} ${selector} { font: ${value} }`;
        const token = /^var\((--type-[\w-]+)\)$/.exec(value);
        if (token) {
          if (!styles.some((s) => s.token === token[1])) problems.push(`${where}: ${token[1]} is no style in design/tokens.json.`);
          continue;
        }
        if (value === 'inherit') continue;
        const special = SPECIAL_FONTS[selector];
        if (special && special.font === value) {
          used.add(selector);
          continue;
        }
        const exact = styles.find((s) => s.value === value);
        const near = nearest(value);
        problems.push(
          exact
            ? `${where}: that's ${exact.token}: write font: var(${exact.token}).`
            : `${where} is written by hand. Write the type style it's meant to be${near ? ` (the nearest is ${near.token}, ${near.value})` : ''}, or if it's a style of its own, add it to design/tokens.json named for what it's for and run \`npm run tokens\`. A font whose numbers do a job no style can (a line height that is its box's height) goes in SPECIAL_FONTS in this file, with why.`,
        );
      }
    }
    // The list stays true: each entry is a rule that still writes that font.
    for (const [selector, { font }] of Object.entries(SPECIAL_FONTS)) if (!used.has(selector)) problems.push(`SPECIAL_FONTS lists ${selector} { font: ${font} }, which no stylesheet writes: take it off the list.`);
    expect(problems, problems.join('\n\n')).toEqual([]);
  });

  it('have tokens.css as scripts/tokens.mjs writes it', () => {
    const tokens: unknown = JSON.parse(read(join(root, 'design/tokens.json')));
    expect(read(join(src, 'styles/tokens.css')), 'src/styles/tokens.css differs from what design/tokens.json gives: run `npm run tokens` (and edit tokens.json, never tokens.css).').toBe(tokensCss(tokens).css);
  });
});
