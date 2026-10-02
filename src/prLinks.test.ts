import { describe, expect, it } from 'vitest';
import { prIndex, prLabel, prRefOf } from './prLinks';
import * as fx from './testing/fixtures';

describe('prRefOf', () => {
  it('reads a PR link, whatever part of the PR it goes to', () => {
    expect(prRefOf('https://github.com/Acme/tool.js/pull/1057')).toEqual({ slug: 'Acme/tool.js', number: 1057, key: 'acme/tool.js#1057', part: null });
    const parts = [
      ['https://github.com/acme/app/pull/7/files', 'files'],
      ['https://github.com/acme/app/pull/7/files/3f2a1b', 'files'],
      ['https://github.com/acme/app/pull/7/commits/abc123', 'commits'],
      ['https://github.com/acme/app/pull/7/checks?check_run_id=9', 'checks'],
      ['https://github.com/acme/app/pull/7#issuecomment-123', 'comment'],
      ['https://github.com/acme/app/pull/7/files#discussion_r456', 'comment'],
      ['https://github.com/acme/app/pull/7#pullrequestreview-789', 'review'],
      ['http://www.github.com/acme/app/pull/7?w=1', null],
      ['https://github.com/acme/app/pull/7/', null],
    ] as const;
    for (const [url, part] of parts) expect([url, prRefOf(url)?.part]).toEqual([url, part]);
  });

  it("takes only links to a PR", () => {
    for (const url of [
      'https://github.com/acme/app/pulls',
      'https://github.com/acme/app/pull/',
      'https://github.com/acme/app/pull/12abc',
      'https://github.com/acme/app/issues/7',
      'https://github.com/acme/pull/7',
      'https://gitlab.com/acme/app/pull/7',
      'https://github.com.evil.io/acme/app/pull/7',
    ]) {
      expect([url, prRefOf(url)]).toEqual([url, null]);
    }
  });
});

describe('prLabel', () => {
  it('shortens a link as GitHub does, saying what part it goes to', () => {
    expect(prLabel(prRefOf('https://github.com/acme/app/pull/7')!)).toBe('app#7');
    expect(prLabel(prRefOf('https://github.com/acme/app/pull/7#issuecomment-1')!)).toBe('app#7 (comment)');
  });
});

describe('prIndex', () => {
  const item = (key: string, url: string) => fx.prItem({ key, url });

  it("finds a PR by the board's name for its repo and by its URL's, whatever the case", () => {
    const renamed = item('oldorg/app#3', 'https://github.com/NewOrg/app/pull/3');
    const index = prIndex([item('Acme/App#7', 'https://github.com/Acme/App/pull/7'), renamed]);
    expect(index.get(prRefOf('https://github.com/acme/app/pull/7')!.key)?.key).toBe('Acme/App#7');
    expect(index.get(prRefOf('https://github.com/oldorg/app/pull/3')!.key)).toBe(renamed);
    expect(index.get(prRefOf('https://github.com/neworg/APP/pull/3')!.key)).toBe(renamed);
    expect(index.get('acme/app#8')).toBeUndefined();
  });
});
