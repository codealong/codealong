import { describe, expect, it } from 'vitest';
import { reviewLane, reviewPicks, reviewPrompt } from './review';
import * as fx from './testing/fixtures';
import type { Lane, StartAccount } from './types';

const ghostty = { place: 'terminal' as const, label: 'Ghostty' };
const background = { place: 'background' as const, label: 'Background' };
const codexApp = { place: 'codex-app' as const, label: 'Codex app' };

const ACCOUNTS: StartAccount[] = [
  fx.startAccount({ name: 'claude', places: [ghostty, background] }),
  fx.startAccount({ name: 'claude2', unavailable: 'Not signed in.' }),
  fx.startAccount({ name: 'codex', harness: 'codex', places: [codexApp] }),
];

const lane = (id: string, repoSlug: string | null, origin: Lane['origin'] = 'other'): Lane => fx.lane({ id, repoSlug, origin });

describe('reviewLane', () => {
  const pr = { repoSlug: 'acme/app', lanes: ['wt-2'] };
  it('starts in a worktree that holds the PR', () => {
    expect(reviewLane(pr, [lane('main', 'acme/app', 'main'), lane('wt-2', 'acme/app')])).toEqual({ lane: lane('wt-2', 'acme/app'), checkedOut: true });
  });
  it('else in the repo’s main checkout, never on another device', () => {
    const lanes = [lane('@mini:wt', 'acme/app'), lane('other', 'acme/app'), lane('main', 'acme/app', 'main'), lane('x', 'acme/x', 'main')];
    expect(reviewLane({ repoSlug: 'acme/app', lanes: ['@mini:wt'] }, lanes)).toEqual({ lane: lane('main', 'acme/app', 'main'), checkedOut: false });
  });
  it('knows a repo by its name now', () => {
    const renamed = { ...lane('old', 'acme/old'), repoSlugNow: 'acme/app' };
    expect(reviewLane({ repoSlug: 'acme/app', lanes: [] }, [renamed])?.lane.id).toBe('old');
  });
  it('is null without a checkout of the repo', () => {
    expect(reviewLane({ repoSlug: 'acme/app', lanes: [] }, [lane('x', 'acme/x', 'main')])).toBeNull();
  });
});

describe('reviewPicks', () => {
  it('lists Auto, then each account that can start, where it starts', () => {
    expect(reviewPicks(ACCOUNTS, [], 0, true)).toEqual([
      { kind: 'start', label: 'Auto (claude) in Ghostty', account: 'claude', place: 'terminal' },
      { kind: 'start', label: 'claude in Ghostty', account: 'claude', place: 'terminal' },
      { kind: 'codex-app', label: 'codex in the Codex app' },
    ]);
  });
  it('goes where the account last started', () => {
    expect(reviewPicks(ACCOUNTS, [], 0, true, { claude: 'background' })[1]).toEqual({ kind: 'start', label: 'claude in the background', account: 'claude', place: 'background' });
  });
  it('without a checkout offers only the Codex app’s review page', () => {
    expect(reviewPicks(ACCOUNTS, [], 0, false)).toEqual([{ kind: 'codex-app', label: 'codex in the Codex app' }]);
  });
});

describe('reviewPrompt', () => {
  it('names the PR and asks for a review that changes nothing', () => {
    const text = reviewPrompt({ number: 12, repoSlug: 'acme/app', url: 'https://github.com/acme/app/pull/12', title: 'Fix it' }, true);
    expect(text).toContain('#12 in acme/app');
    expect(text).toContain('gh pr diff 12 -R acme/app');
    expect(text).toContain('has its branch checked out');
    expect(text).toContain('Don’t change files');
  });
});
