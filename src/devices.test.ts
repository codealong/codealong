import { describe, expect, it } from 'vitest';
import { deviceLabel, deviceOf, floatingLanes, needsOn, shortOf, withDevices, workingOn } from './devices';
import * as fx from './testing/fixtures';
import { DEVICES_OFF, type DeviceTag, type Lane, type Session } from './types';

const session = (key: string, status: Session['status']): Session => fx.session({ key, status });
const lane = (id: string, sessions: Session[] = []): Lane => fx.lane({ id, sessions });

describe('your other devices', () => {
  it('reads which device a key is on', () => {
    expect(shortOf('@a1b2c3d4e5:claude:claude:9f')).toBe('a1b2c3d4e5');
    expect(shortOf('@a1b2c3d4e5:/Users/sam/app')).toBe('a1b2c3d4e5');
    expect(shortOf('claude:claude:9f')).toBeNull();
    expect(shortOf('/Users/sam/app')).toBeNull();
  });

  it('finds the linked device a key is on', () => {
    const view = fx.devicesView({ devices: [fx.deviceView({ short: 'a1b2c3d4e5', name: 'Mac mini' })] });
    expect(deviceOf(view, '@a1b2c3d4e5:claude:claude:9f')?.name).toBe('Mac mini');
    expect(deviceOf(view, '@ffffffffff:claude:claude:9f')).toBeNull();
    expect(deviceOf(view, 'claude:claude:9f')).toBeNull();
    expect(deviceOf(null, '@a1b2c3d4e5:x')).toBeNull();
  });

  it('shows every device, or the ones picked', () => {
    const here = [lane('/a')];
    const there = [lane('@m1:/b'), lane('@m2:/c')];
    expect(withDevices(here, there, []).map((l) => l.id)).toEqual(['/a', '@m1:/b', '@m2:/c']);
    expect(withDevices(here, there, ['here']).map((l) => l.id)).toEqual(['/a']);
    expect(withDevices(here, there, ['m2']).map((l) => l.id)).toEqual(['@m2:/c']);
    expect(withDevices(here, there, ['here', 'm2']).map((l) => l.id)).toEqual(['/a', '@m2:/c']);
    expect(withDevices(here, there, ['here'])).toBe(here);
    // With no other device's lanes, this device's list is handed back as it is.
    expect(withDevices(here, [], [])).toBe(here);
  });

  it('lists in the windows that float the connected devices’ worktrees, unless that’s turned off', () => {
    const tag = (short: string, online: boolean): DeviceTag => ({ name: `Mac ${short}`, short, color: 'teal', online, lastSeen: null });
    const lanes = [{ ...lane('@m1:/b'), device: tag('m1', true) }, { ...lane('@m2:/c'), device: tag('m2', false) }];
    const on = { ...DEVICES_OFF, enabled: true };
    expect(floatingLanes(on, lanes).map((l) => l.id)).toEqual(['@m1:/b']);
    expect(floatingLanes({ ...on, in_bar: false }, lanes)).toEqual([]);
    expect(floatingLanes(DEVICES_OFF, lanes)).toEqual([]);
    expect(floatingLanes(undefined, lanes)).toEqual([]);
  });

  it('counts what needs you and what works on each device', () => {
    const lanes = [lane('/a', [session('s1', 'needs'), session('s2', 'working')]), lane('@m1:/b', [session('@m1:s3', 'error'), session('@m1:s4', 'needs')])];
    expect(needsOn(lanes, null)).toBe(1);
    expect(needsOn(lanes, 'm1')).toBe(2);
    expect(needsOn(lanes, 'm2')).toBe(0);
    expect(workingOn(lanes, null)).toBe(1);
    expect(workingOn(lanes, 'm1')).toBe(0);
    // A helper or a headless run at work isn't an agent of yours at work.
    const runs = [lane('/a', [session('s2', 'working'), { ...session('r1', 'working'), headless: true }, { ...session('h1', 'working'), helper: { parent: 's2' } }])];
    expect(workingOn(runs, null)).toBe(1);
  });

  it('names a device without its owner', () => {
    expect(deviceLabel('Alex’s Mac mini')).toBe('Mac mini');
    expect(deviceLabel("Sam's MacBook Pro")).toBe('MacBook Pro');
    expect(deviceLabel('Studio')).toBe('Studio');
    expect(deviceLabel('build-box')).toBe('build-box');
  });
});
