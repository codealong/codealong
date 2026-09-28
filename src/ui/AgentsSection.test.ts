import { describe, expect, it } from 'vitest';
import type { AccountSkill, PluginState } from '../types';
import { pluginAction, pluginState, skillAction, skillState, versionText } from './AgentsSection';

function skill(over: Partial<AccountSkill>): AccountSkill {
  return { name: 'codex', harness: 'codex', path: '/Users/you/.codex/skills/codealong', installed: true, current: true, edited: false, plugin: null, ...over };
}

describe('skill rows', () => {
  it('says where a copy stands', () => {
    expect(skillState(skill({})).label).toBe('Installed');
    expect(skillState(skill({ current: false }))).toEqual({ label: 'Update available', tone: 'attn' });
    expect(skillState(skill({ installed: false, current: false })).label).toBe('Not installed');
    expect(skillState(skill({ path: null, installed: false, current: false })).label).toBe('No skills folder known');
  });

  it('leaves an edited copy alone until you replace it', () => {
    expect(skillState(skill({ edited: true }))).toEqual({ label: 'Edited', tone: 'none' });
    expect(skillState(skill({ edited: true, current: false }))).toEqual({ label: 'Edited, update available', tone: 'none' });
    expect(skillAction(skill({ edited: true }))).toBe('Restore');
    expect(skillAction(skill({ edited: true, current: false }))).toBe('Update anyway');
    expect(skillAction(skill({ current: false }))).toBe('Update');
  });
});

describe('plugin rows', () => {
  const shipped = '0.2.0+1a2b3c4d';
  const plugin = (over: Partial<PluginState>): PluginState => ({ version: shipped, disabled: false, from: null, ...over });

  it('says where the plugin stands against the one this build carries', () => {
    expect(pluginState(plugin({}), shipped)).toEqual({ label: 'Installed', tone: 'ok' });
    expect(pluginState(plugin({ version: '0.2.0+99999999' }), shipped)).toEqual({ label: 'Update available', tone: 'attn' });
    expect(pluginState(plugin({ version: null }), shipped)).toEqual({ label: 'Not installed', tone: 'attn' });
    expect(pluginState(plugin({ disabled: true }), shipped).label).toBe('Turned off');
    expect(pluginState(plugin({ from: 'codealong/codealong', version: '0.2.0' }), shipped).label).toBe('Installed from elsewhere');
  });

  it('offers what would put the app’s own plugin in place', () => {
    expect(pluginAction(plugin({ version: null }), shipped)).toBe('Install plugin');
    expect(pluginAction(plugin({ version: '0.1.0+00000000' }), shipped)).toBe('Update');
    expect(pluginAction(plugin({ disabled: true }), shipped)).toBe('Turn on');
    expect(pluginAction(plugin({ from: 'codealong/codealong' }), shipped)).toBe('Use Codealong’s copy');
    expect(pluginAction(plugin({}), shipped)).toBe('Install again');
  });

  it('reads a stamped version', () => {
    expect(versionText('0.2.0+1a2b3c4d')).toBe('0.2.0 · 1a2b3c4d');
    expect(versionText('0.2.0')).toBe('0.2.0');
  });
});
