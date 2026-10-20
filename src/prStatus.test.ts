import { describe, expect, it } from 'vitest';
import { agentApproves, approvedBefore, asksFor, chipWord, commentPreview, commentText, commentsToAsk, heldForAgent, mayApprove, nudgeKind, nudgeMerges, nudgeWords, prFlag, prPills, prReady, primaryWords, reviewedBefore, reviewMark, reviewersWords, withMerges, yourReview, yourVerdictStands, type PrFacts } from './prStatus';
import * as fx from './testing/fixtures';
import type { LastReview, PrComment, PrItem } from './types';

const checks = fx.checks;
const facts = (f: Partial<PrFacts> = {}): PrFacts => ({ state: 'open', checks: checks({ passed: 12 }), merge: 'blocked', base: 'main', ...f });
const item = (f: Partial<PrItem> = {}): PrItem => fx.prItem({ ...facts(), mine: true, reviewRequested: false, ...f });
const texts = (f: Partial<PrFacts>) => prPills(facts(f)).map((p) => [p.text, p.tone]);

describe('prPills', () => {
  it('folds into one pill when GitHub would merge it', () => {
    const ready = facts({ state: 'approved', merge: 'clean' });
    expect(prReady(ready)).toBe(true);
    expect(prPills(ready).map((p) => [p.text, p.tone, p.title])).toEqual([['Ready to merge', 'ready', 'Approved, 12 checks passed, no conflicts: GitHub can merge it']]);
    // A repo that requires no review.
    expect(prPills(facts({ merge: 'clean', checks: checks() }))[0].title).toBe('No conflicts: GitHub can merge it');
  });

  it('says Review requested while anyone is asked to review it, Awaiting review while nobody is', () => {
    expect(prPills(facts({}))[0]).toMatchObject({ text: 'Awaiting review', title: 'Not approved yet, and nobody is asked to review it' });
    expect(prPills(facts({ reviewersAsked: ['ana', 'platform'] }))[0]).toMatchObject({ text: 'Review requested', tone: 'plain', title: 'Waiting on a review from ana and platform' });
    expect(prPills(facts({ reviewRequested: true }))[0]).toMatchObject({ text: 'Review requested', title: 'Waiting on a review from you' });
    // A draft, or a review that's in, says so whoever is asked.
    expect(prPills(facts({ state: 'draft', reviewersAsked: ['ana'] }))[0].text).toBe('Draft');
    expect(prPills(facts({ state: 'changes', reviewersAsked: ['ana'] }))[0].text).toBe('Changes requested');
  });

  it('says Re-review where your review is asked for again after you gave one, ready to merge or not', () => {
    const head = 'c0ffee1aaaa';
    const yours = (state: LastReview['state'], commit = '1234567ffff'): Partial<PrFacts> => ({ headOid: head, lastReview: { state, commit, at: 5 } });
    expect(prPills(facts({ reviewRequested: true, reviewersAsked: ['ana'], ...yours('approved') }))[0]).toMatchObject({ text: 'Re-review', icon: 'repeat', title: 'Your review is asked for again. You approved 1234567, and it’s had commits since. Waiting on a review from you and ana.' });
    expect(prPills(facts({ reviewRequested: true, ...yours('commented', head) }))[0].title).toBe('Your review is asked for again. You commented on c0ffee1, the commit it’s at. Waiting on a review from you.');
    // Your approval of an earlier commit is likely what has it ready: Re-review goes first.
    expect(texts({ state: 'approved', merge: 'clean', reviewRequested: true, ...yours('approved') })).toEqual([['Re-review', 'plain'], ['Ready to merge', 'ready']]);
    expect(prPills(facts({ state: 'approved', merge: 'blocked', reviewRequested: true, ...yours('dismissed') }))[0].title).toMatch(/^Your review is asked for again\. Your review of 1234567 was dismissed\./);
    // Not asked of you again, it says what yours was where it says the PR's approved.
    expect(prPills(facts({ state: 'approved', merge: 'unknown', ...yours('approved', head) }))[0]).toMatchObject({ text: 'Approved', title: "Approved. GitHub hasn't said yet whether it can merge. You approved c0ffee1, the commit it’s at." });
    // Never on a PR of yours, nor one you hadn't reviewed.
    expect(prPills(facts({ reviewRequested: true, mine: true, ...yours('commented') }))[0].text).toBe('Review requested');
    expect(prPills(facts({ reviewRequested: true }))[0].text).toBe('Review requested');
  });

  it("doesn't call it ready on an approval and passing checks alone", () => {
    expect(texts({ state: 'approved', merge: 'unknown' })).toEqual([['Approved', 'pass'], ['12', 'pass']]);
    expect(texts({ state: 'approved', merge: 'conflicts', checks: checks() })).toEqual([['Approved', 'pass'], ['Conflicts', 'attn']]);
    expect(texts({ state: 'approved', merge: 'behind' })).toEqual([['Approved', 'pass'], ['12', 'pass'], ['Out of date', 'attn']]);
    // Clean, but a check is still running.
    expect(prReady(facts({ state: 'approved', merge: 'clean', checks: checks({ passed: 11, pending: 1 }) }))).toBe(false);
  });

  it('shows the review and the checks side by side, each in its own color', () => {
    expect(texts({ checks: checks({ passed: 3, failed: 2, failing: ['lint', 'e2e'] }) })).toEqual([['Awaiting review', 'plain'], ['2 failing', 'alert']]);
    expect(texts({ state: 'changes', checks: checks({ pending: 4 }) })).toEqual([['Changes requested', 'attn'], ['4 running', 'plain']]);
    expect(texts({ state: 'draft', checks: checks() })).toEqual([['Draft', 'quiet']]);
    const failing = prPills(facts({ checks: checks({ failed: 2, failing: ['lint', 'e2e'] }) }))[1];
    expect([failing.short, failing.title]).toEqual(['2', '2 checks failing: lint, e2e']);
    // Short of room, Conflicts keeps only its icon.
    expect(prPills(facts({ merge: 'conflicts' })).map((p) => p.short)).toEqual([null, null, '']);
  });

  it("says only merged or closed once it's done", () => {
    expect(texts({ state: 'merged', merge: 'unknown', checks: checks({ failed: 1 }) })).toEqual([['Merged', 'merged']]);
    expect(texts({ state: 'closed', merge: 'unknown' })).toEqual([['Closed', 'quiet']]);
  });
});

