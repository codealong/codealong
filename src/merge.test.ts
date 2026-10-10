import { describe, expect, it } from 'vitest';
import { boxKey, commitText, confirmLabel, deletesWith, doneWords, mainMode, methodsByRepo, modeLabel, offers, opFor, pickMethod, rereads, standing, type MergeForm } from './merge';
import * as fx from './testing/fixtures';
import type { Checks, MergeBox } from './types';

const box = (over: Partial<MergeBox> = {}): MergeBox => ({
  id: 'PR_1',
  head: 'abc',
  branch: 'teams',
  state: 'OPEN',
  draft: false,
  mergeable: 'MERGEABLE',
  mergeState: 'CLEAN',
  reviewDecision: 'APPROVED',
  canMerge: true,
  methods: ['merge', 'squash', 'rebase'],
  defaultMethod: 'squash',
  mergeText: { title: 'Merge pull request #7 from ana/teams', body: 'Add teams' },
  squashText: { title: 'Add teams (#7)', body: '* Add teams' },
  emails: ['ana@example.com', 'ana@work.example'],
  defaultEmail: 'ana@example.com',
  canReady: true,
  canBypass: false,
  canAuto: false,
  canStopAuto: false,
  auto: null,
  canUpdateBranch: false,
  queue: false,
  queued: null,
  deletesBranch: false,
  branchRef: 'REF_1',
  mergedBy: null,
  ...over,
});

const checks = (over: Partial<Checks> = {}): Checks => ({ passed: 11, failed: 0, pending: 0, skipped: 0, failing: [], ...over });
const pr = (over: Partial<Checks> = {}) => ({ base: 'main', checks: checks(over), repoSlug: 'acme/app' });

describe('standing', () => {
  it('says a clean PR is ready, with what made it so', () => {
    expect(standing(box(), pr())).toEqual({ tone: 'ok', says: 'Ready to merge', more: 'Approved, 11 checks passed.' });
    expect(standing(box({ reviewDecision: null }), pr({ passed: 0 }))).toEqual({ tone: 'ok', says: 'Ready to merge', more: undefined });
  });

  it('says what holds a blocked PR back', () => {
    expect(standing(box({ mergeState: 'BLOCKED', reviewDecision: 'REVIEW_REQUIRED' }), pr({ pending: 2 }))).toEqual({ tone: 'wait', says: 'Merging is blocked', more: 'It needs an approving review, checks are still running.' });
    expect(standing(box({ mergeState: 'BLOCKED', reviewDecision: 'CHANGES_REQUESTED' }), pr({ failed: 1 })).tone).toBe('bad');
    expect(standing(box({ mergeState: 'BLOCKED', reviewDecision: 'APPROVED' }), pr()).more).toBe('The repo’s rules hold it back.');
  });

  it('says checks the repo doesn’t require don’t hold it back', () => {
    expect(standing(box({ mergeState: 'UNSTABLE' }), pr({ failed: 2, pending: 1 })).more).toBe('2 checks failed, 1 running, which the repo doesn’t require.');
  });

  it('says conflicts, a branch behind, a draft, the queue and auto-merge', () => {
    expect(standing(box({ mergeable: 'CONFLICTING', mergeState: 'DIRTY' }), pr())).toMatchObject({ tone: 'bad', says: 'Conflicts with main' });
    expect(standing(box({ mergeState: 'BEHIND' }), pr())).toMatchObject({ tone: 'wait', says: 'Behind main' });
    expect(standing(box({ draft: true, mergeState: 'BLOCKED' }), pr()).says).toBe('Still a draft');
    expect(standing(box({ queue: true, queued: { position: 1, state: 'AWAITING_CHECKS' } }), pr())).toEqual({ tone: 'wait', says: 'In the merge queue', more: 'Waiting for the queue’s checks.' });
    expect(standing(box({ auto: { method: 'squash', by: 'ana', at: 1 } }), pr()).more).toBe('GitHub will squash and merge it once its requirements are met. Turned on by ana.');
  });

  it('says who may merge where you can’t', () => {
    expect(standing(box({ canMerge: false }), pr()).more).toBe('Only people who can write to acme/app can merge it.');
  });

  it('says a merged or closed PR is over', () => {
    expect(standing(box({ state: 'MERGED', mergedBy: 'ana' }), pr())).toEqual({ tone: 'done', says: 'Merged by ana' });
    expect(standing(box({ state: 'CLOSED' }), pr()).says).toBe('Closed without merging');
  });

  it('waits while GitHub works out whether it can merge', () => {
    expect(standing(box({ mergeable: 'UNKNOWN', mergeState: 'UNKNOWN' }), pr()).says).toBe('Checking whether it can merge…');
  });
});

