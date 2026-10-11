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
    // A review of a PR of yours, which nobody asked you to review.
    expect(openingTab({ reviewRequested: false, state: 'open', autoReview: { ...auto('posted'), stage: 'kept', own: true } })).toBe('review');
    expect(openingTab({ reviewRequested: false, state: 'open', autoReview: { ...auto('reviewing'), own: true } })).toBe('description');
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
  });

  it('says what goes with it', () => {
    expect(agentsWords('drafted', 3)).toBe('with your draft review and its 3 comments');
    expect(agentsWords('drafted', 1)).toBe('with your draft review and its comment');
    expect(agentsWords('posted', null)).toBe('pointing to the agent’s review on the PR');
    expect(agentsWords('kept', null)).toBe('with the agent’s review as its text');
  });
});
