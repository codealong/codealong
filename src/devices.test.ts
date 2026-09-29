import { describe, expect, it } from 'vitest';
import { byDevice, deviceLabel, deviceOf, needsOn, shortOf, withDevices, workingOn } from './devices';
import type { DevicesView, Lane, Session } from './types';

const session = (key: string, status: Session['status']): Session => ({ key, status, visitor: null, archived: false }) as unknown as Session;
const lane = (id: string, sessions: Session[] = []): Lane => ({ id, sessions }) as unknown as Lane;

describe('your other devices', () => {
  it('reads which device a key is on', () => {
    expect(shortOf('@a1b2c3d4e5:claude:claude:9f')).toBe('a1b2c3d4e5');
    expect(shortOf('@a1b2c3d4e5:/Users/sam/app')).toBe('a1b2c3d4e5');
    expect(shortOf('claude:claude:9f')).toBeNull();
    expect(shortOf('/Users/sam/app')).toBeNull();
  });

  it('finds the linked device a key is on', () => {
    const view = { devices: [{ short: 'a1b2c3d4e5', name: 'Mac mini' }] } as unknown as DevicesView;
    expect(deviceOf(view, '@a1b2c3d4e5:claude:claude:9f')?.name).toBe('Mac mini');
    expect(deviceOf(view, '@ffffffffff:claude:claude:9f')).toBeNull();
    expect(deviceOf(view, 'claude:claude:9f')).toBeNull();
    expect(deviceOf(null, '@a1b2c3d4e5:x')).toBeNull();
  });

  it('shows every device, this one, or one other', () => {
    const here = [lane('/a')];
    const there = [lane('@m1:/b'), lane('@m2:/c')];
    expect(withDevices(here, there, null).map((l) => l.id)).toEqual(['/a', '@m1:/b', '@m2:/c']);
    expect(withDevices(here, there, 'here').map((l) => l.id)).toEqual(['/a']);
    expect(withDevices(here, there, 'm2').map((l) => l.id)).toEqual(['@m2:/c']);
    // With no other device's lanes, this device's list is handed back as it is.
    expect(withDevices(here, [], null)).toBe(here);
  });

  it('counts what needs you and what works on each device', () => {
    const lanes = [lane('/a', [session('s1', 'needs'), session('s2', 'working')]), lane('@m1:/b', [session('@m1:s3', 'error'), session('@m1:s4', 'needs')])];
    expect(needsOn(lanes, null)).toBe(1);
    expect(needsOn(lanes, 'm1')).toBe(2);
    expect(needsOn(lanes, 'm2')).toBe(0);
    expect(workingOn(lanes, null)).toBe(1);
    expect(workingOn(lanes, 'm1')).toBe(0);
  });

  it('names a device without its owner', () => {
    expect(deviceLabel('Gordon’s Mac mini')).toBe('Mac mini');
    expect(deviceLabel("Sam's MacBook Pro")).toBe('MacBook Pro');
    expect(deviceLabel('Studio')).toBe('Studio');
    expect(deviceLabel('build-box')).toBe('build-box');
  });

  it('groups lanes by device, this one first, then in linking order', () => {
    const view = { devices: [{ short: 'm2' }, { short: 'm1' }] } as unknown as DevicesView;
    const groups = byDevice([lane('@m1:/b'), lane('/a'), lane('@m2:/c'), lane('@m1:/d')], view);
    expect(groups.map((g) => [g.short, g.lanes.map((l) => l.id)])).toEqual([
      [null, ['/a']],
      ['m2', ['@m2:/c']],
      ['m1', ['@m1:/b', '@m1:/d']],
    ]);
    expect(byDevice([lane('/a')], null).map((g) => g.short)).toEqual([null]);
  });
});