describe('offers', () => {
  it('merges a clean or unstable PR now, and nothing else in its way', () => {
    expect(offers(box())).toMatchObject({ merge: true, bypass: false, auto: false, queue: false });
    expect(offers(box({ mergeState: 'UNSTABLE' })).merge).toBe(true);
    expect(offers(box({ mergeState: 'HAS_HOOKS' })).merge).toBe(true);
  });

  it('doesn’t merge what GitHub wouldn’t', () => {
    for (const over of [{ mergeState: 'BLOCKED' }, { mergeState: 'BEHIND' }, { mergeState: 'DIRTY' }, { mergeState: 'UNKNOWN' }, { draft: true }, { canMerge: false }, { methods: [] }, { state: 'MERGED' as const }]) {
      expect(offers(box(over)).merge, JSON.stringify(over)).toBe(false);
    }
  });

  it('offers auto-merge and merging past the rules where GitHub says you may', () => {
    const blocked = box({ mergeState: 'BLOCKED', canAuto: true, canBypass: true });
    expect(offers(blocked)).toMatchObject({ merge: false, auto: true, bypass: true });
    expect(mainMode(offers(blocked), false)).toBe('auto');
    expect(mainMode(offers(blocked), true)).toBe('bypass');
    // Conflicts can't be merged past.
    expect(offers(box({ mergeState: 'DIRTY', mergeable: 'CONFLICTING', canBypass: true }))).toMatchObject({ bypass: false, conflicts: true });
  });

  it('queues where the base branch merges through a queue', () => {
    const queue = box({ queue: true });
    expect(offers(queue)).toMatchObject({ merge: false, queue: true });
    expect(mainMode(offers(queue), false)).toBe('queue');
    expect(offers(box({ queue: true, queued: { position: 1, state: 'QUEUED' } }))).toMatchObject({ queue: false, dequeue: true });
  });

  it('turns auto-merge off, updates the branch, readies a draft and deletes a merged branch', () => {
    expect(offers(box({ auto: { method: 'merge', by: null, at: null }, canStopAuto: true }))).toMatchObject({ merge: false, stopAuto: true });
    expect(offers(box({ mergeState: 'BEHIND', canUpdateBranch: true })).update).toBe(true);
    expect(offers(box({ draft: true })).ready).toBe(true);
    expect(offers(box({ state: 'MERGED' })).deleteBranch).toBe(true);
    expect(offers(box({ state: 'MERGED', branchRef: null })).deleteBranch).toBe(false);
    expect(offers(box()).deleteBranch).toBe(false);
  });

  it('offers nothing to press on a blocked PR you can do nothing about', () => {
    expect(mainMode(offers(box({ mergeState: 'BLOCKED' })), true)).toBeNull();
  });
});

describe('reading it again', () => {
  it('reads it again as the board’s word on it changes, though the PR wasn’t updated', () => {
    const running = fx.prItem({ updatedAt: 5, state: 'approved', merge: 'blocked', checks: checks({ passed: 9, pending: 2 }) });
    const passed = { ...running, merge: 'clean' as const, checks: checks({ passed: 11 }) };
    expect(boxKey(passed)).not.toBe(boxKey(running));
    expect(boxKey({ ...running, checks: checks({ passed: 10, pending: 1 }) })).not.toBe(boxKey(running));
    expect(boxKey({ ...running, state: 'merged' })).not.toBe(boxKey(running));
    expect(boxKey({ ...running })).toBe(boxKey(running));
  });

  it('reads again while GitHub hasn’t worked it out, or says blocked where the board read ready', () => {
    expect(rereads(box({ mergeState: 'UNKNOWN' }), { merge: 'unknown' })).toBe(true);
    expect(rereads(box({ mergeState: 'BLOCKED' }), { merge: 'clean' })).toBe(true);
    expect(rereads(box({ mergeState: 'BLOCKED' }), { merge: 'blocked' })).toBe(false);
    expect(rereads(box({ mergeState: 'CLEAN' }), { merge: 'clean' })).toBe(false);
    expect(rereads(box({ state: 'MERGED', mergeState: 'UNKNOWN' }), { merge: 'unknown' })).toBe(false);
  });
});

