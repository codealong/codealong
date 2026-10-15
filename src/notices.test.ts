import { describe, expect, it } from 'vitest';
import { keptDismissals, noticeAction, noticeId, noticeLabel, shownNotices, untilPhrase } from './notices';
import type { Notice } from './types';

const NOW = new Date(2026, 9, 1, 15, 31).getTime();
const limit: Notice = { key: 'github', title: 'GitHub rate limit reached', text: "GitHub: your account's rate limit is used up, so PRs wait until it resets", until: new Date(2026, 9, 1, 15, 43).getTime() };
const config: Notice = { key: 'config', title: 'config.toml has a problem', text: 'config.toml: expected a table. Codealong is using the last good settings.', until: null };

describe('notices', () => {
  it('shows what has not been dismissed', () => {
    expect(shownNotices([limit, config], [])).toEqual([limit, config]);
    expect(shownNotices([limit, config], [noticeId(limit)])).toEqual([config]);
    // Said once, however many times it's reported.
    expect(shownNotices([config, { ...config }], [])).toEqual([config]);
  });

  it('says a dismissed notice again when it comes back, or in other words', () => {
    const dismissed = [noticeId(limit), noticeId(config)];
    // GitHub answers again: its dismissal is forgotten.
    expect(keptDismissals(dismissed, [config])).toEqual([noticeId(config)]);
    expect(shownNotices([limit, config], keptDismissals(dismissed, [config]))).toEqual([limit]);
    // Another failure of GitHub's is another notice.
    const signedOut = { ...limit, title: 'Signed out of GitHub', until: null };
    expect(shownNotices([signedOut], dismissed)).toEqual([signedOut]);
    // The same one with a later end stays dismissed.
    expect(shownNotices([{ ...limit, until: NOW + 1 }], dismissed)).toEqual([]);
  });

  it('says until when, where that is known and ahead', () => {
    expect(noticeLabel(limit, NOW)).toBe('GitHub rate limit reached until 3:43 PM');
    expect(untilPhrase(limit, NOW)).toBe('Until 3:43 PM, in 12m');
    expect(noticeLabel(limit, limit.until! + 1)).toBe('GitHub rate limit reached');
    expect(untilPhrase(limit, limit.until! + 1)).toBeNull();
    expect(noticeLabel(config, NOW)).toBe('config.toml has a problem');
    expect(untilPhrase(config, NOW)).toBeNull();
  });

  it('offers the settings file for a problem with it', () => {
    expect(noticeAction(config)).toEqual({ label: 'Open config.toml', opens: 'settings-file' });
    expect(noticeAction(limit)).toBeNull();
  });

  it('offers to start a review that did not start again', () => {
    const review: Notice = { key: 'reviews', title: "A pull request wasn't reviewed", text: "Codealong couldn't start a review of o/r#1238: GitHub: gh timed out after 30s", reviewAgain: 'o/r#1238' };
    expect(noticeAction(review)).toEqual({ label: 'Try again', reviews: { key: 'o/r#1238', number: 1238 } });
    // One that can't be started again from here (a review that wasn't posted) offers nothing.
    expect(noticeAction({ ...review, reviewAgain: null })).toBeNull();
  });
});
