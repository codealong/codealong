import { describe, expect, it } from 'vitest';
import { chainFrom, countIn, countWords, findFor, findPattern, pageHits, plainOf, says, shownText } from './threadFind';
import type { TranscriptItem } from './types';

const item = (kind: TranscriptItem['kind'], text: string, extra: Partial<TranscriptItem> = {}): TranscriptItem => ({ at: 0, kind, text, failed: false, cursor: 0, n: 0, ...extra });

describe('findPattern', () => {
  it('finds the words in any case, a run of spaces matching any whitespace', () => {
    const p = findPattern('  Session   thread ')!;
    expect(countIn(p, 'the session thread, the SESSION\nTHREAD and the session  thread')).toBe(3);
    expect(says(p, 'session-thread')).toBe(false);
  });

  it('takes what it finds as written, not as a pattern', () => {
    const p = findPattern('a.b (c)* [d] $e^ {f} | \\g')!;
    expect(says(p, 'x a.b (c)* [d] $e^ {f} | \\g y')).toBe(true);
    expect(says(findPattern('a.b')!, 'axb')).toBe(false);
  });

  it('is nothing for nothing typed', () => {
    expect(findPattern('')).toBeNull();
    expect(findPattern('  \n ')).toBeNull();
  });

  it('can be used again and again', () => {
    const p = findPattern('one')!;
    expect(countIn(p, 'one one')).toBe(2);
    expect(says(p, 'one')).toBe(true);
    expect(says(p, 'one')).toBe(true);
    expect(countIn(p, 'one')).toBe(1);
  });
});

describe('plainOf', () => {
  it('reads Markdown as it shows', () => {
    expect(plainOf('## Done\n\nThe **fix** is in `lanes.ts`, see [the PR](https://x/1).\n> quoted ![shot](images/a.png)')).toBe('Done\n\nThe fix is in lanes.ts, see the PR.\nquoted  ');
  });

  it('keeps the underscores in a name', () => {
    expect(says(findPattern('snake_case_name')!, plainOf('call `snake_case_name` here'))).toBe(true);
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
  });

  it('reads a reply’s Codex directives as they’re drawn, and a prompt’s as written', () => {
    const block = ':::writing{variant="standard" id="1"}\nRead the status.\n:::';
    expect(shownText(item('agent', block))).toBe('Writing\nRead the status.');
    expect(shownText(item('you', block))).toBe(block);
  });

  it('leaves out what shows only once opened', () => {
    const p = findPattern('passed')!;
    expect(pageHits(p, [item('tool', 'Bash · `npm test`', { output: 'all passed' }), item('result', '', { output: 'passed' })])).toBe(0);
    expect(pageHits(p, [item('agent', 'It passed, and passed again.'), item('you', 'passed?')])).toBe(3);
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

describe('countWords', () => {
  const none = { at: null, total: 0, partial: false, reading: false, looking: false };
  it('counts from the top, so far while more is to read', () => {
    expect(countWords({ ...none, at: 2, total: 12 })).toBe('3 of 12');
    expect(countWords({ ...none, at: 2, total: 12, partial: true, reading: true })).toBe('3 of 12 so far');
    expect(countWords({ ...none, total: 5, partial: true, reading: true })).toBe('5 so far');
  });

  it('says when there are none, or it’s still looking', () => {
    expect(countWords(none)).toBe('No matches');
    expect(countWords({ ...none, partial: true, reading: true })).toBe('Looking…');
    expect(countWords({ ...none, partial: true })).toBe('No matches so far');
    expect(countWords({ ...none, at: 0, total: 4, looking: true })).toBe('Looking earlier…');
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