describe('prFlag', () => {
  it('flags a review asked of you, whoever wrote the PR', () => {
    expect(prFlag(item({ mine: false, reviewRequested: true }))).toBe('attn');
    expect(prFlag(item({ mine: false, reviewRequested: true, snoozedUntil: 5 }))).toBeNull();
  });

  it('flags your PRs that are ready or held up by something you can fix', () => {
    expect(prFlag(item({ state: 'approved', merge: 'clean' }))).toBe('ready');
    expect(prFlag(item({ checks: checks({ failed: 1 }) }))).toBe('alert');
    expect(prFlag(item({ state: 'changes' }))).toBe('attn');
    expect(prFlag(item({ state: 'approved', merge: 'conflicts' }))).toBe('attn');
    expect(prFlag(item({ state: 'approved', merge: 'behind' }))).toBe('attn');
  });

  it("leaves waiting PRs, other people's and finished ones alone", () => {
    expect(prFlag(item())).toBeNull();
    expect(prFlag(item({ state: 'approved', merge: 'unknown' }))).toBeNull();
    expect(prFlag(item({ mine: false, state: 'approved', merge: 'clean' }))).toBeNull();
    expect(prFlag(item({ mine: false, checks: checks({ failed: 3 }) }))).toBeNull();
    expect(prFlag(item({ state: 'merged', merge: 'unknown', reviewRequested: true }))).toBeNull();
  });
});

describe('mayApprove', () => {
  it('approves any open PR but your own, whether or not your review was asked for', () => {
    expect(mayApprove(item({ mine: false, reviewRequested: true }))).toBe(true);
    expect(mayApprove(item({ mine: false }))).toBe(true);
    expect(mayApprove(item({ mine: false, state: 'draft' }))).toBe(true);
    expect(mayApprove(item({ mine: true }))).toBe(false);
    expect(mayApprove(item({ mine: false, state: 'merged' }))).toBe(false);
    expect(mayApprove(item({ mine: false, state: 'closed' }))).toBe(false);
  });
});

