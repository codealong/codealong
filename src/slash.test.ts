import { describe, expect, it } from 'vitest';
import { slashCommand, slashMatches } from './slash';
import * as fx from './testing/fixtures';
import type { Session } from './types';

const session = (harness: Session['harness'], key = `${harness}:a:1`): Session => fx.session({ key, harness });
const claude = session('claude');

describe('slashCommand', () => {
  it('takes /fork alone or with a prompt for the copy', () => {
    expect(slashCommand('/fork', claude)?.command.name).toBe('fork');
    expect(slashCommand('  /fork  ', claude)?.rest).toBe('');
    expect(slashCommand('/fork try it with a queue instead', claude)?.rest).toBe('try it with a queue instead');
    expect(slashCommand('/fork\nfirst line\nsecond line', claude)?.rest).toBe('first line\nsecond line');
  });

  it('leaves the harness’s own commands and plain text to the session', () => {
    expect(slashCommand('/compact', claude)).toBeNull();
    expect(slashCommand('/forked', claude)).toBeNull();
    expect(slashCommand('please /fork it', claude)).toBeNull();
    expect(slashCommand('/Fork', claude)).toBeNull();
  });

  it('takes /side, or /btw, with its question', () => {
    expect(slashCommand('/side why a queue?', claude)).toMatchObject({ command: { name: 'side' }, rest: 'why a queue?' });
    expect(slashCommand('/btw why a queue?', claude)).toMatchObject({ command: { name: 'side' }, rest: 'why a queue?' });
    // Alone, it shows the ones kept from before (SessionReply.tsx).
    expect(slashCommand('/side', session('codex'))?.rest).toBe('');
    expect(slashCommand('/sidebar', claude)).toBeNull();
    // OpenCode has no throwaway copy. One on another machine is asked there.
    expect(slashCommand('/side why?', session('opencode'))).toBeNull();
    expect(slashCommand('/side why?', session('claude', '@ab12:claude:a:1'))?.rest).toBe('why?');
    expect(slashCommand('/fork', session('claude', '@ab12:claude:a:1'))).not.toBeNull();
  });

  it('is only a command where the harness can fork', () => {
    expect(slashCommand('/fork', session('codex'))).not.toBeNull();
    expect(slashCommand('/fork', session('opencode'))).not.toBeNull();
    expect(slashCommand('/fork', session('antigravity'))).toBeNull();
  });
});

describe('slashMatches', () => {
  it('offers the commands a name being typed could become', () => {
    expect(slashMatches('/', claude).map((c) => c.name)).toEqual(['fork', 'side']);
    expect(slashMatches('/s', claude).map((c) => c.name)).toEqual(['side']);
    expect(slashMatches('/bt', claude).map((c) => c.name)).toEqual(['side']);
    expect(slashMatches('/btw what about', claude).map((c) => c.name)).toEqual(['side']);
    expect(slashMatches('/', session('opencode')).map((c) => c.name)).toEqual(['fork']);
    expect(slashMatches('/fo', claude).map((c) => c.name)).toEqual(['fork']);
    expect(slashMatches('/fork and go on', claude).map((c) => c.name)).toEqual(['fork']);
    expect(slashMatches('/co', claude)).toEqual([]);
    expect(slashMatches('/fo', session('antigravity'))).toEqual([]);
    expect(slashMatches('hello', claude)).toEqual([]);
  });
});
