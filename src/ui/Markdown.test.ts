import { describe, expect, it } from 'vitest';
import { isValidElement, type ReactNode } from 'react';
import { blocks, inline, linked } from './Markdown';
import { FileChip, KeptImage } from './DescriptionEditor';
import { prLabel, prRefOf } from '../prLinks';

/** The web links in a line, as [the words they show, url], found through their elements. A bare
 *  link shows its URL, or a PR's as GitHub shortens it unless it's `plain`. */
function links(nodes: ReactNode[]): [string, string][] {
  const out: [string, string][] = [];
  const text = (n: ReactNode): string => (Array.isArray(n) ? n.map(text).join('') : isValidElement<{ children?: ReactNode }>(n) ? text(n.props.children) : n == null || typeof n === 'boolean' ? '' : String(n));
  const walk = (n: ReactNode) => {
    if (Array.isArray(n)) n.forEach(walk);
    else if (isValidElement<{ url?: string; plain?: boolean; children?: ReactNode }>(n)) {
      const { url, plain } = n.props;
      if (url) out.push([n.props.children == null ? (prRefOf(url) && !plain ? prLabel(prRefOf(url)!) : url) : text(n.props.children), url]);
      walk(n.props.children);
    }
  };
  walk(nodes);
  return out;
}

describe('inline', () => {
  it('makes web links clickable, bare or written as links, and leaves others as text', () => {
    expect(links(inline('See https://github.com/a/b/pull/12. Or [the docs](https://x.dev/a_b) and <https://y.io>.'))).toEqual([
      ['b#12', 'https://github.com/a/b/pull/12'],
      ['the docs', 'https://x.dev/a_b'],
      ['https://y.io', 'https://y.io'],
    ]);
    expect(links(inline('[file](src/a.ts) and `https://in.code`'))).toEqual([]);
    expect(links(inline('**(https://bold.example/x)**'))).toEqual([['https://bold.example/x', 'https://bold.example/x']]);
  });

  it("doesn't put a link inside a link", () => {
    expect(links(inline('[https://a.io](https://a.io)'))).toEqual([['https://a.io', 'https://a.io']]);
  });

  it('shows a file or an image Codealong keeps as a prompt of yours does, and any other image as before', () => {
    const kept = inline('The log is [run.log](files/0123456789abcdef/run.log): ![shot](images/0123456789abcdef.png)');
    const props = (type: unknown) => kept.filter((n) => isValidElement(n) && n.type === type).map((n) => (isValidElement<Record<string, unknown>>(n) ? n.props : null));
    expect(props(FileChip)).toEqual([{ name: 'run.log', path: 'files/0123456789abcdef/run.log' }]);
    expect(props(KeptImage)).toEqual([{ name: '0123456789abcdef.png', alt: 'shot' }]);
    // Not one it keeps: its `!` and its link's words, a web link opening.
    expect(links(inline('![chart](https://a.io/c.png) and ![x](images/elsewhere.png)'))).toEqual([['chart', 'https://a.io/c.png']]);
    const text = (n: ReactNode): string => (Array.isArray(n) ? n.map(text).join('') : isValidElement<{ children?: ReactNode }>(n) ? text(n.props.children) : n == null ? '' : String(n));
    expect(text(inline('![x](images/elsewhere.png) [y](files/nope/y)'))).toBe('!x y');
  });
});

describe('linked', () => {
  /** What the nodes read as: output must keep every character. */
  const words = (nodes: ReactNode[]) => nodes.map((n) => (isValidElement<{ url: string }>(n) ? n.props.url : String(n))).join('');

  it('makes the web links in a tool’s output clickable and keeps its text as it was', () => {
    const published = 'Published /private/tmp/x/security-dashboard.html at https://claude.ai/artifact/VuawMfgy2qCBAFhpc6dC3H (Version 9, version id 1790960406-8f4f)\n\nTo update: publish the same file path here.';
    expect(links(linked(published))).toEqual([['https://claude.ai/artifact/VuawMfgy2qCBAFhpc6dC3H', 'https://claude.ai/artifact/VuawMfgy2qCBAFhpc6dC3H']]);
    expect(words(linked(published))).toBe(published);
    // A PR's link reads as it was printed, not as GitHub shortens it.
    expect(links(linked('https://github.com/a/b/pull/12\n'))).toEqual([['https://github.com/a/b/pull/12', 'https://github.com/a/b/pull/12']]);
  });

  it('ends a link where JSON does', () => {
    const json = '{"url":"https://a.dev/x?y=1","title":"T"}\n{"href": "https://b.dev/p_q",\n "note": "see https://c.dev/end."}\n"https:\\/\\/escaped.dev"';
    expect(links(linked(json)).map(([, url]) => url)).toEqual(['https://a.dev/x?y=1', 'https://b.dev/p_q', 'https://c.dev/end']);
    expect(words(linked(json))).toBe(json);
    expect(linked('no links here')).toEqual(['no links here']);
  });
});

