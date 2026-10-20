import { describe, expect, it } from 'vitest';
import { agentsVerdict, agentsWords, excerptAt, lineLink, openingTab } from './agentReview';

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
  it('gives the lines around a line of the file as it was, for a thread on a removed line', () => {
    // Line 2 of the old file is the removed `old();`, not the new file's `new();`, and the lines
    // are numbered as the old file has them.
    expect(shown(excerptAt(PATCH, 2, 2, true))).toEqual(['1 ctx fn a() {', '2*del     old();', '- add     new();', '- add     more();', '3 ctx }', '4 ctx ']);
    // The second hunk starts at line 20 of the old file and 21 of the new: its removed
    // `let y = 2;` is line 21 before, and the line after it 22.
    expect(shown(excerptAt(PATCH, 21, 1, true))).toEqual(['20 ctx let x = 1;', '21*del let y = 2;', '- add let y = 3;', '22 ctx x + y']);
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
    expect(openingTab({ reviewRequested: true, state: 'open', autoReview: auto('drafted'), mine: false })).toBe('review');
    expect(openingTab({ reviewRequested: true, state: 'open', autoReview: auto('posted'), mine: false })).toBe('review');
    expect(openingTab({ reviewRequested: true, state: 'open', autoReview: auto('reviewing'), mine: false })).toBe('description');
    expect(openingTab({ reviewRequested: false, state: 'open', autoReview: auto('drafted'), mine: false })).toBe('description');
    expect(openingTab({ reviewRequested: true, state: 'merged', autoReview: auto('drafted'), mine: false })).toBe('description');
    expect(openingTab({ reviewRequested: true, state: 'open', autoReview: undefined, mine: false })).toBe('description');
    // A review of a PR of yours, which nobody asked you to review.
    expect(openingTab({ reviewRequested: false, state: 'open', autoReview: { ...auto('posted'), stage: 'kept', own: true }, mine: true })).toBe('review');
    expect(openingTab({ reviewRequested: false, state: 'open', autoReview: { ...auto('reviewing'), own: true }, mine: true })).toBe('description');
  });
  it('opens a PR of yours on its reviews when someone asked for changes to it', () => {
    expect(openingTab({ reviewRequested: false, state: 'changes', autoReview: undefined, mine: true })).toBe('review');
    expect(openingTab({ reviewRequested: false, state: 'approved', autoReview: undefined, mine: true })).toBe('description');
    // Someone else's: what you'd look at first is still what it does.
    expect(openingTab({ reviewRequested: false, state: 'changes', autoReview: undefined, mine: false })).toBe('description');
  });
});

describe('agentsVerdict', () => {
  const pr = (a: Record<string, unknown>, more: Record<string, unknown> = {}) => ({ state: 'open' as const, mine: false, reviewRequested: true, headOid: 'abc', autoReview: { stage: 'posted' as const, head: 'abc', at: 0, rounds: 1, suggests: 'request-changes' as const, ...a }, ...more });
  it('is what the agent suggests asking or saying, while its review of the commit the PR is at waits to be submitted', () => {
    expect(agentsVerdict(pr({}), false)).toBe('request-changes');
    expect(agentsVerdict(pr({ suggests: 'comment', stage: 'kept' }), false)).toBe('comment');
    expect(agentsVerdict(pr({ stage: 'drafted' }), true)).toBe('request-changes');
  });

  it('is nothing for an approval, a review of an earlier commit, one submitted, one under way, or a PR of yours, closed or reviewed', () => {
    expect(agentsVerdict(pr({ suggests: 'approve' }), false)).toBeNull();
    expect(agentsVerdict(pr({ head: 'old' }), false)).toBeNull();
    expect(agentsVerdict(pr({ submitted: 'request-changes' }), false)).toBeNull();
    expect(agentsVerdict(pr({ stage: 'reviewing' }), false)).toBeNull();
    expect(agentsVerdict(pr({ suggests: null }), false)).toBeNull();
    expect(agentsVerdict(pr({ own: true }), false)).toBeNull();
    expect(agentsVerdict(pr({}, { mine: true }), false)).toBeNull();
    expect(agentsVerdict(pr({}, { state: 'merged' }), false)).toBeNull();
    expect(agentsVerdict(pr({}, { headOid: undefined }), false)).toBeNull();
    // A review of yours settled the request, or the agent's draft went from GitHub.
    expect(agentsVerdict(pr({}, { reviewRequested: false }), false)).toBeNull();
    expect(agentsVerdict(pr({ stage: 'drafted' }), false)).toBeNull();
    // Asked again, after you approved the commit it's at: your verdict stands over the agent's.
    expect(agentsVerdict(pr({}, { lastReview: { state: 'approved', commit: 'abc', at: 5 } }), false)).toBeNull();
    // A comment of yours, or a review of an earlier commit, leaves it.
    expect(agentsVerdict(pr({}, { lastReview: { state: 'commented', commit: 'abc', at: 5 } }), false)).toBe('request-changes');
    expect(agentsVerdict(pr({}, { lastReview: { state: 'approved', commit: 'old', at: 5 } }), false)).toBe('request-changes');
  });

  it('says what goes with it', () => {
    expect(agentsWords('drafted', 3)).toBe('with your draft review and its 3 comments');
    expect(agentsWords('drafted', 1)).toBe('with your draft review and its comment');
    expect(agentsWords('posted', null)).toBe('pointing to the agent’s review on the PR');
    expect(agentsWords('kept', null)).toBe('with the agent’s review as its text');
  });
});
