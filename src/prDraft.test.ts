import { describe, expect, it, vi } from 'vitest';
import { prDraft } from './api';
import { changeDraft, draftShown, draftStamp, readAgain, reviewedWords, saysSomething, submittedWord, verdictWords } from './prDraft';
import { reloadWindow } from './reload';
import type { Draft, YourReview } from './types';

vi.mock('./api', () => ({ prDraft: vi.fn(), refresh: vi.fn(() => Promise.resolve()), refreshUsage: vi.fn(() => Promise.resolve()) }));

const draft = (body: string, count: number): Draft => ({ id: 'PRR_1', body, commit: 'abc', url: null, comments: [], count });

/** Reads of your review that answer when told to, in the order they were asked. */
function heldReads() {
  const answers: ((r: YourReview) => void)[] = [];
  vi.mocked(prDraft).mockImplementation(() => new Promise((resolve) => answers.push(resolve)));
  return answers;
}

/** The draft's text as the window holds it now, if it holds one. */
function heldBody(key: string): string | null {
  let body: string | null = null;
  changeDraft(key, (d) => {
    body = d.body;
    return d;
  });
  return body;
}

const settled = () => new Promise((resolve) => setTimeout(resolve, 0));

describe('prDraft', () => {
  it('reads a PR again when it changes on GitHub or its agent review moves', () => {
    const pr = { updatedAt: 5, autoReview: undefined };
    expect(draftStamp(pr)).toBe('5|');
    const auto = { stage: 'drafted', head: 'abc', at: 9, rounds: 1, behind: false } as const;
    expect(draftStamp({ updatedAt: 5, autoReview: auto })).toBe('5|drafted@9:1');
    expect(draftStamp({ updatedAt: 5, autoReview: { ...auto, stage: 'reviewing', at: 12, rounds: 2 } })).not.toBe(draftStamp({ updatedAt: 5, autoReview: auto }));
  });

  it('keeps the latest read of a review when an earlier one for the same changes answers after it', async () => {
    const answers = heldReads();
    const pr = { key: 'o/r#1', repoSlug: 'o/r', number: 1, updatedAt: 5, autoReview: undefined };
    readAgain(pr);
    readAgain(pr);
    answers[1]({ draft: draft('Newer.', 0), submitted: null });
    await settled();
    answers[0]({ draft: draft('Older.', 0), submitted: null });
    await settled();
    expect(heldBody(pr.key)).toBe('Newer.');
  });

  it('takes no read asked before a reload, which reads it again as it is next wanted', async () => {
    const answers = heldReads();
    const pr = { key: 'o/r#2', repoSlug: 'o/r', number: 2, updatedAt: 5, autoReview: undefined };
    readAgain(pr);
    reloadWindow({ usage: false });
    answers[0]({ draft: draft('From before.', 0), submitted: null });
    await settled();
    expect(heldBody(pr.key)).toBeNull();
  });

  it('shows the line naming the agent it was reviewed with in italics', () => {
    expect(draftShown('Hold it.\n\n<sub>Reviewed 322941c with Claude Code · drafted via Codealong</sub>')).toBe('Hold it.\n\n*Reviewed 322941c with Claude Code · drafted via Codealong*');
    expect(draftShown('Plain.')).toBe('Plain.');
  });

  it('a comment or a request for changes needs something to say, which a draft can hold', () => {
    expect(saysSomething('approve', '', null)).toBe(true);
    expect(saysSomething('comment', '', null)).toBe(false);
    expect(saysSomething('comment', ' x ', null)).toBe(true);
    expect(saysSomething('request-changes', '', draft('', 2))).toBe(true);
    expect(saysSomething('comment', '', draft('Hold it.', 0))).toBe(true);
    expect(saysSomething('comment', '', draft(' ', 0))).toBe(false);
  });

  it('says a verdict submits your draft, with its comments', () => {
    expect(reviewedWords('approve', 12, { draft: null })).toBe('Approved #12.');
    expect(reviewedWords('approve', 12, null)).toBe('Approved #12.');
    expect(reviewedWords('comment', 12, { draft: 4 })).toBe('Commented on #12, submitting your draft review and its 4 comments.');
    expect(reviewedWords('request-changes', 12, { draft: 0 })).toBe('Requested changes on #12, submitting your draft review.');
    expect(verdictWords('approve', null)).toBe(null);
    expect(verdictWords('approve', draft('x', 1))).toBe('Approve, submitting your draft review and its 1 comment');
    expect(submittedWord('APPROVED')).toBe('approving it');
    expect(submittedWord('CHANGES_REQUESTED')).toBe('asking for changes');
  });
});