describe('primaryWords', () => {
  const sessions = [
    { key: 'a', lane: '/r', pushes: 11 },
    { key: 'b', lane: '/r' },
  ];
  it('says why the primary session is the one, and where to pick another', () => {
    expect(primaryWords({ primary: 'a', primaryBy: 'pushed', sessions })).toBe('The PR’s primary session: when the PR is nudged, only this one is told what happens on it. It pushed to the PR’s branch most, 11 times. The PR’s menu picks another (Primary session).');
    expect(primaryWords({ primary: 'b', primaryBy: 'opened', sessions })).toMatch(/ It opened the PR, and no other session pushed to it more\. /);
    expect(primaryWords({ primary: 'b', primaryBy: 'picked', sessions })).toMatch(/ It was picked as the primary one\. /);
    expect(primaryWords({ primary: 'b', primaryBy: 'started', sessions })).toMatch(/ so it’s the one that started first\. /);
    // An engine from before says no why.
    expect(primaryWords({ primary: 'b', sessions })).toBe('The PR’s primary session: when the PR is nudged, only this one is told what happens on it. The PR’s menu picks another (Primary session).');
  });
});

describe('nudgeWords', () => {
  it('explains blocked notifications for flagged primaries even when stopped or switched off', () => {
    for (const on of [true, false]) for (const runs of [true, false]) {
      const primary = { title: 'Atlas build', runs, stopped: 'resumes' as const, blocked: true };
      const words = nudgeWords(item({ state: 'approved' }), on, primary);
      expect(words).toContain('was flagged by its provider. PR notifications are blocked in Settings › Automation.');
      expect(words).not.toMatch(/is sent a message|resumed out of sight|turning this on tells/);
      expect(nudgeWords(item({}), true, { ...primary, blocked: false })).not.toContain('blocked');
    }
  });
  const made = { title: 'marketing email broadcast', runs: true };
  it('names the one session that is told, and says when nobody is', () => {
    expect(nudgeWords(item({}), true, made)).toMatch(/^On for this PR\. “marketing email broadcast”, its primary session, is sent a message/);
    expect(nudgeWords(item({ nudgeByDefault: true }), true, made)).toMatch(/^On for every PR of yours, by Settings\. “marketing/);
    expect(nudgeWords(item({}), true, { ...made, runs: false })).toBe('On for this PR. “marketing email broadcast”, its primary session, isn’t running here, so nobody is told until it is.');
    expect(nudgeWords(item({}), true, null)).toBe('On for this PR. No session of it is known here, so nobody is told.');
  });
  it('says what turning it on would do, an approval it has already first', () => {
    expect(nudgeWords(item({ state: 'approved' }), false, made)).toBe('Off for this PR. It’s approved already: turning this on tells “marketing email broadcast” that now, then of what happens on it next.');
    expect(nudgeWords(item({}), false, null)).toBe('Off for this PR. Turned on, its primary session is told of reviews, comments, an approval, a merge, failing checks and conflicts.');
  });
  it('says only what Settings has nudges tell of', () => {
    expect(nudgeWords(item({}), true, made, 'feedback')).toBe('On for this PR. “marketing email broadcast”, its primary session, is sent a message as one of yours when someone else asks for changes or comments, never for an approval or a merge.');
    expect(nudgeWords(item({}), true, made, 'feedback-and-checks')).toMatch(/asks for changes or comments and when its checks fail or its branch conflicts with its base, never for an approval or a merge\.$/);
    // An approval it has already isn't told either.
    expect(nudgeWords(item({ state: 'approved' }), false, made, 'feedback')).toBe('Off for this PR. Turned on, “marketing email broadcast” is told of comments and requests for changes, never an approval or a merge.');
    expect(nudgeWords(item({}), true, made, 'feedback-checks-and-merges')).toMatch(/its branch conflicts with its base, and when it’s merged, never for an approval\.$/);
    expect(nudgeWords(item({}), false, made, 'feedback-and-merges')).toBe('Off for this PR. Turned on, “marketing email broadcast” is told of comments, requests for changes and a merge, never an approval.');
  });
  it('resumes a primary that stopped for what asks for work, not for a merge', () => {
    expect(nudgeWords(item({}), true, { ...made, runs: false, stopped: 'resumes' }, 'feedback-and-merges')).toMatch(/resumed out of sight to be told when someone else asks for changes or comments, never for an approval or a merge\.$/);
  });
  it('says a primary that stopped is resumed for what asks for work, unless another of its sessions runs', () => {
    const stopped = { ...made, runs: false };
    expect(nudgeWords(item({}), true, { ...stopped, stopped: 'resumes' })).toBe(
      'On for this PR. “marketing email broadcast”, its primary session, isn’t running here, so it’s resumed out of sight to be told when someone else asks for changes or comments and when its checks fail or its branch conflicts with its base, never for an approval or a merge.',
    );
    expect(nudgeWords(item({}), true, { ...stopped, stopped: 'resumes' }, 'feedback')).toMatch(/resumed out of sight to be told when someone else asks for changes or comments, never for an approval or a merge\.$/);
    expect(nudgeWords(item({}), true, { ...stopped, stopped: 'beside' })).toBe('On for this PR. “marketing email broadcast”, its primary session, isn’t running here, and another of the PR’s sessions is, so it isn’t resumed beside that one: nobody is told until it runs.');
    expect(nudgeWords(item({}), true, { ...stopped, stopped: 'never' })).toMatch(/isn’t running here, so nobody is told until it is\.$/);
  });
});

