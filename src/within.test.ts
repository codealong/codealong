// scripts/within.sh, which check-release.sh runs a downloaded app's command under: whatever the
// command does, whoever called the script gets an answer by the limit. It's here rather than
// beside the script so that typecheck and lint read it.
// The project has no types for node (the tests are the only code that runs there).
// @ts-expect-error no @types/node
import { spawn } from 'node:child_process';
import { expect, test } from 'vitest';

const script: string = new URL('../scripts/within.sh', import.meta.url).pathname;

type Run = { status: number | null; out: string; err: string };

// Runs it with pipes for its output, as `$(…)` in check-release.sh reads it, and answers once
// nothing holds them open, which is when that caller would go on. One that takes past the
// deadline fails by itself, the test's timer not reaching a wait that blocks, and its process
// group is killed, as it is after every run, so nothing it started outlives the test. The
// deadline is far above what these take, as this Mac can be very loaded.
function within(args: string[], deadline = 30_000): Promise<Run> {
  return new Promise((done, fail) => {
    const child = spawn(script, args, { detached: true });
    const killGroup = () => {
      try {
        // @ts-expect-error no @types/node
        process.kill(-child.pid, 'SIGKILL');
      } catch {
        // The group is gone already: everything in it ended.
      }
    };
    let out = '';
    let err = '';
    child.stdout.on('data', (d: unknown) => (out += String(d)));
    child.stderr.on('data', (d: unknown) => (err += String(d)));
    const timer = setTimeout(() => {
      killGroup();
      fail(new Error(`within.sh ${args.join(' ')} held its output past ${deadline} ms`));
    }, deadline);
    child.on('close', (status: number | null) => {
      clearTimeout(timer);
      killGroup();
      done({ status, out, err });
    });
  });
}

test('a command that finishes says what it said and ends as it ended', { timeout: 60_000 }, async () => {
  const run = await within(['60', 'bash', '-c', 'echo out; echo err >&2; exit 3']);
  expect(run.out).toBe('out\nerr\n');
  expect(run.status).toBe(3);
});

test('a command still running at the limit is stopped and fails', { timeout: 60_000 }, async () => {
  const run = await within(['1', 'sleep', '600']);
  expect(run.status).toBe(124);
  expect(run.err).toBe('sleep was still running after 1 seconds, so it was stopped.\n');
});

test('what a stopped command left running holds nobody up', { timeout: 60_000 }, async () => {
  // The killed shell's sleep lives on with the shell's output, as a process that can't be killed
  // would: passed straight through, that output would keep the caller waiting until it ended.
  const run = await within(['1', 'bash', '-c', 'echo started; sleep 120; echo late']);
  expect(run.status).toBe(124);
  expect(run.out).toBe('started\n');
});

test('it needs a limit and a command', { timeout: 60_000 }, async () => {
  expect((await within(['5'])).status).toBe(2);
});
