import { test } from 'vitest';

// A probe of main's merge queue, made to fail so that it can never land (one).
test('a probe of the merge queue, never to land', () => {
  throw new Error('this pull request is a probe, not a change');
});
