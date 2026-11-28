import { describe, expect, it } from 'vitest';
import { agentId } from '../start';
import { parseMovePick } from './SessionAgent';

describe('the session chip’s lines', () => {
  it('tells a move to another account from a model of its own, whatever the account is named', () => {
    expect(parseMovePick(':move:claude2')).toEqual({ account: 'claude2', model: null });
    expect(parseMovePick(':move:codex|')).toEqual({ account: 'codex', model: null });
    expect(parseMovePick(':move:codex|gpt-5.5')).toEqual({ account: 'codex', model: 'gpt-5.5' });
    // An account may be named `move`: its models are still its own.
    expect(parseMovePick(agentId('move', 'opus'))).toBeNull();
    expect(parseMovePick(agentId('move', null))).toBeNull();
    expect(parseMovePick(agentId('claude', 'opus'))).toBeNull();
  });
});
