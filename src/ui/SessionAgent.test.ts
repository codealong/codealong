import { describe, expect, it } from 'vitest';
import { agentId } from '../start';
import { moveTarget } from './SessionAgent';

describe('the session chip’s lines', () => {
  it('tells a move to another account from a model of its own, whatever the account is named', () => {
    expect(moveTarget(':move:claude2')).toBe('claude2');
    // An account may be named `move`: its models are still its own.
    expect(moveTarget(agentId('move', 'opus'))).toBeNull();
    expect(moveTarget(agentId('move', null))).toBeNull();
    expect(moveTarget(agentId('claude', 'opus'))).toBeNull();
  });
});
