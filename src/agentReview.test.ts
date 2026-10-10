import { describe, expect, it } from 'vitest';
import { excerptAt, lineLink, openingTab } from './agentReview';

/** Two hunks of one file's patch, as GitHub gives it. */
const PATCH = ['@@ -1,4 +1,5 @@', ' fn a() {', '-    old();', '+    new();', '+    more();', ' }', ' ', '@@ -20,3 +21,3 @@ fn b() {', ' let x = 1;', '-let y = 2;', '+let y = 3;', ' x + y', '\\ No newline at end of file'].join('\n');

const shown = (lines: ReturnType<typeof excerptAt>) => lines.map((l) => `${l.n ?? '-'}${l.at ? '*' : ' '}${l.kind} ${l.text}`);

describe('excerptAt', () => {
  it('gives the lines around a comment’s line, the removed ones among them, the line marked', () => {
    expect(shown(excerptAt(PATCH, 2))).toEqual(['1 ctx fn a() {', '- del     old();', '2*add     new();', '3 add     more();', '4 ctx }']);
  });
  it('keeps to the line’s own hunk', () => {
    expect(shown(excerptAt(PATCH, 22, 5))).toEqual(['21 ctx let x = 1;', '- del let y = 2;', '22*add let y = 3;', '23 ctx x + y']);
    expect(shown(excerptAt(PATCH, 5, 1))).toEqual(['4 ctx }', '5*ctx ']);
  });
  it('gives nothing for a line the patch doesn’t show', () => {
    expect(excerptAt(PATCH, 10)).toEqual([]);
    expect(excerptAt('', 1)).toEqual([]);
  });
});

describe('lineLink', () => {
  it('links a line of a file at the commit reviewed', () => {
    expect(lineLink('o/r', 'abc123', 'docs/new file.md', 7)).toBe('https://github.com/o/r/blob/abc123/docs/new%20file.md#L7');
  });
});

describe('openingTab', () => {
  const auto = (stage: 'drafted' | 'reviewing' | 'posted') => ({ stage, head: 'abc', at: 0, rounds: 1 });
  it('opens on the review when the agent’s is there to read and the PR waits on yours', () => {
    expect(openingTab({ reviewRequested: true, state: 'open', autoReview: auto('drafted') })).toBe('review');
    expect(openingTab({ reviewRequested: true, state: 'open', autoReview: auto('posted') })).toBe('review');
    expect(openingTab({ reviewRequested: true, state: 'open', autoReview: auto('reviewing') })).toBe('description');
    expect(openingTab({ reviewRequested: false, state: 'open', autoReview: auto('drafted') })).toBe('description');
    expect(openingTab({ reviewRequested: true, state: 'merged', autoReview: auto('drafted') })).toBe('description');
    expect(openingTab({ reviewRequested: true, state: 'open', autoReview: undefined })).toBe('description');
  });
});