describe('withMerges', () => {
  it('adds a merge to what Settings picked, or takes it away, and leaves everything as it is', () => {
    expect(withMerges('feedback', true)).toBe('feedback-and-merges');
    expect(withMerges('feedback-and-checks', true)).toBe('feedback-checks-and-merges');
    expect(withMerges('feedback-checks-and-merges', false)).toBe('feedback-and-checks');
    expect(withMerges('everything', false)).toBe('everything');
    expect([nudgeKind('feedback-and-merges'), nudgeMerges('feedback-and-merges'), nudgeMerges('feedback'), nudgeMerges('everything')]).toEqual(['feedback', true, false, true]);
  });
});

describe('review comments', () => {
  const c = (id: string, extra: Partial<PrComment> = {}): PrComment => ({ id, author: 'ana', body: 'x', at: null, url: null, path: null, resolved: false, mine: false, ...extra });
  const all = [c('a'), c('mine', { mine: true }), c('done', { resolved: true }), c('b')];

  it('asks about others’ comments on open threads only, and marks the likely ones', () => {
    expect(commentsToAsk(all).map((x) => x.id)).toEqual(['a', 'b']);
    expect([...asksFor(all, { a: 0.9, b: 0.4, mine: 0.99, done: 0.99 })]).toEqual(['a']);
  });

  it('shows a comment as plain text on one line', () => {
    expect(commentPreview('**Please** add a [test](http://x)\n\n```ts\ncode\n```')).toBe('Please add a test [code]');
    expect(commentPreview('a'.repeat(300), 10)).toBe('aaaaaaaaa…');
    expect(commentPreview('<details><summary>Preview</summary>\n\nDeployed <img src="x"> here</details>')).toBe('Preview Deployed here');
  });

  it('shows a comment as the Markdown view draws it, without GitHub’s hidden marks or HTML', () => {
    const posted = 'Looks right.\n\n<sub>An agent’s review of 1111111, posted by Codealong.</sub>\n<!-- codealong-review o/r#7 1111111aaaa -->';
    expect(commentText(posted)).toBe('Looks right.\n\n*An agent’s review of 1111111, posted by Codealong.*');
    expect(commentText('<p align="center">Hi<br/>there</p>\n\n\n\nBye')).toBe('Hi\nthere\n\nBye');
    // A type in a comment isn't a tag.
    expect(commentText('Use `Vec<String>` and `Option<T>`.')).toBe('Use `Vec<String>` and `Option<T>`.');
    // Nor is anything in its code: spans and fenced blocks keep every character.
    expect(commentText('Wrap it in `<div>`, as `Array<B>` does.')).toBe('Wrap it in `<div>`, as `Array<B>` does.');
    const fenced = 'Like this:\n\n```html\n<div class="a">\n  <!-- keep -->\n  <p>Hi</p>\n</div>\n```\n\n<p>Done</p>';
    expect(commentText(fenced)).toBe('Like this:\n\n```html\n<div class="a">\n  <!-- keep -->\n  <p>Hi</p>\n</div>\n```\n\nDone');
    expect(commentText('Unclosed:\n```\n<b>x</b>')).toBe('Unclosed:\n```\n<b>x</b>');
    // A fence inside code doesn't end the block: only one at a line's start does.
    const quoted = '```js\nconst fence = "```";\nconst element = "<div>";\n```\n\n<p>After</p>';
    expect(commentText(quoted)).toBe('```js\nconst fence = "```";\nconst element = "<div>";\n```\n\nAfter');
    // Code keeps its blank lines; the prose around it has its own run together.
    expect(commentText('a\n\n\n\n```\nx\n\n\n\ny\n```')).toBe('a\n\n```\nx\n\n\n\ny\n```');
    // A backtick inside a hidden comment opens no code, and `<sub>` around code is still small print.
    expect(commentText('Text.\n<!-- hidden `metadata` -->')).toBe('Text.');
    expect(commentText('Fine.\n\n<sub>Reviewed at `abc123`.</sub>')).toBe('Fine.\n\n*Reviewed at `abc123`.*');
    // As the Markdown view reads code: a fence indented in a list, closed by a longer fence or
    // one after a backtick in its info string.
    const listed = '- Like this:\n  ```html\n  <div>x</div>\n\n  <!-- keep -->\n  ```\n- <b>Next</b>';
    expect(commentText(listed)).toBe('- Like this:\n  ```html\n  <div>x</div>\n\n  <!-- keep -->\n  ```\n- Next');
    expect(commentText('```\ncode\n````\n\n<p>After</p>\n<!-- hidden -->')).toBe('```\ncode\n````\n\nAfter');
    expect(commentText('```js`x\n<b>\n```\n<p>After</p>')).toBe('```js`x\n<b>\n```\nAfter');
    // A `</sub>` in code ends nothing, nor a fence in a hidden comment.
    expect(commentText('<sub>Use `</sub><div>` here</sub>')).toBe('*Use `</sub><div>` here*');
    expect(commentText('Text\n<!--\n```\n-->\nMore')).toBe('Text\n\nMore');
  });
});

