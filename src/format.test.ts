import { describe, expect, it } from 'vitest';
import { ago, count, duration, middleTruncate, resetsPhrase, tildePath, untilShort, withTimes } from './format';

const NOW = 1_790_200_000_000;

describe('format', () => {
  it('writes compact relative times', () => {
    expect(ago(NOW - 10_000, NOW)).toBe('now');
    expect(ago(NOW - 4 * 60_000, NOW)).toBe('4m');
    expect(ago(NOW - 2 * 3_600_000, NOW)).toBe('2h');
    expect(ago(NOW - 3 * 86_400_000, NOW)).toBe('3d');
  });

  it('writes durations for running work', () => {
    expect(duration(NOW - 30_000, NOW)).toBe('<1m');
    expect(duration(NOW - 14 * 60_000, NOW)).toBe('14m');
    expect(duration(NOW - (2 * 60 + 5) * 60_000, NOW)).toBe('2h 5m');
  });

  it('says when a limit resets', () => {
    expect(resetsPhrase(NOW + 42 * 60_000, NOW)).toBe('in 42m');
    expect(resetsPhrase(NOW + (3 * 60 + 10) * 60_000, NOW)).toBe('in 3h 10m');
    expect(resetsPhrase(NOW - 1, NOW)).toBe('now');
    expect(resetsPhrase(NOW + 3 * 86_400_000, NOW)).toMatch(/^[A-Z][a-z]{2} \d{1,2}:\d{2}\s?[AP]M$/);
  });

  it('shortens time to a reset', () => {
    expect(untilShort(NOW + 20_000, NOW)).toBe('<1m');
    expect(untilShort(NOW + 42 * 60_000, NOW)).toBe('42m');
    expect(untilShort(NOW + 59.8 * 60_000, NOW)).toBe('1h');
    expect(untilShort(NOW + (4 * 60 + 47) * 60_000, NOW)).toBe('5h');
    expect(untilShort(NOW + 23.8 * 3_600_000, NOW)).toBe('1d');
    expect(untilShort(NOW + 47 * 3_600_000, NOW)).toBe('2d');
  });

  it('renders time tokens', () => {
    expect(withTimes(`{for:${NOW - 14 * 60_000}} · running tests`, NOW)).toBe('14m · running tests');
    expect(withTimes(`#1016 merged {ago:${NOW - 2 * 3_600_000}}`, NOW)).toBe('#1016 merged 2h ago');
  });

  it('shortens paths and branches', () => {
    expect(middleTruncate('codex/agent-execution-grants', 20)).toBe('codex/agen…on-grants');
    expect(tildePath('/Users/sam/projects/x')).toBe('~/projects/x');
    expect(count(1, 'file')).toBe('1 file');
    expect(count(3, 'file')).toBe('3 files');
  });
});