describe('blocks', () => {
  /** An item that's one paragraph, as the parser reads one. */
  const para = (text: string) => [{ kind: 'para', text }];

  it('reads headings, lists, code and paragraphs the way agents write them', () => {
    const b = blocks("Here's a plan.\n\n## What I found\n\n- **One** thing\n  wraps on\n- `two`\n\n1. first\n2. second\n\n```\ncargo test\n```\n> a quote\n---\nlast line");
    expect(b.map((x) => x.kind)).toEqual(['para', 'heading', 'list', 'list', 'code', 'quote', 'rule', 'para']);
    expect(b[2]).toEqual({ kind: 'list', ordered: false, items: [para('**One** thing wraps on'), para('`two`')] });
    expect(b[3]).toEqual({ kind: 'list', ordered: true, items: [para('first'), para('second')] });
    expect(b[4]).toEqual({ kind: 'code', text: 'cargo test', shell: false, session: false, bare: true });
  });

  it('reads a code block in a list as part of its item, and the list going on after it', () => {
    const b = blocks('Then:\n1. Run this:\n   ```bash\n   cd app &&\n     npm test\n   ```\n2. Check the output.');
    expect(b).toEqual([
      { kind: 'para', text: 'Then:' },
      {
        kind: 'list',
        ordered: true,
        items: [[{ kind: 'para', text: 'Run this:' }, { kind: 'code', text: 'cd app &&\n  npm test', shell: true, session: false, bare: false }], para('Check the output.')],
      },
    ]);
  });

  it('nests a list indented under an item, and the paragraph under it, as Claude Code writes them', () => {
    // A reply that came out flat, its sub-items beside their items and its paragraph out of the
    // list (2026-10-08).
    const src = [
      '- **For a Northwind review, effort sets how many checks the planner proposes:**',
      '  - Quick: about 10.',
      '  - Deep: also every interface.',
      '',
      '  The approval card would say what the next level adds.',
      "- **A customer's own list has no effort setting.** The list is the scope.",
      '- **"Only P1s" isn\'t effort.** It fits in two places:',
      '  - the list;',
      '  - a filter.',
    ].join('\n');
    expect(blocks(src)).toEqual([
      {
        kind: 'list',
        ordered: false,
        items: [
          [
            { kind: 'para', text: '**For a Northwind review, effort sets how many checks the planner proposes:**' },
            { kind: 'list', ordered: false, items: [para('Quick: about 10.'), para('Deep: also every interface.')] },
            { kind: 'para', text: 'The approval card would say what the next level adds.' },
          ],
          para("**A customer's own list has no effort setting.** The list is the scope."),
          [{ kind: 'para', text: '**"Only P1s" isn\'t effort.** It fits in two places:' }, { kind: 'list', ordered: false, items: [para('the list;'), para('a filter.')] }],
        ],
      },
    ]);
  });

  it('keeps items a blank line apart in one list, and starts another where the kind changes', () => {
    expect(blocks('1. one\n\n2. two\n\n- a')).toEqual([
      { kind: 'list', ordered: true, items: [para('one'), para('two')] },
      { kind: 'list', ordered: false, items: [para('a')] },
    ]);
    // A numbered list that starts later keeps its number, and lists nest three deep.
    expect(blocks('3. three\n   - a\n     - b\n\nafter')).toEqual([
      {
        kind: 'list',
        ordered: true,
        start: 3,
        items: [[{ kind: 'para', text: 'three' }, { kind: 'list', ordered: false, items: [[{ kind: 'para', text: 'a' }, { kind: 'list', ordered: false, items: [para('b')] }]] }]],
      },
      { kind: 'para', text: 'after' },
    ]);
  });

  it('keeps a code block whole in its item, however its lines are indented', () => {
    expect(blocks('- Run:\n  ```\nmake all\n  ```\n- Then check.')).toEqual([
      { kind: 'list', ordered: false, items: [[{ kind: 'para', text: 'Run:' }, { kind: 'code', text: 'make all', shell: false, session: false, bare: true }], para('Then check.')] },
    ]);
  });

  it('marks finished shell blocks, which can run, and no others', () => {
    const shell = (src: string) => blocks(src).map((b) => b.kind === 'code' && b.shell);
    expect(shell('```bash\nnpm test\n```')).toEqual([true]);
    expect(shell('  ```sh title="build"\n  make\n  ```')).toEqual([true]);
    expect(shell('```\nnpm test\n```\n\n```ts\nconst a = 1;\n```\n\n```console\n$ ls\na.txt\n```')).toEqual([false, false, false]);
    // Still being written: the reply streams in.
    expect(blocks('Run:\n```bash\nnpm te')).toEqual([{ kind: 'para', text: 'Run:' }, { kind: 'code', text: 'npm te', shell: false, session: false, bare: false }]);
  });

  it('marks finished blocks with no language, which run if they hold commands', () => {
    const bare = (src: string) => blocks(src).map((b) => b.kind === 'code' && b.bare);
    expect(bare('```\ngit pull --ff-only\n```\n\n```bash\nnpm test\n```\n\n```toml\n[a]\n```')).toEqual([true, false, false]);
    // A block led by ! is one already; one still being written isn't yet.
    expect(bare('```\n! npm test\n```\n\n```\nnpm te')).toEqual([false, false]);
  });

  it('marks a block led by ! as a command for the shell of the session', () => {
    const marks = (src: string) => blocks(src).map((b) => b.kind === 'code' && [b.shell, b.session]);
    // As an agent writes one for you to run in the session (2026-10-07), with no language or a shell's.
    expect(marks('To send it, run:\n\n```\n! cd /w && node scripts/notify.mjs send\n```')).toEqual([false, [true, true]]);
    expect(marks('```bash\n!npm test\n```')).toEqual([[true, true]]);
    // Not in another language, nor while it's still being written.
    expect(marks('```js\n!done && go()\n```')).toEqual([[false, false]]);
    expect(marks('```\n! npm te')).toEqual([[false, false]]);
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

describe('Codex directives', () => {
  it('reads a writing block as a card holding its Markdown, up to the ::: that closes it', () => {
    const b = blocks('Here’s a prompt you can paste:\n:::writing{variant="standard" id="83147"}\nW&M 3 is **done**.\n\n- a\n- b\n:::\nAfter.');
    expect(b.map((x) => x.kind)).toEqual(['para', 'card', 'para']);
    expect(b[1]).toEqual({ kind: 'card', name: 'writing', attrs: { variant: 'standard', id: '83147' }, text: 'W&M 3 is **done**.\n\n- a\n- b' });
  });

  it('keeps a ::: inside a code block in a card, and a card still being written holds the rest', () => {
    const b = blocks(':::writing{variant="document" id="1"}\n```\n:::\n```\nmore');
    expect(b).toEqual([{ kind: 'card', name: 'writing', attrs: { variant: 'document', id: '1' }, text: '```\n:::\n```\nmore' }]);
    // Another directive's block is a card too, with no attributes.
    expect(blocks(':::note\nHeads up.\n:::')).toEqual([{ kind: 'card', name: 'note', attrs: {}, text: 'Heads up.' }]);
  });

  it('reads a code comment alone on its line, and leaves a directive it doesn’t know as text', () => {
    const b = blocks('Two findings:\n::code-comment{title="[P2] Off-by-one" body="Past the end at 0." file="/w/foo.ts" start=10 end=11}\n::created-thread{threadId="t1"}');
    expect(b).toEqual([
      { kind: 'para', text: 'Two findings:' },
      { kind: 'comment', attrs: { title: '[P2] Off-by-one', body: 'Past the end at 0.', file: '/w/foo.ts', start: '10', end: '11' } },
      { kind: 'para', text: '::created-thread{threadId="t1"}' },
    ]);
  });

  it('draws a follow-up and a cited file in a line as theirs', () => {
    const props = (nodes: ReactNode[]) => nodes.filter((n) => isValidElement(n)).map((n) => n.props as Record<string, unknown>);
    expect(props(inline('- :codex-followup[Fix image downloads]{prompt="Implement the \\"download\\" fix. Do not push."}'))).toEqual([{ label: 'Fix image downloads', prompt: 'Implement the "download" fix. Do not push.' }]);
    // A cited file opens on a click, but not inside a link's text, where the link takes it.
    expect(props(inline(':codex-file-citation{path="/Users/me/Bldg 742/Architectural.pdf" purpose="source"}'))).toEqual([{ path: '/Users/me/Bldg 742/Architectural.pdf', opens: true }]);
    expect(props(inline(':codex-file-citation{path="/a/b.pdf"}', false))).toEqual([{ path: '/a/b.pdf', opens: false }]);
    // In code it's code, and in a word or a URL it's the word's.
    expect(props(inline('`:codex-followup[a]{prompt="b"}`')).map((p) => p.children)).toEqual([':codex-followup[a]{prompt="b"}']);
    expect(props(inline('x:codex-file-citation{path="/a"}'))).toEqual([]);
  });
});