describe('chipWord', () => {
  it('says what the PR’s header leads with', () => {
    expect(chipWord(facts({ state: 'approved', merge: 'clean' }))).toBe('Ready to merge');
    expect(chipWord(facts({ state: 'approved', merge: 'blocked' }))).toBe('Approved');
    expect(chipWord(facts({ reviewRequested: true }))).toBe('Review requested');
    expect(chipWord(facts({ reviewRequested: false, reviewersAsked: ['ana'] }))).toBe('Review requested');
    expect(chipWord(facts({ reviewRequested: false }))).toBe('Awaiting review');
    expect(chipWord(facts({ state: 'changes' }))).toBe('Changes requested');
    expect(chipWord(facts({ state: 'draft' }))).toBe('Draft');
    expect(chipWord(facts({ state: 'merged' }))).toBe('Merged');
    expect(chipWord(facts({ state: 'closed' }))).toBe('Closed');
  });

  it('says Open for a worktree’s PR, which doesn’t say whose review it waits on', () => {
    expect(chipWord(fx.pr({ state: 'open' }))).toBe('Open');
    expect(chipWord(fx.pr({ state: 'approved', merge: 'blocked' }))).toBe('Approved');
  });
});

describe('reviewMark', () => {
  const key = (f: Partial<PrItem>) => reviewMark(item(f))?.key ?? null;

  it('marks what the chip’s color can’t tell apart', () => {
    expect(key({ state: 'approved' })).toBe('approved');
    expect(key({ state: 'changes' })).toBe('changes');
    expect(key({ state: 'open' })).toBe(null);
  });

  it('puts a review asked of you first, unless it’s snoozed, and the agent’s before that', () => {
    expect(reviewMark(item({ state: 'approved', mine: false, reviewRequested: true, requestedBy: 'gwen' }))).toMatchObject({ key: 'asked', tone: 'attn', title: 'Your review is asked for, by gwen' });
    expect(key({ state: 'approved', mine: false, reviewRequested: true, snoozedUntil: Date.now() + 60_000 })).toBe('approved');
    expect(key({ mine: false, reviewRequested: true, autoReview: { stage: 'reviewing', head: 'abc', at: 0, rounds: 1, holds: true } })).toBe('agent');
    // Asked again after you gave yours.
    expect(reviewMark(item({ mine: false, reviewRequested: true, requestedBy: 'gwen', headOid: 'c0ffee1aaaa', lastReview: { state: 'approved', commit: '1234567ffff', at: 5 } }))?.title).toBe('Your review is asked for again, by gwen. You approved 1234567, and it’s had commits since.');
  });

  it('knows whether your last review is of the commit the PR’s at', () => {
    const at = (lastReview: LastReview | undefined, more: Partial<PrFacts> = {}) => ({ headOid: 'c0ffee1aaaa', lastReview, mine: false, ...more });
    const of = (state: LastReview['state'], commit: string | null): LastReview => ({ state, commit, at: 5 });
    expect(yourReview(at(of('approved', 'c0ffee1aaaa')))?.current).toBe(true);
    expect(yourReview(at(of('approved', '1234567ffff')))?.current).toBe(false);
    expect(yourReview(at(of('dismissed', 'c0ffee1aaaa')))?.current).toBe(false);
    expect(yourReview(at(of('approved', null)))?.current).toBe(false);
    expect(yourReview(at(undefined))).toBe(null);
    expect(yourReview(at(of('commented', 'c0ffee1aaaa'), { mine: true }))).toBe(null);
    // What's compared since: the commit of an earlier one, of whichever kind.
    expect(reviewedBefore(at(of('dismissed', '1234567ffff')))).toBe('1234567ffff');
    expect(reviewedBefore(at(of('approved', 'c0ffee1aaaa')))).toBe(null);
    expect(reviewedBefore(at(of('approved', '1234567ffff'), { headOid: undefined }))).toBe(null);
    // An approval or a request for changes of the commit it's at says what you'd do; a comment doesn't.
    expect([yourVerdictStands(at(of('approved', 'c0ffee1aaaa'))), yourVerdictStands(at(of('changes', 'c0ffee1aaaa'))), yourVerdictStands(at(of('commented', 'c0ffee1aaaa'))), yourVerdictStands(at(of('approved', '1234567ffff')))]).toEqual([true, true, false, false]);
  });

  it('marks nothing on a draft or a PR that’s done', () => {
    expect(key({ state: 'draft', reviewRequested: true })).toBe(null);
    expect(key({ state: 'merged' })).toBe(null);
    expect(key({ state: 'closed' })).toBe(null);
  });

  it('goes by the state alone for a worktree’s PR', () => {
    expect(reviewMark(fx.pr({ state: 'approved' }))?.key).toBe('approved');
    expect(reviewMark(fx.pr({ state: 'open' }))).toBe(null);
  });
});

