import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import * as fx from '../testing/fixtures';
import type { GitInfo } from '../types';
import { driftWords, Seg } from './primitives';

describe('driftWords', () => {
  const git = (g: Partial<GitInfo>): GitInfo => fx.git({ base: 'origin/main', baseAhead: 0, baseBehind: 0, ...g });

  it('says how far a worktree has moved from the default branch', () => {
    expect(driftWords(git({ baseAhead: 3, baseBehind: 42 }))).toBe('3 commits ahead of origin/main, 42 behind');
    expect(driftWords(git({ baseAhead: 1 }))).toBe('1 commit ahead of origin/main');
    expect(driftWords(git({ baseBehind: 1200 }))).toBe('1,200 commits behind origin/main');
  });

  it('says nothing when it is even with it or git has not said', () => {
    expect(driftWords(git({}))).toBeNull();
    expect(driftWords(git({ base: undefined, baseAhead: undefined, baseBehind: undefined }))).toBeNull();
    expect(driftWords(null)).toBeNull();
  });
});

describe('Seg', () => {
  const options = [
    { id: 'a', label: 'First', title: 'The first one' },
    { id: 'b', label: 'Second', icon: 'sun' },
  ];
  const html = (props: object) => renderToStaticMarkup(createElement(Seg<string>, { options, value: 'b', onPick: () => {}, label: 'Order', ...props }));

  it('is a group of radios that says which is on', () => {
    const out = html({ className: 'ca-side-sorts' });
    expect(out).toMatch(/^<div class="ca-segctl ca-side-sorts" role="radiogroup" aria-label="Order">/);
    expect(out).toContain('<button type="button" role="radio" aria-checked="false" title="The first one">First</button>');
    expect(out).toMatch(/<button type="button" role="radio" aria-checked="true" class="is-on"><svg[^>]*>.*<\/svg>Second<\/button>/);
  });

  it('draws words alone, or icons alone named by their words', () => {
    expect(html({ look: 'text' })).not.toContain('<svg');
    const icons = html({ look: 'icon' });
    expect(icons).toContain('aria-label="Second"');
    expect(icons).toContain('title="Second"');
    expect(icons).not.toContain('>Second<');
  });

  it('is pressed buttons where none need be on', () => {
    const out = html({ toggles: true, value: null });
    expect(out).toContain('role="group"');
    expect(out).not.toContain('role="radio"');
    expect(out.match(/aria-pressed="false"/g)).toHaveLength(2);
  });

  it('turns every choice off when it is disabled, or one by itself', () => {
    expect(html({ disabled: true }).match(/disabled=""/g)).toHaveLength(2);
    expect(html({ options: [options[0], { ...options[1], disabled: true }] }).match(/disabled=""/g)).toHaveLength(1);
  });
});
