import hljs from 'highlight.js/lib/common';
import { describe, expect, it } from 'vitest';
import { languageOf, paint, runsOf, type Run } from './syntax';

/** Code in a language as highlight.js colors it, each line as `tone:text` runs. */
const said = (code: string, language: string) => runsOf(hljs.highlight(code, { language, ignoreIllegals: true }).value).map((line) => line.map((r) => `${r.tone ?? '-'}:${r.text}`));

describe('runsOf', () => {
  it('folds highlight.js scopes into tones, a run a stretch of one', () => {
    expect(said('export function f(a: number) { return this.g(1) > 2; } // x', 'typescript')).toEqual([
      ['keyword:export', '-: ', 'keyword:function', '-: ', 'entity:f', '-:(', 'constant:a', '-:: ', 'constant:number', '-:) { ', 'keyword:return', '-: ', 'keyword:this', '-:.', 'entity:g', '-:(', 'constant:1', '-:) > ', 'constant:2', '-:; } ', 'comment:// x'],
    ]);
  });

  it('keeps a line for each of the code’s, a span running across them in its tone on each', () => {
    expect(said('a = 1; /* one\ntwo */\n\nb', 'javascript')).toEqual([['-:a = ', 'constant:1', '-:; ', 'comment:/* one'], ['comment:two */'], [], ['-:b']]);
  });

  it('draws what’s put into a string plain, and what a literal holds as the literal', () => {
    expect(said('`a ${b} c`', 'javascript')).toEqual([['string:`a ', '-:${b}', 'string: c`']]);
    expect(said('[true, null]', 'json')).toEqual([['-:[', 'constant:true', '-:, ', 'constant:null', '-:]']]);
  });

  it('gives back the code’s own characters', () => {
    const code = `if (a < b && c > "d") { e = 'f'; }`;
    expect(runsOf(hljs.highlight(code, { language: 'javascript' }).value)[0].map((r) => r.text).join('')).toBe(code);
  });

  it('colors tags and their attributes in markup', () => {
    expect(said('<a href="x">y</a>', 'xml')).toEqual([['-:<', 'tag:a', '-: ', 'constant:href', '-:=', 'string:"x"', '-:>y</', 'tag:a', '-:>']]);
  });
});

describe('paint', () => {
  const runs: Run[] = [
    { text: 'const ', tone: 'keyword' },
    { text: 'team', tone: null },
    { text: ' = 1', tone: null },
  ];
  const as = (p: ReturnType<typeof paint>) => p.map((g) => (g.hit ? '[' : '') + g.runs.map((r) => `${r.tone ?? '-'}:${r.text}`).join(' ') + (g.hit ? ']' : ''));

  it('cuts runs where the marks begin and end, a mark across runs kept as one', () => {
    expect(as(paint(runs, [[3, 8]]))).toEqual(['keyword:con', '[keyword:st  -:te]', '-:am -: = 1']);
    expect(as(paint(runs, [[0, 2], [6, 10]]))).toEqual(['[keyword:co]', 'keyword:nst ', '[-:team]', '-: = 1']);
  });

  it('leaves runs as they are with nothing marked', () => {
    expect(as(paint(runs, []))).toEqual(['keyword:const  -:team -: = 1']);
  });
});

describe('languageOf', () => {
  const known = (name: string) => ['typescript', 'ts', 'tsx', 'rs', 'dockerfile', 'makefile', 'xml', 'ruby', 'markdown'].includes(name);

  it('goes by the extension, as highlight.js names languages', () => {
    expect(languageOf('apps/app/src/Team.tsx', known)).toBe('tsx');
    expect(languageOf('src-tauri/src/lib.RS', known)).toBe('rs');
    expect(languageOf('notes.txt', known)).toBeNull();
    expect(languageOf('LICENSE', known)).toBeNull();
    // A name that starts with a dot has no extension.
    expect(languageOf('.ts', known)).toBeNull();
  });

  it('knows the files named for their language, and extensions highlight.js doesn’t', () => {
    expect(languageOf('deploy/Dockerfile', known)).toBe('dockerfile');
    expect(languageOf('deploy/Dockerfile.dev', known)).toBe('dockerfile');
    expect(languageOf('Makefile', known)).toBe('makefile');
    expect(languageOf('Gemfile', known)).toBe('ruby');
    expect(languageOf('ui/App.vue', known)).toBe('xml');
    expect(languageOf('docs/guide.mdx', known)).toBe('markdown');
  });
});