describe('reviewersWords', () => {
  it('says whose reviews an open PR waits on, you first', () => {
    expect(reviewersWords(item({ mine: false, reviewRequested: true, reviewersAsked: ['ana', 'platform'] }))).toBe('Waiting on a review from you, ana and platform');
    expect(reviewersWords(item({ reviewersAsked: ['ana'] }))).toBe('Waiting on a review from ana');
    expect(reviewersWords(item({ mine: false, reviewRequested: true }))).toBe('Waiting on a review from you');
  });

  it('says nothing when it waits on nobody’s, or once it’s done', () => {
    expect(reviewersWords(item({}))).toBe(null);
    expect(reviewersWords(item({ state: 'merged', reviewersAsked: ['ana'] }))).toBe(null);
    expect(reviewersWords(fx.pr({ state: 'open' }))).toBe(null);
  });
});

describe('heldForAgent', () => {
  const asked = (autoReview: PrItem['autoReview']) => item({ mine: false, reviewRequested: true, autoReview });
  it('keeps a review the agent is still on out of what waits on you, and says so', () => {
    const held = asked({ stage: 'reviewing', head: '309811a', at: 0, rounds: 1, holds: true });
    expect(heldForAgent(held)).toBe(true);
    expect(prFlag(held)).toBe(null);
    expect(prPills(held)[0]).toMatchObject({ text: 'Agent reviewing', tone: 'plain' });
  });
  it('hands it to you once the agent is done', () => {
    const done = asked({ stage: 'posted', head: '309811a', at: 0, rounds: 1 });
    expect(heldForAgent(done)).toBe(false);
    expect(prFlag(done)).toBe('attn');
    expect(prPills(done)[0].text).toBe('Review requested');
    expect(prFlag(asked(undefined))).toBe('attn');
  });
});

