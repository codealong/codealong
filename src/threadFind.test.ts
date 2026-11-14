import { describe, expect, it } from 'vitest';
import { chainFrom, findFor, pageHits, plainOf, shownText } from './threadFind';
import { findPattern, says } from './windowFind';
import type { TranscriptItem } from './types';

const item = (kind: TranscriptItem['kind'], text: string, extra: Partial<TranscriptItem> = {}): TranscriptItem => ({ at: 0, kind, text, failed: false, cursor: 0, n: 0, ...extra });

describe('plainOf', () => {
  it('reads Markdown as it shows', () => {
    expect(plainOf('## Done\n\nThe **fix** is in `lanes.ts`, see [the PR](https://x/1).\n> quoted ![shot](images/a.png)')).toBe('Done\n\nThe fix is in lanes.ts, see the PR.\nquoted  ');
  });

  it('keeps the underscores in a name', () => {
    expect(says(findPattern('snake_case_name')!, plainOf('call `snake_case_name` here'))).toBe(true);
  });

  it('leaves the marks it doesn’t draw, as a glob written without backticks', () => {
    expect(plainOf('Read apps/*, libs/* and src/__tests__, *then* **all of `it`** and __more__.')).toBe('Read apps/*, libs/* and src/__tests__, then all of it and more.');
    // Inside one another, as they're drawn, but code as it's written.
    expect(plainOf('__very *important*__, *a __b__ c* and **`__x__`**')).toBe('very important, a b c and __x__');
  });
});

describe('shownText', () => {
  it('is what each item shows', () => {
    expect(shownText(item('you', 'fix the **bar**'))).toBe('fix the bar');
    expect(shownText(item('you', '', { input: 'git status' }))).toBe('! git status');
    expect(shownText(item('tool', 'Bash · `npm test`', { input: 'npm test --watch', output: 'all passed' }))).toBe('Bash · npm test');
    expect(shownText(item('note', 'Conversation compacted'))).toBe('Conversation compacted');
    expect(shownText(item('codealong', 'On your PR #7: **ken** approved it.'))).toBe('On your PR #7: ken approved it.');
    expect(shownText(item('peer', 'Which test fails?\n\n- The **parser**’s'))).toBe('Which test fails?\n\n- The parser’s');
    expect(shownText(item('result', '', { output: 'all passed' }))).toBe('');
    // Your answers to questions show as a message of yours.
    expect(shownText(item('result', '', { output: 'Your questions have been answered', answers: { 'Which limit?': ['`100` days'], 'Which days?': ['Mon', 'Tue'] } }))).toBe('100 days\nMon, Tue');
    // Drawn as written but for a code span's marks: no emphasis is read in them.
    expect(shownText(item('result', '', { answers: { 'Which files?': ['*.ts and __init__.py'] } }))).toBe('*.ts and __init__.py');
    const sent = { files: ['/tmp/shots/before.png', '/tmp/shots/after.png'], caption: 'Before and after' };
    expect(shownText(item('tool', 'SendUserFile · `before.png` and 1 more', { sent }))).toBe('SendUserFile · before.png and 1 more\nBefore and after\nbefore.png\nafter.png');
  });

  it('reads Codex directives as they’re drawn, in a reply or a prompt, and Codealong’s as written', () => {
    const block = ':::writing{variant="standard" id="1"}\nRead the status.\n:::';
    expect(shownText(item('agent', block))).toBe('Writing\nRead the status.');
    expect(shownText(item('you', block))).toBe('Writing\nRead the status.');
    expect(shownText(item('codealong', block))).toBe(block);
  });

  it('leaves out what shows only once opened', () => {
    const p = findPattern('passed')!;
    expect(pageHits(p, [item('tool', 'Bash · `npm test`', { output: 'all passed' }), item('result', '', { output: 'passed' })])).toBe(0);
    expect(pageHits(p, [item('agent', 'It passed, and passed again.'), item('you', 'passed?')])).toBe(3);
  });

  it('counts your answers to questions each apart, as they’re laid out', () => {
    const answers = item('result', '', { answers: { 'Which colour?': ['Red'], 'Which size?': ['Large'] } });
    expect(pageHits(findPattern('red')!, [answers])).toBe(1);
    expect(pageHits(findPattern('red large')!, [answers])).toBe(0);
  });
});

describe('chainFrom', () => {
  const read = new Map([
    [300, { before: 200 }],
    [200, { before: 100 }],
    [100, { before: null }],
    [50, { before: null }],
  ]);

  it('follows the pages back from where the ones shown begin', () => {
    const c = chainFrom(300, read);
    expect(c.pages.map((p) => p.at)).toEqual([300, 200, 100]);
    expect(c.whole).toBe(true);
    expect(chainFrom(200, read).pages.map((p) => p.at)).toEqual([200, 100]);
  });

  it('stops at a page not read yet', () => {
    const c = chainFrom(300, new Map([[300, { before: 200 }]]));
    expect(c.pages.map((p) => p.at)).toEqual([300]);
    expect(c.whole).toBe(false);
    expect(chainFrom(400, read)).toEqual({ pages: [], whole: false });
  });

  it('is whole with the session’s start shown', () => {
    expect(chainFrom(null, read)).toEqual({ pages: [], whole: true });
  });
});

describe('findFor', () => {
  it('finds the words as typed when the line found has them together', () => {
    expect(findFor('…so the Webhook   retries go through the same path', ['webhook', 'retries'])).toBe('webhook retries');
    expect(findFor('The retry loop lives in stripe.ts', ['stripe'])).toBe('stripe');
  });
  it('else the longest word the line has, else the longest typed', () => {
    expect(findFor('Retries happen in the webhook handler', ['webhook', 'retries'])).toBe('webhook');
    expect(findFor('…the handler retries…', ['webhook', 'retries'])).toBe('retries');
    expect(findFor('nothing here', ['ab', 'webhook'])).toBe('webhook');
    expect(findFor('anything', [])).toBe('');
  });
});
