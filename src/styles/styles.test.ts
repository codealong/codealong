// The stylesheets stay in order: each class has one home, nothing is styled that nothing renders,
// every file is in the index once, tokens.css is what its generator writes, a value a token
// holds is written as the token, and every font is a type token or a special listed below, with
// no size, line height or weight written around it but the ones listed.
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

  // A font size, line height or weight written on its own changes the type style its element
  // has, the one its rule gives (font: var(--type-meta); font-weight: 600) or the one it inherits,
  // into another: a style written by hand. Where that's another style of the scale, the test
  // names it, to write in their place. Where it's a style no token has, it's a new style, to add
  // to design/tokens.json when places share it, or to list here when its numbers do a job only
  // that place has: a line that sits it level with something, a size relative to the text around
  // it, a size on the line of the row it's in where a style's own line would move the row. A rule
  // is listed by its selector as written, with its longhands as written, in order.
  const SPECIAL_LONGHANDS: Record<string, { set: string; why: string }> = {
    '.ca-ic': { set: 'font-size: 0.9em', why: 'inline code in running text: nine tenths of the text it sits in, whatever its style, on that text’s line' },
    '.ca-code': { set: 'font-size: 11px', why: 'code in a line of text (a session’s id, a path in a key-value list): mono at 11px on the line of the text around it, meta’s or body’s' },
    '.ca-found-home': { set: 'font-size: 12px', why: 'a found account’s home folder: code’s size in place of .ca-code’s 11px (it’s both), on its row’s 18px line' },
    '.ca-nav.is-child': { set: 'font-size: 12px', why: 'a child row in the sidebar (NavItem `child`, which no view passes today): mono at code’s size on the row’s 18px line' },
    '.ca-tree-age': { set: 'font-size: 11px', why: 'a session’s age at the end of its tree row: caption’s size on the row’s 18px line, level with the name before it' },
    '.ca-projswitch-key': { set: 'font-size: 12px', why: 'a project’s ⌥⌘ digit, like a native menu’s shortcut column: meta’s size on the menu row’s 18px line' },
    '.ca-prcard-failing': { set: 'font-size: 12px', why: 'a PR’s failing checks in its card: meta’s size, wrapping on the card’s 18px line' },
    '.ca-sidestrip-kept-when': { set: 'font-size: 12px', why: 'when a kept side conversation was and how many questions it has, after its question on the conversation’s 19px line' },
    '.ca-agents-foot': { set: 'line-height: 18px', why: 'the note under Settings › Agents, whose lines wrap inline code: body’s 18px line, so one line’s code clears the next' },
    '.ca-cv-tidy-rowlabel': { set: 'font-size: 13px', why: 'a row’s name in Tidy (PRs, Layout): body’s size, on the 16px line of the notes it shares the grid with' },
    '.ca-delta-row.is-lines': { set: 'line-height: 20px', why: 'a row’s lines added and removed, level with the name before it, whose line is taller' },
    '.ca-insp-actions .ca-devbadge': { set: 'font-size: 12px', why: 'a device’s badge among an inspector’s actions: a pixel over the badge’s own size, its height set by its 22px box' },
    '.ca-devgroup-head .ca-devbadge': { set: 'font-size: 12px', why: 'a device’s badge heading its group in Settings › Devices: a pixel over the badge’s own size, its height set by its 20px box' },
    '.ca-acct-meters .ca-meter-value': { set: 'font-size: 20px; line-height: 20px', why: 'a usage meter’s figure in an account’s card: the meter’s face, smaller, on a line of its own size' },
    '.ca-side-update': { set: 'font-size: 12.5px; font-weight: 500', why: 'Restart to update, the sidebar’s one call to act: a size of its own between meta and body, on the sidebar’s line' },
    '.ca-md .ca-md-h': { set: 'font-weight: 600', why: 'a heading in an agent’s Markdown: semibold at the conversation’s size and on its 19px line, so it keeps the reading’s rhythm' },
    '.ca-md .ca-md-h1': { set: 'font-size: 15px', why: 'a first-level heading in an agent’s Markdown: a panel title’s size, on the conversation’s 19px line' },
    '.ca-md table': { set: 'font-size: 12px; line-height: 17px', why: 'a table in an agent’s Markdown: a size under the conversation’s, a pixel looser than meta so rows of figures read' },
    '.ca-md th': { set: 'font-weight: 600', why: 'a Markdown table’s header cells: semibold as headings are, in place of the browser’s bold, at the table’s size' },
    '.ca-prbody table': { set: 'font-size: 12px', why: 'a table in a PR’s description: a size under the description’s, on its 19px line' },
    '.ca-prfile-path mark, .ca-patch mark': { set: 'font-weight: 600', why: 'what the Files changed filter matched, in a path or a diff: semibold at the size of the code it marks' },
  };
  it('write no font size, line height or weight by hand around a type style, but the specials listed here', () => {
    const tokens = JSON.parse(read(join(root, 'design/tokens.json'))) as {
      type: { groups: { family: string; styles: { name: string; fontSize: string; lineHeight: string; fontWeight: number }[] }[] };
    };
    interface Style {
      weight: number;
      size: number;
      line: number;
      family: string;
    }
    const scale = tokens.type.groups.flatMap((g) => g.styles.map((t) => ({ token: `--type-${t.name}`, weight: t.fontWeight, size: parseFloat(t.fontSize), line: parseFloat(t.lineHeight), family: g.family })));
    const show = (s: Style) => `${s.weight} ${s.size}px/${s.line}px ${s.family}`;
    const named = (s: Style) => scale.find((t) => t.weight === s.weight && t.size === s.size && t.line === s.line && t.family === s.family);
    const nearest = (s: Style) => {
      const same = scale.filter((t) => t.family === s.family);
      const score = (t: Style) => Math.abs(t.size - s.size) * 100 + Math.abs(t.line - s.line) * 10 + Math.abs(t.weight - s.weight) / 100;
      return (same.length ? same : scale).reduce((a, b) => (score(b) < score(a) ? b : a));
    };
    // A font shorthand's style: a type token's, or a special's numbers.
    const shorthand = (value: string): (Style & { token?: string }) | null => {
      const token = /^var\((--type-[\w-]+)\)$/.exec(value);
      if (token) return scale.find((t) => t.token === token[1]) ?? null;
      const m = /^(\d+) ([\d.]+)px\/([\d.]+)px var\(--font-(\w+)\)$/.exec(value);
      return m ? { weight: Number(m[1]), size: parseFloat(m[2]), line: parseFloat(m[3]), family: m[4] } : null;
    };
    const FONT = new Set(['font-family', 'font-size', 'line-height', 'font-weight']);
    const HAND = new Set(['font-size', 'line-height', 'font-weight']);
    // Every rule, in the cascade's order, with its declarations.
    const all = parsed
      .filter(({ file }) => file !== join(src, 'styles/tokens.css'))
      .flatMap(({ file }) => {
        const text: string = read(file).replace(/\/\*[\s\S]*?\*\//g, (c: string) => c.replace(/[^\n]/g, ' '));
        return [...text.matchAll(/([^{}]+)\{([^{}]*)\}/g)].map((m) => ({
          file,
          line: text.slice(0, m.index + m[1].length - m[1].trimStart().length).split('\n').length,
          selector: m[1].trim().replace(/\s+/g, ' '),
          decls: m[2]
            .split(';')
            .map((d) => d.trim())
            .filter((d) => d.includes(':'))
            .map((d) => [d.slice(0, d.indexOf(':')).trim(), d.slice(d.indexOf(':') + 1).trim().replace(/\s*!important$/, '')] as [string, string]),
        }));
      });
    // A compound selector's tag and classes, its pseudo-classes left off when `loose`; null for
    // one with anything else in it.
    const parts = (compound: string, loose: boolean) => {
      const bare = loose ? compound.replace(/::?[\w-]+(\([^)]*\))?/g, '') : compound;
      const m = /^([a-z][\w-]*)?((?:\.[\w-]+)*)$/.exec(bare);
      return m && bare ? { tag: m[1] ?? '', classes: m[2].split('.').filter(Boolean) } : null;
    };
    // The rules that give a lone compound a font, each with that font's style.
    const fonts = all.flatMap((r, order) => {
      const font = r.decls.filter(([p]) => p === 'font').pop();
      const style = font && shorthand(font[1]);
      if (!style) return [];
      return splitTop(r.selector).flatMap((sel) => {
        const p = parts(sel, false);
        return p ? [{ rule: r, order, sel, ...p, style }] : [];
      });
    });
    // The font a rule gives an element the compound matches, other than `self`: the most specific
    // whose classes are all the compound's, the last of those.
    const given = (compound: string, self: unknown) => {
      const c = parts(compound, true);
      if (!c) return null;
      const fits = fonts.filter((f) => f.rule !== self && (f.tag === '' || f.tag === c.tag) && f.classes.every((k) => c.classes.includes(k)));
      const weight = (f: (typeof fonts)[number]) => f.classes.length * 10 + (f.tag ? 1 : 0);
      return fits.reduce<(typeof fonts)[number] | null>((a, b) => (!a || weight(b) > weight(a) || (weight(b) === weight(a) && b.order > a.order) ? b : a), null);
    };
    const problems: string[] = [];
    const used = new Set<string>();
    for (const r of all) {
      const hand = r.decls.filter(([p]) => HAND.has(p));
      if (!hand.length) continue;
      const set = hand.map(([p, v]) => `${p}: ${v}`).join('; ');
      if (SPECIAL_LONGHANDS[r.selector]?.set === set) {
        used.add(r.selector);
        continue;
      }
      const own = r.decls.filter(([p]) => p === 'font').pop();
      const after = r.decls.slice(own ? r.decls.lastIndexOf(own) + 1 : 0).filter(([p]) => FONT.has(p));
      for (const sel of splitTop(r.selector)) {
        const where = `${short(r.file)}:${r.line} ${sel} { ${set} }`;
        // The style it starts from: the rule's own font, else one its element's classes are
        // given, else one given a class it's inside.
        let base: (Style & { token?: string }) | null = own ? shorthand(own[1]) : null;
        let how = 'its own font';
        if (!base) {
          const chain = sel.split(/\s*([>+~ ])\s*/);
          const self = own ? null : given(chain[chain.length - 1], r);
          if (self) [base, how] = [self.style, `as ${self.sel} gives it`];
          for (let i = chain.length - 3; !base && i >= 0 && /^[> ]$/.test(chain[i + 1]); i -= 2) {
            const up = given(chain[i], r);
            if (up) [base, how] = [up.style, `inherited from ${up.sel}`];
          }
        }
        // Then what the rule sets after it; a value the test can't reckon with (em, inherit)
        // leaves the style unknown.
        let style: Style | null = base ? { weight: base.weight, size: base.size, line: base.line, family: base.family } : null;
        let factor: number | null = null;
        for (const [p, v] of after) {
          if (!style) break;
          const px = /^([\d.]+)px$/.exec(v);
          const family = /^var\(--font-(\w+)\)$/.exec(v);
          if (p === 'font-family') style = family ? { ...style, family: family[1] } : null;
          else if (p === 'font-size') style = px ? { ...style, size: parseFloat(px[1]) } : null;
          else if (p === 'font-weight') style = /^\d+$/.test(v) ? { ...style, weight: Number(v) } : null;
          else if (px) {
            style = { ...style, line: parseFloat(px[1]) };
            factor = null;
          } else if (/^[\d.]+$/.test(v)) factor = parseFloat(v);
          else style = null;
        }
        if (style && factor != null) style.line = style.size * factor;
        const from = base && (base.token ?? show(base));
        const token = style && named(style);
        const list = `or, if its numbers do a job only this place has, list the rule in SPECIAL_LONGHANDS in this file, with why`;
        if (style && token && token.token === base?.token) problems.push(`${where}: with ${from} (${how}) that's ${token.token} still. Take ${hand.length > 1 ? 'them' : 'it'} out.`);
        else if (style && token) problems.push(`${where} turns ${from} (${how}) into ${token.token}: write font: var(${token.token}) in ${hand.length > 1 ? 'their' : 'its'} place.`);
        else if (style) problems.push(`${where} turns ${from} (${how}) into ${show(style)}, which is no type style. Write the style it's meant to be (the nearest is ${nearest(style).token}); add the style to design/tokens.json if other places share it, named for what it's for; ${list}.`);
        else problems.push(`${where} sets a font by hand on whatever its element has. Give the element the type style it's meant to have, font: var(--type-…) (what it inherits with this change is often one), ${list}.`);
      }
    }
    // The list stays true: each entry is a rule that still sets those.
    for (const [selector, { set }] of Object.entries(SPECIAL_LONGHANDS)) if (!used.has(selector)) problems.push(`SPECIAL_LONGHANDS lists ${selector} { ${set} }, which no stylesheet sets: take it off the list.`);
    expect(problems, problems.join('\n\n')).toEqual([]);
  });

  it('have tokens.css as scripts/tokens.mjs writes it', () => {
    const tokens: unknown = JSON.parse(read(join(root, 'design/tokens.json')));
    expect(read(join(src, 'styles/tokens.css')), 'src/styles/tokens.css differs from what design/tokens.json gives: run `npm run tokens` (and edit tokens.json, never tokens.css).').toBe(tokensCss(tokens).css);
  });
});