describe('agentApproves', () => {
  const asked = (autoReview: PrItem['autoReview'], more: Partial<PrItem> = {}) => item({ mine: false, reviewRequested: true, headOid: '309811aaaabbbb', autoReview, ...more });
  const approving = { stage: 'posted' as const, head: '309811aaaabbbb', at: 0, rounds: 1, suggests: 'approve' as const, approves: '309811aaaabbbb' };
  it('says a PR the agent approves waits on your sign-off, in its pill, its chip and its mark', () => {
    const pr = asked(approving);
    expect(agentApproves(pr)).toBe(true);
    expect(prFlag(pr)).toBe('attn');
    expect(prPills(pr)[0]).toMatchObject({ text: 'Agent approves', tone: 'pass', icon: null, title: 'Codealong’s agent approves 309811a: it waits on your sign-off' });
    expect(chipWord(pr)).toBe('Agent approves');
    expect(reviewMark({ ...pr, requestedBy: 'gwen' })?.title).toBe('Your sign-off is asked for, by gwen: Codealong’s agent approves 309811a');
  });
  it('says a PR held for the commits after the approved one is being reviewed again, not yours yet', () => {
    const moved = asked({ ...approving, stage: 'reviewing', head: '4444444cccc', suggests: null, holds: true }, { headOid: '4444444cccc' });
    expect(agentApproves(moved)).toBe(false);
    expect(approvedBefore(moved)).toBe('309811a');
    expect(prFlag(moved)).toBe(null);
    expect(prPills(moved)[0]).toMatchObject({ text: 'Agent re-reviewing', tone: 'plain', icon: 'hourglass', title: 'Codealong’s agent approved 309811a and reviews the commits since: it waits on your sign-off once the agent approves them' });
    // The chip, which shows only the pill's words, says so on its mark.
    expect(reviewMark(moved)).toMatchObject({ key: 'agent', title: 'Codealong’s agent approved 309811a and reviews the commits since: it waits on your sign-off once the agent approves them' });
    // So does one asked for changes before, whose request GitHub keeps.
    expect(prPills({ ...moved, state: 'changes' })[0].text).toBe('Agent re-reviewing');
    // Their review didn't get done: it's yours to review, with no approval of what it's at.
    const missed = asked({ ...approving, stage: 'failed', head: '4444444cccc', suggests: null }, { headOid: '4444444cccc' });
    expect([agentApproves(missed), approvedBefore(missed), prFlag(missed)]).toEqual([false, null, 'attn']);
    expect(prPills(missed)[0].text).toBe('Review requested');
  });
  it('says nothing of a sign-off before the agent approves, or once it’s not asked of you', () => {
    expect(agentApproves(asked({ ...approving, approves: null, suggests: 'request-changes' }))).toBe(false);
    expect(prPills(asked({ ...approving, approves: null, suggests: 'request-changes' }))[0].text).toBe('Review requested');
    expect(agentApproves(asked({ ...approving, holds: true }))).toBe(false);
    expect(agentApproves(asked(approving, { reviewRequested: false }))).toBe(false);
    expect(agentApproves(asked(approving, { state: 'merged' }))).toBe(false);
    expect(approvedBefore(asked({ ...approving, head: '4444444cccc', holds: true }, { headOid: '4444444cccc', state: 'merged' }))).toBe(null);
  });
  it('says where the agent stands over GitHub’s approval, and beside Ready to merge', () => {
    expect(prPills(asked(approving, { state: 'approved' }))[0].text).toBe('Agent approves');
    const clean = { merge: 'clean' as const, checks: { passed: 3, failed: 0, pending: 0, skipped: 0, failing: [] } };
    expect(prPills(asked(approving, { state: 'approved', ...clean })).map((p) => p.text)).toEqual(['Agent approves', 'Ready to merge']);
    const moved = asked({ ...approving, stage: 'reviewing', head: '4444444cccc', suggests: null, holds: true }, { headOid: '4444444cccc', state: 'approved', ...clean });
    expect(prPills(moved).map((p) => p.text)).toEqual(['Agent re-reviewing', 'Ready to merge']);
    expect(prPills(asked({ ...approving, approves: null, stage: 'reviewing', holds: true }, clean)).map((p) => p.text)).toEqual(['Agent reviewing', 'Ready to merge']);
    // Without the agent, Ready to merge says it all, and Approved is GitHub's.
    expect(prPills(asked(undefined, { state: 'approved', ...clean })).map((p) => p.text)).toEqual(['Ready to merge']);
    expect(prPills(asked(undefined, { state: 'approved' }))[0].text).toBe('Approved');
  });
  it('says the approval over a request for changes it submitted for you before, which GitHub keeps until you approve', () => {
    const pr = asked(approving, { state: 'changes' });
    expect(prPills(pr)[0]).toMatchObject({ text: 'Agent approves', tone: 'pass' });
    expect(chipWord(pr)).toBe('Agent approves');
    expect(prPills(asked({ ...approving, approves: null }, { state: 'changes' }))[0].text).toBe('Changes requested');
  });
});
