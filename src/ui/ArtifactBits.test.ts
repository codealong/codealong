import { describe, expect, it } from 'vitest';
import { artifactWords } from './ArtifactBits';

const NOW = 1_790_000_000_000;

describe('artifactWords', () => {
  it('says what was last done with it and when', () => {
    expect(artifactWords({ url: 'https://claude.ai/artifact/Vuaw', title: 'Security dashboard', note: 'Updated · version 9', at: NOW - 2 * 3_600_000 }, NOW)).toBe('Security dashboard · Updated · version 9 · 2h ago');
  });

  it('leaves out what its record doesn’t say', () => {
    expect(artifactWords({ url: 'https://claude.ai/artifact/BKx5', title: 'Kestrel Design System', at: 0 }, NOW)).toBe('Kestrel Design System');
  });
});
