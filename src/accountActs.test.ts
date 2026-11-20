// @vitest-environment jsdom
import { beforeEach, describe, expect, it } from 'vitest';
import { engine, settle } from './testing/dom';
import { accountActsOf } from './accountActs';
import type { Training } from './types';

// What the main window's usage bar, its notices and Settings ask for an account (accountActs.ts).

let did: string[] = [];
beforeEach(() => {
  did = [];
});
const acts = () =>
  accountActsOf({
    setToast: (m) => void did.push(`say “${m}”`),
    track: (label, work) => {
      did.push(`busy: ${label}`);
      return work;
    },
  });
const training = (over: Partial<Training> = {}): Training => ({ account: 'northwind', state: 'off', note: '', canStop: true, url: null, asOf: null, ...over });

describe('what’s asked for an account', () => {
  it('opens config.toml, saying why when it can’t', async () => {
    acts().editConfig();
    await settle();
    expect([engine.sent('open_config'), did]).toEqual([[{}], []]);
    engine.answer('open_config', () => {
      throw new Error('No editor.');
    });
    acts().editConfig();
    await settle();
    expect(did).toEqual(['say “Error: No editor.”']);
  });

  it('signs an account in, and says where', async () => {
    acts().startSignIn('northwind');
    await settle();
    expect(engine.sent('sign_in')).toEqual([{ name: 'northwind' }]);
    expect(did).toEqual(['say “Signing in to northwind in your terminal.”']);
  });

  it('turns training off for an account, showing it’s under way, and says it’s off', async () => {
    engine.answer('stop_training', training());
    acts().turnOffTraining('northwind');
    await settle();
    expect(engine.sent('stop_training')).toEqual([{ name: 'northwind' }]);
    expect(did).toEqual(['busy: Turning off training for northwind…', 'say “Training is off for northwind.”']);
  });

  it('says so when training still reads as on after it was asked off, or the engine turns it down', async () => {
    engine.answer('stop_training', training({ state: 'on', note: 'Its setting is managed by the team.' }));
    acts().turnOffTraining('northwind');
    await settle();
    expect(did[1]).toBe('say “Training for northwind was asked to turn off, but still reads as on. Its setting is managed by the team.”');
    did = [];
    engine.answer('stop_training', () => {
      throw new Error('Signed out.');
    });
    acts().turnOffTraining('northwind');
    await settle();
    expect(did[1]).toBe('say “Signed out.”');
  });

  it('uses a limit reset, the one named when one is, and says how it went', async () => {
    engine.answer('use_reset', { message: 'Reset northwind’s weekly limit.' });
    acts().resetLimits('northwind');
    acts().resetLimits('northwind', 'credit-2');
    await settle();
    expect(engine.sent('use_reset')).toEqual([
      { name: 'northwind', credit: null },
      { name: 'northwind', credit: 'credit-2' },
    ]);
    expect(did).toEqual(['busy: Resetting northwind’s limits…', 'busy: Resetting northwind’s limits…', 'say “Reset northwind’s weekly limit.”', 'say “Reset northwind’s weekly limit.”']);
  });
});
