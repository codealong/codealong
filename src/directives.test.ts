import { describe, expect, it } from 'vitest';
import { attrsOf, plainDirectives, tonesOf } from './directives';

describe('attrsOf', () => {
  it('reads quoted and bare values, escaped quotes and braces inside quotes', () => {
    expect(attrsOf('{variant="standard" id="83147"}')).toEqual({ variant: 'standard', id: '83147' });
    expect(attrsOf('{title="[P2] Off-by-one" body="Say \\"no\\" {here}." file="/a/b.ts" start=10 end=11 priority=2}')).toEqual({
      title: '[P2] Off-by-one',
      body: 'Say "no" {here}.',
      file: '/a/b.ts',
      start: '10',
      end: '11',
      priority: '2',
    });
    expect(attrsOf('')).toEqual({});
  });
});

describe('tonesOf', () => {
  it('splits a writing block into the versions its tone lines lead', () => {
    expect(tonesOf('---tone Friendly\nHi there!\n\n---tone Direct\nDo it by Friday.\n')).toEqual([
      { label: 'Friendly', text: 'Hi there!' },
      { label: 'Direct', text: 'Do it by Friday.' },
    ]);
    expect(tonesOf('\nJust one draft.\n')).toEqual([{ label: '', text: 'Just one draft.' }]);
    // A block still empty, as one starts streaming.
    expect(tonesOf('')).toEqual([{ label: '', text: '' }]);
  });
});

describe('plainDirectives', () => {
  it('reads Codex’s directives as they’re drawn', () => {
    const reply = [
      "Here's the prompt:",
      ':::writing{variant="standard" id="83147"}',
      '---tone Short',
      'Read the status first.',
      ':::',
      '::code-comment{title="[P1] Retry skips checks" body="It bypasses the team check." file="/w/a.ts" start=3}',
      '- :codex-followup[Fix image downloads]{prompt="Implement the fix. Do not push."}',
      ':codex-file-citation{path="/w/out/report.pdf" purpose="output"}',
    ].join('\n');
    expect(plainDirectives(reply)).toBe(
      ["Here's the prompt:", '', 'Short', 'Read the status first.', '', '[P1] Retry skips checks /w/a.ts It bypasses the team check.', '- Fix image downloads', '/w/out/report.pdf'].join('\n'),
    );
  });
});
