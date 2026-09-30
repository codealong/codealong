import { describe, expect, it } from 'vitest';
import { openLabel, openWhere, openWords, where } from './open';
import type { Lane } from './types';

const lane = {
  id: '/r',
  targets: [
    { id: 's|claude:c:1|focus', label: 'Respond in Ghostty', detail: null, disabled: false },
    { id: 'l|preview', label: 'Open preview', detail: 'Its server isn’t running', disabled: true },
  ],
} as unknown as Lane;

describe('open', () => {
  it('says how a session opens as the engine does', () => {
    expect(openLabel(lane, { openTarget: 's|claude:c:1|focus' }, 'Ghostty')).toBe('Respond in Ghostty');
    expect(openWords('s|claude:c:2|resume', 'Ghostty')).toBe('Resume in Ghostty');
    expect(openWords('s|claude:c:2|hosted', 'Ghostty')).toBe('Show in Codealong');
    expect(openWords('s|x|unknown', 'Ghostty')).toBe('Open');
  });

  it('says why a target can’t open now, and does nothing else', () => {
    const said: string[] = [];
    let before = false;
    openWhere(where(lane, 'l|preview'), { report: { error: (m) => said.push(m), info: () => {} }, from: 'panel', before: () => (before = true) });
    expect(said).toEqual(['Its server isn’t running']);
    expect(before).toBe(false);
  });
});