describe('the method', () => {
  it('starts on the one picked in the repo before, else GitHub’s', () => {
    expect(pickMethod(box(), 'rebase')).toBe('rebase');
    expect(pickMethod(box(), null)).toBe('squash');
    // One the repo no longer allows isn't kept.
    expect(pickMethod(box({ methods: ['merge', 'squash'] }), 'rebase')).toBe('squash');
    expect(pickMethod(box({ methods: [], defaultMethod: null }), 'rebase')).toBeNull();
  });

  it('says each way, and what GitHub would write for it', () => {
    expect(modeLabel('merge', 'squash')).toBe('Squash and merge');
    expect(modeLabel('merge', 'merge')).toBe('Merge');
    expect(modeLabel('auto', 'squash')).toBe('Enable auto-merge');
    expect(confirmLabel('merge', 'rebase')).toBe('Confirm rebase and merge');
    expect(confirmLabel('auto', 'rebase')).toBe('Confirm auto-merge');
    expect(commitText(box(), 'squash')).toEqual({ title: 'Add teams (#7)', body: '* Add teams' });
    expect(commitText(box(), 'rebase')).toBeNull();
  });

  it('keeps the picks by repo, dropping what it can’t read', () => {
    expect(methodsByRepo.parse('{"acme/app":"squash","acme/tool":"octopus"}')).toEqual({ 'acme/app': 'squash' });
    expect(methodsByRepo.parse('["squash"]')).toBeUndefined();
  });
});

describe('what a merge asks', () => {
  const form = (over: Partial<MergeForm> = {}): MergeForm => ({ method: 'squash', title: 'Add teams (#7)', body: '* Add teams', email: 'ana@work.example', deleteBranch: true, ...over });

  it('names the head it saw, what was written and the branch to delete', () => {
    expect(opFor(box(), 'merge', form())).toEqual({ op: 'merge', head: 'abc', method: 'squash', title: 'Add teams (#7)', body: '* Add teams', email: 'ana@work.example', deleteBranch: 'REF_1' });
  });

  it('writes nothing of its own for a rebase', () => {
    expect(opFor(box(), 'merge', form({ method: 'rebase', deleteBranch: false }))).toEqual({ op: 'merge', head: 'abc', method: 'rebase', title: null, body: null, email: null, deleteBranch: null });
  });

  it('leaves the branch to GitHub where it deletes it itself, and to auto-merge', () => {
    expect(opFor(box({ deletesBranch: true }), 'merge', form())).toMatchObject({ deleteBranch: null });
    expect(deletesWith(box(), 'auto')).toBe(false);
    expect(opFor(box(), 'auto', form())).toEqual({ op: 'auto', head: 'abc', method: 'squash', title: 'Add teams (#7)', body: '* Add teams', email: 'ana@work.example' });
    expect(opFor(box(), 'queue', form())).toEqual({ op: 'enqueue', head: 'abc' });
  });

  it('says what was done', () => {
    expect(doneWords({ op: 'merge', head: 'abc', method: 'squash', title: null, body: null, email: null, deleteBranch: 'REF_1' }, 7, 'teams')).toBe('Squashed and merged #7 and deleted teams.');
    expect(doneWords({ op: 'update-branch', rebase: true, head: 'abc' }, 7, 'teams')).toBe('Brought teams up to date by a rebase.');
    expect(doneWords({ op: 'delete-branch', branchRef: 'REF_1' }, 7, 'teams')).toBe('Deleted teams.');
  });
});
