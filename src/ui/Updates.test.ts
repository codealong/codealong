import { describe, expect, it } from 'vitest';
import { checkedSays } from './Updates';

describe('checkedSays', () => {
  it('says what the app menu’s check came to', () => {
    expect(checkedSays({ current: '0.5.0', state: 'upToDate', checkedAt: 0 }, 0)).toBe('Codealong 0.5.0 is the latest version.');
    expect(checkedSays({ current: '0.5.0', state: 'ready', version: '0.6.0', notes: 'Faster' }, 0)).toBe('Codealong 0.6.0 is ready: restart to update from the sidebar.');
    expect(checkedSays({ current: '0.5.0', state: 'failed', error: 'Couldn’t check for updates: offline', at: 0 }, 0)).toBe('Couldn’t check for updates: offline');
    expect(checkedSays({ current: '0.5.0', state: 'off' }, 0)).toBe('This is a development build, which never updates itself.');
  });
});
