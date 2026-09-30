import { describe, expect, it } from 'vitest';
import { isValidElement, type ReactNode } from 'react';
import { blocks, inline } from './Markdown';

/** The web links in a line, as [text, url], found through its elements. */
function links(nodes: ReactNode[]): [string, string][] {
  const out: [string, string][] = [];
  const text = (n: ReactNode): string => (Array.isArray(n) ? n.map(text).join('') : isValidElement<{ children?: ReactNode }>(n) ? text(n.props.children) : n == null || typeof n === 'boolean' ? '' : String(n));
  const walk = (n: ReactNode) => {
    if (Array.isArray(n)) n.forEach(walk);
    else if (isValidElement<{ url?: string; children?: ReactNode }>(n)) {
      if (n.props.url) out.push([text(n.props.children), n.props.url]);
      walk(n.props.children);
    }
  };
  walk(nodes);
  return out;
}

describe('inline', () => {
  it('makes web links clickable, bare or written as links, and leaves others as text', () => {
    expect(links(inline('See https://github.com/a/b/pull/12. Or [the docs](https://x.dev/a_b) and <https://y.io>.'))).toEqual([
      ['https://github.com/a/b/pull/12', 'https://github.com/a/b/pull/12'],
      ['the docs', 'https://x.dev/a_b'],
      ['https://y.io', 'https://y.io'],
    ]);
    expect(links(inline('[file](src/a.ts) and `https://in.code`'))).toEqual([]);
    expect(links(inline('**(https://bold.example/x)**'))).toEqual([['https://bold.example/x', 'https://bold.example/x']]);
  });

  it("doesn't put a link inside a link", () => {
    expect(links(inline('[https://a.io](https://a.io)'))).toEqual([['https://a.io', 'https://a.io']]);
  });
});

describe('blocks', () => {
  it('reads headings, lists, code and paragraphs the way agents write them', () => {
    const b = blocks("Here's a plan.\n\n## What I found\n\n- **One** thing\n  wraps on\n- `two`\n\n1. first\n2. second\n\n```\ncargo test\n```\n> a quote\n---\nlast line");
    expect(b.map((x) => x.kind)).toEqual(['para', 'heading', 'list', 'list', 'code', 'quote', 'rule', 'para']);
    expect(b[2]).toEqual({ kind: 'list', ordered: false, items: ['**One** thing wraps on', '`two`'] });
    expect(b[3]).toMatchObject({ ordered: true, items: ['first', 'second'] });
    expect(b[4]).toEqual({ kind: 'code', text: 'cargo test', shell: false });
  });

  it('reads a code block in a list as code, and the list going on after it', () => {
    const b = blocks('Then:\n1. Run this:\n   ```bash\n   cd app &&\n     npm test\n   ```\n2. Check the output.');
    expect(b).toEqual([
      { kind: 'para', text: 'Then:' },
      { kind: 'list', ordered: true, items: ['Run this:'], start: 1 },
      { kind: 'code', text: 'cd app &&\n  npm test', shell: true },
      { kind: 'list', ordered: true, items: ['Check the output.'], start: 2 },
    ]);
  });

  it('marks finished shell blocks, which can run, and no others', () => {
    const shell = (src: string) => blocks(src).map((b) => b.kind === 'code' && b.shell);
    expect(shell('```bash\nnpm test\n```')).toEqual([true]);
    expect(shell('  ```sh title="build"\n  make\n  ```')).toEqual([true]);
    expect(shell('```\nnpm test\n```\n\n```ts\nconst a = 1;\n```\n\n```console\n$ ls\na.txt\n```')).toEqual([false, false, false]);
    // Still being written: the reply streams in.
    expect(blocks('Run:\n```bash\nnpm te')).toEqual([{ kind: 'para', text: 'Run:' }, { kind: 'code', text: 'npm te', shell: false }]);
  });

  it('reads tables, with alignment, ragged rows and pipes in code', () => {
    const b = blocks('Results:\n| File | Lines | Note |\n|:--|--:|:-:|\n| `a.ts` | 12 | uses `x | y` |\n| b.ts | 3 |\n\nafter');
    expect(b.map((x) => x.kind)).toEqual(['para', 'table', 'para']);
    expect(b[1]).toEqual({
      kind: 'table',
      align: ['left', 'right', 'center'],
      head: ['File', 'Lines', 'Note'],
      rows: [
        ['`a.ts`', '12', 'uses `x | y`'],
        ['b.ts', '3', ''],
      ],
    });
  });

  it("doesn't read a pipe over a rule as a table", () => {
    expect(blocks('a | b\n---').some((x) => x.kind === 'table')).toBe(false);
  });
});
