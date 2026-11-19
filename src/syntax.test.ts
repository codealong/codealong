import hljs from 'highlight.js/lib/common';
import { describe, expect, it } from 'vitest';
import { fenceLanguage, fenceNamesLanguage, languageOf, paint, runsOf, type Run } from './syntax';

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

  it('colors a diff’s added lines as GitHub does, green, and its removed ones red', () => {
    expect(said('@@ -1 +1 @@\n-old\n+new\n same', 'diff')).toEqual([['entity:@@ -1 +1 @@'], ['keyword:-old'], ['tag:+new'], ['-: same']]);
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

  it('walks a long line’s runs and marks together', () => {
    // A minified line: tens of thousands of runs and of marks. Walked together they take a few
    // milliseconds; a run tried against every mark, as it once was, takes seconds. The time is
    // the best of three, since anything can hold one run up, and the limit is far from both. At
    // a load of 900 on ten cores (2026-11-19) single runs of this took 7 to 520 ms, the best of
    // three 34 at most, and every mark against every run 20 s; the test this replaces, one run
    // of half the size under 200 ms, failed there every time.
    const long: Run[] = Array.from({ length: 60000 }, (_, i) => ({ text: 'ab', tone: i % 2 ? 'keyword' : null }));
    const ranges = Array.from({ length: 20000 }, (_, i): [number, number] => [i * 6 + 1, i * 6 + 3]);
    let groups: ReturnType<typeof paint> = [];
    const took = Math.min(
      ...[0, 1, 2].map(() => {
        const started = performance.now();
        groups = paint(long, ranges);
        return performance.now() - started;
      }),
    );
    expect(took).toBeLessThan(1000);
    expect(groups.filter((g) => g.hit)).toHaveLength(20000);
    expect(groups.flatMap((g) => g.runs).map((r) => r.text).join('')).toBe('ab'.repeat(60000));
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

  it('takes a name every object has for a name like any other', () => {
    const hljsKnown = (name: string) => !!hljs.getLanguage(name);
    expect(languageOf('src/constructor', hljsKnown)).toBeNull();
    expect(languageOf('data.__proto__', hljsKnown)).toBeNull();
    expect(languageOf('a.toString', hljsKnown)).toBeNull();
  });
});

describe('fenceLanguage', () => {
  const known = (name: string) => !!hljs.getLanguage(name);

  it('takes the fence’s word as highlight.js names languages and their aliases', () => {
    expect(fenceLanguage('ts', known)).toBe('ts');
    expect(fenceLanguage('rust', known)).toBe('rust');
    expect(fenceLanguage('Python', known)).toBe('python');
    expect(fenceLanguage('diff', known)).toBe('diff');
  });

  it('reads `shell` as a script, not the console session highlight.js takes it for', () => {
    expect(fenceLanguage('shell', known)).toBe('bash');
    expect(fenceLanguage('jsonc', known)).toBe('json');
  });

  it('leaves plain what names no language, or one highlight.js doesn’t know', () => {
    for (const word of ['', 'text', 'txt', 'output', 'nosuchlang', 'constructor', '__proto__']) expect(fenceLanguage(word, known)).toBeNull();
  });

  it('says before highlight.js is in whether a block could want it', () => {
    expect(fenceNamesLanguage('ts')).toBe(true);
    expect(fenceNamesLanguage('')).toBe(false);
    expect(fenceNamesLanguage('text')).toBe(false);
  });
});
