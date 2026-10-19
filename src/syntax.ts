// Code in its colors, as highlight.js reads it: a file's language from its name (`languageOf`),
// and its lines as runs of text (`runsOf`), each in one of a few tones that tokens.json colors in
// both themes (`--syntax-*`, after GitHub's, so code reads here as it does there). highlight.js
// names many more kinds of token than that (its scopes); `TONES` folds them into the ones a reader
// tells apart at a glance, and a scope it doesn't name takes the tone of the one it's in.
//
// highlight.js is loaded the first time a window draws code (`useSyntax`), so a window that never
// does doesn't carry it, and code drawn before it's in shows plain, then in its colors.

import { useEffect, useState } from 'react';
import type { HLJSApi } from 'highlight.js';

export type Tone = 'keyword' | 'string' | 'constant' | 'entity' | 'tag' | 'variable' | 'comment';

/** A stretch of a line of code in one tone; null is the code's own color. */
export interface Run {
  text: string;
  tone: Tone | null;
}

/** highlight.js's scopes (`title.function`, its class `hljs-title function_`) by the tone each is
 *  drawn in; one not here takes its parent's, and null is plain even inside a string (`${…}`). */
const TONES: Record<string, Tone | null> = {
  keyword: 'keyword',
  'variable.language': 'keyword',
  'template-tag': 'keyword',
  doctag: 'keyword',
  string: 'string',
  regexp: 'string',
  symbol: 'string',
  link: 'string',
  number: 'constant',
  literal: 'constant',
  'variable.constant': 'constant',
  built_in: 'constant',
  attr: 'constant',
  attribute: 'constant',
  'selector-attr': 'constant',
  section: 'constant',
  code: 'constant',
  bullet: 'constant',
  title: 'entity',
  type: 'entity',
  meta: 'entity',
  'selector-class': 'entity',
  'selector-id': 'entity',
  'selector-pseudo': 'entity',
  name: 'tag',
  'selector-tag': 'tag',
  variable: 'variable',
  'template-variable': 'variable',
  comment: 'comment',
  quote: 'comment',
  subst: null,
};

/** `table`'s own entry for `key`, never what every object has (`constructor`, `__proto__`): a file
 *  can be named anything. */
const own = <T>(table: Record<string, T>, key: string): T | undefined => (Object.hasOwn(table, key) ? table[key] : undefined);

/** Scopes whose whole content is their tone: JSON's `true` is a literal holding a keyword. */
const HOLDS = new Set(['literal']);

/** A span's class as highlight.js writes it, as its scope: `hljs-title class_ inherited__` is
 *  `title.class.inherited`; a sublanguage's `language-xml` is none. */
function scopeOf(cls: string): string | null {
  const [head, ...rest] = cls.split(' ');
  if (!head.startsWith('hljs-')) return null;
  return [head.slice(5), ...rest.map((r) => r.replace(/_+$/, ''))].join('.');
}

/** The tone a scope is drawn in: its own, else the nearest broader one's (`title.function.invoke`
 *  is a `title`), else undefined for its parent's. */
function toneOf(scope: string): Tone | null | undefined {
  for (let s = scope; ; s = s.slice(0, s.lastIndexOf('.'))) {
    if (Object.hasOwn(TONES, s)) return TONES[s];
    if (!s.includes('.')) return undefined;
  }
}

