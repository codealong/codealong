import { describe, expect, it } from 'vitest';
import { slashCommand, slashMatches } from './slash';
import type { Session } from './types';

const session = (harness: Session['harness']): Session => ({ key: `${harness}:a:1`, harness }) as Session;
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

  it('is only a command where the harness can fork', () => {
    expect(slashCommand('/fork', session('codex'))).not.toBeNull();
    expect(slashCommand('/fork', session('opencode'))).not.toBeNull();
    expect(slashCommand('/fork', session('antigravity'))).toBeNull();
  });
});

describe('slashMatches', () => {
  it('offers the commands a name being typed could become', () => {
    expect(slashMatches('/', claude).map((c) => c.name)).toEqual(['fork']);
    expect(slashMatches('/fo', claude).map((c) => c.name)).toEqual(['fork']);
    expect(slashMatches('/fork and go on', claude).map((c) => c.name)).toEqual(['fork']);
    expect(slashMatches('/co', claude)).toEqual([]);
    expect(slashMatches('/fo', session('antigravity'))).toEqual([]);
    expect(slashMatches('hello', claude)).toEqual([]);
  });
});
