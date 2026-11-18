import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { byWords, FilterChips, rowsOptions, rowsSetting, ViewMenu, viewSetting } from './ListBar';
import { queueWords } from './SessionPanel';

const SORTS = [
  { id: 'status', label: 'Status', icon: 'sort-descending-shapes', title: 'Sort by status' },
  { id: 'started', label: 'Started', icon: 'sort-descending-numbers', title: 'Sort by start' },
];
const ROWS = rowsOptions('two', 'one');

describe('viewSetting', () => {
  it('says nothing at its default', () => {
    expect(viewSetting('Sort', SORTS, 'status', 'status', () => {}, byWords).said).toBeNull();
    expect(rowsSetting(ROWS, 'one', () => {}).said).toBeNull();
  });

  it('words the choice that is off its default', () => {
    expect(viewSetting('Sort', SORTS, 'started', 'status', () => {}, byWords).said).toBe('by started');
    expect(rowsSetting(ROWS, 'two', () => {}).said).toBe('two lines');
  });
});

describe('ViewMenu', () => {
  const html = (settings: ReturnType<typeof viewSetting>[]) => renderToStaticMarkup(createElement(ViewMenu, { label: 'How Sessions is listed', settings }));

  it('names on its button what is off its default, and only that', () => {
    const out = html([viewSetting('Sort', SORTS, 'started', 'status', () => {}, byWords), rowsSetting(ROWS, 'one', () => {})]);
    expect(out).toContain('<span class="ca-viewmenu-said">by started</span>');
    expect(out).not.toContain('one line');
  });

  it('is a plain View at every default', () => {
    expect(html([rowsSetting(ROWS, 'one', () => {})])).not.toContain('ca-viewmenu-said');
  });
});

describe('FilterChips', () => {
  const chip = (label: string) => ({ key: label, label, name: label, clear: () => {} });
  const html = (n: number) => renderToStaticMarkup(createElement(FilterChips, { filters: ['Claude', 'Last 7 days'].slice(0, n).map(chip), onClearAll: () => {} }));

  it('shows nothing while no filter is on', () => {
    expect(html(0)).toBe('');
  });

  it('offers Clear only for several', () => {
    expect(html(1)).not.toContain('Clear');
    expect(html(2)).toContain('ca-listfilters-clear');
  });
});

describe('queueWords', () => {
  const queue = (left: number, waiting: boolean) => ({ next: { title: 'Release notes' }, left, waiting, onNext: () => {} });

  it('counts what waits after the session shown, and names the next', () => {
    expect(queueWords(queue(2, true))).toBe('2 more waiting · next: Release notes');
    expect(queueWords(queue(1, false))).toBe('1 waiting · next: Release notes');
  });

  it('says nothing with nothing after it', () => {
    expect(queueWords(undefined)).toBe('');
    expect(queueWords({ next: null, left: 0, onNext: () => {} })).toBe('');
  });
});