const ENTITIES: Record<string, string> = { '&amp;': '&', '&lt;': '<', '&gt;': '>', '&quot;': '"', '&#x27;': "'", '&#39;': "'" };
const decode = (text: string) => text.replace(/&(?:amp|lt|gt|quot|#x27|#39);/g, (e) => ENTITIES[e]);

/** highlight.js's HTML for some code as its lines, each as runs of one tone, as many lines as the
 *  code has (a span can run across several: a comment, a template string). */
export function runsOf(html: string): Run[][] {
  const lines: Run[][] = [[]];
  // Each open span's tone, and whether what's in it keeps that tone.
  const open: { tone: Tone | null; holds: boolean }[] = [];
  const add = (text: string) => {
    const tone = open.length > 0 ? open[open.length - 1].tone : null;
    const line = lines[lines.length - 1];
    const last = line[line.length - 1];
    if (last && last.tone === tone) last.text += text;
    else line.push({ text, tone });
  };
  for (const m of html.matchAll(/<span class="([^"]*)">|<\/span>|[^<]+/g)) {
    if (m[1] !== undefined) {
      const parent = open[open.length - 1];
      const scope = scopeOf(m[1]);
      const own = scope === null || parent?.holds ? undefined : toneOf(scope);
      open.push({ tone: own === undefined ? (parent?.tone ?? null) : own, holds: !!parent?.holds || (scope !== null && HOLDS.has(scope)) });
    } else if (m[0] === '</span>') {
      open.pop();
    } else {
      decode(m[0])
        .split('\n')
        .forEach((text, i) => {
          if (i > 0) lines.push([]);
          if (text) add(text);
        });
    }
  }
  return lines;
}

/** Runs cut where `ranges` (prFiles.ts `marks`: start and end in the line, in order, none
 *  overlapping) begin and end, gathered into what's marked and what isn't, in order. The runs and
 *  the ranges are walked together, so a long line with many of each costs no more than both. */
export function paint(runs: Run[], ranges: [number, number][]): { hit: boolean; runs: Run[] }[] {
  const out: { hit: boolean; runs: Run[] }[] = [];
  const put = (hit: boolean, run: Run) => {
    const last = out[out.length - 1];
    if (last && last.hit === hit) last.runs.push(run);
    else out.push({ hit, runs: [run] });
  };
  let at = 0;
  // The first range that doesn't end before the run.
  let next = 0;
  for (const run of runs) {
    const end = at + run.text.length;
    let from = at;
    while (next < ranges.length && ranges[next][1] <= from) next++;
    for (let i = next; i < ranges.length && ranges[i][0] < end; i++) {
      const [a, b] = ranges[i];
      if (a > from) put(false, { text: run.text.slice(from - at, a - at), tone: run.tone });
      const to = Math.min(b, end);
      put(true, { text: run.text.slice(Math.max(a, from) - at, to - at), tone: run.tone });
      from = to;
    }
    if (from < end) put(false, { text: run.text.slice(from - at), tone: run.tone });
    at = end;
  }
  return out;
}

/** Names that say a file's language where its extension doesn't, in lower case. */
const NAMES: Record<string, string> = {
  dockerfile: 'dockerfile',
  containerfile: 'dockerfile',
  makefile: 'makefile',
  gnumakefile: 'makefile',
  gemfile: 'ruby',
  rakefile: 'ruby',
  podfile: 'ruby',
  '.bashrc': 'bash',
  '.zshrc': 'bash',
  '.profile': 'bash',
};

/** Extensions highlight.js doesn't take as a language's name, and what they're written in. */
const EXTENSIONS: Record<string, string> = { htm: 'xml', vue: 'xml', svelte: 'xml', mdx: 'markdown', pyi: 'python', gradle: 'groovy' };

/** The language a file's in, by its name, as one `known` (highlight.js's names and their
 *  aliases) takes; null for one it doesn't, which shows plain. */
export function languageOf(path: string, known: (name: string) => boolean): string | null {
  const name = path.slice(path.lastIndexOf('/') + 1).toLowerCase();
  const dot = name.lastIndexOf('.');
  const ext = dot > 0 ? name.slice(dot + 1) : '';
  const named = own(NAMES, name) ?? (name.startsWith('dockerfile.') || ext === 'dockerfile' ? 'dockerfile' : undefined);
  const language = named ?? own(EXTENSIONS, ext) ?? ext;
  return language && known(language) ? language : null;
}

/** What a window colors code with, once highlight.js is in. */
export interface Syntax {
  /** The language a file's in, by its path; null for one shown plain. */
  language: (path: string) => string | null;
  /** Code in a language as its lines' runs (`runsOf`); null if highlight.js can't read it. */
  lines: (code: string, language: string) => Run[][] | null;
}

function syntaxOf(hljs: HLJSApi): Syntax {
  return {
    language: (path) => languageOf(path, (name) => !!hljs.getLanguage(name)),
    lines: (code, language) => {
      try {
        return runsOf(hljs.highlight(code, { language, ignoreIllegals: true }).value);
      } catch {
        // A grammar that fails on some code leaves it plain, which still reads.
        return null;
      }
    },
  };
}

let loaded: Syntax | null = null;
let loading: Promise<Syntax> | null = null;

/** highlight.js with its common languages and a few more, loaded once. */
function loadSyntax(): Promise<Syntax> {
  loading ??= Promise.all([
    import('highlight.js/lib/common'),
    import('highlight.js/lib/languages/dockerfile'),
    import('highlight.js/lib/languages/dart'),
    import('highlight.js/lib/languages/elixir'),
    import('highlight.js/lib/languages/groovy'),
    import('highlight.js/lib/languages/haskell'),
    import('highlight.js/lib/languages/powershell'),
    import('highlight.js/lib/languages/protobuf'),
    import('highlight.js/lib/languages/scala'),
  ]).then(([common, ...more]) => {
    const hljs = common.default;
    const names = ['dockerfile', 'dart', 'elixir', 'groovy', 'haskell', 'powershell', 'protobuf', 'scala'];
    more.forEach((m, i) => hljs.registerLanguage(names[i], m.default));
    loaded = syntaxOf(hljs);
    return loaded;
  });
  return loading;
}

/** What colors code in this window: null until highlight.js is in, which it starts loading. */
export function useSyntax(): Syntax | null {
  const [syntax, setSyntax] = useState(loaded);
  useEffect(() => {
    if (syntax) return;
    let live = true;
    loadSyntax()
      .then((s) => live && setSyntax(s))
      // Not loading only leaves code in one color, which still reads.
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [syntax]);
  return syntax;
}
