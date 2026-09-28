import { describe, expect, it } from 'vitest';
import { blocks } from './Markdown';

describe('blocks', () => {
  it('reads headings, lists, code and paragraphs the way agents write them', () => {
    const b = blocks("Here's a plan.\n\n## What I found\n\n- **One** thing\n  wraps on\n- `two`\n\n1. first\n2. second\n\n```\ncargo test\n```\n> a quote\n---\nlast line");
    expect(b.map((x) => x.kind)).toEqual(['para', 'heading', 'list', 'list', 'code', 'quote', 'rule', 'para']);
    expect(b[2]).toEqual({ kind: 'list', ordered: false, items: ['**One** thing wraps on', '`two`'] });
    expect(b[3]).toMatchObject({ ordered: true, items: ['first', 'second'] });
    expect(b[4]).toEqual({ kind: 'code', text: 'cargo test' });
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
