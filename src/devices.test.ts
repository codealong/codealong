import { describe, expect, it } from 'vitest';
import { deviceOf, macOf, needsOn, withMacs } from './devices';
import type { DevicesView, Lane, Session } from './types';

const session = (key: string, status: Session['status']): Session => ({ key, status, visitor: null, archived: false }) as unknown as Session;
const lane = (id: string, sessions: Session[] = []): Lane => ({ id, sessions }) as unknown as Lane;

describe('your other Macs', () => {
  it('reads which Mac a key is on', () => {
    expect(macOf('@a1b2c3d4e5:claude:claude:9f')).toBe('a1b2c3d4e5');
    expect(macOf('@a1b2c3d4e5:/Users/sam/app')).toBe('a1b2c3d4e5');
    expect(macOf('claude:claude:9f')).toBeNull();
    expect(macOf('/Users/sam/app')).toBeNull();
  });

  it('finds the linked Mac a key is on', () => {
    const view = { devices: [{ short: 'a1b2c3d4e5', name: 'Mac mini' }] } as unknown as DevicesView;
    expect(deviceOf(view, '@a1b2c3d4e5:claude:claude:9f')?.name).toBe('Mac mini');
    expect(deviceOf(view, '@ffffffffff:claude:claude:9f')).toBeNull();
    expect(deviceOf(view, 'claude:claude:9f')).toBeNull();
    expect(deviceOf(null, '@a1b2c3d4e5:x')).toBeNull();
  });

  it('shows every Mac, this one, or one other', () => {
    const here = [lane('/a')];
    const there = [lane('@m1:/b'), lane('@m2:/c')];
    expect(withMacs(here, there, null).map((l) => l.id)).toEqual(['/a', '@m1:/b', '@m2:/c']);
    expect(withMacs(here, there, 'here').map((l) => l.id)).toEqual(['/a']);
    expect(withMacs(here, there, 'm2').map((l) => l.id)).toEqual(['@m2:/c']);
    // With no other Mac's lanes, this Mac's list is handed back as it is.
    expect(withMacs(here, [], null)).toBe(here);
  });

  it('counts what needs you on each Mac', () => {
    const lanes = [lane('/a', [session('s1', 'needs'), session('s2', 'working')]), lane('@m1:/b', [session('@m1:s3', 'error'), session('@m1:s4', 'needs')])];
    expect(needsOn(lanes, null)).toBe(1);
    expect(needsOn(lanes, 'm1')).toBe(2);
    expect(needsOn(lanes, 'm2')).toBe(0);
  });
});
