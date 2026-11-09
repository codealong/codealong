// scripts/within.sh, which check-release.sh runs a downloaded app's command under: whatever the
// command does, whoever called the script gets an answer by the limit.
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { expect, test } from "vitest";

const script = fileURLToPath(new URL("./within.sh", import.meta.url));

// Through pipes, as `$(…)` in check-release.sh reads it: spawnSync returns only once nothing holds
// them open. The bounds are far above what these take, as this Mac can be very loaded.
function within(...args: string[]) {
  const started = Date.now();
  const run = spawnSync(script, args, { encoding: "utf8" });
  return { status: run.status, out: run.stdout, err: run.stderr, took: Date.now() - started };
}

test("a command that finishes says what it said and ends as it ended", { timeout: 90_000 }, () => {
  const run = within("60", "bash", "-c", "echo out; echo err >&2; exit 3");
  expect(run.out).toBe("out\nerr\n");
  expect(run.status).toBe(3);
});

test("a command still running at the limit is stopped and fails", { timeout: 90_000 }, () => {
  const run = within("1", "sleep", "600");
  expect(run.status).toBe(124);
  expect(run.err).toBe("sleep was still running after 1 seconds, so it was stopped.\n");
  expect(run.took).toBeLessThan(60_000);
});

test("what a stopped command left running holds nobody up", { timeout: 90_000 }, () => {
  // The killed shell's sleep lives on with the shell's output, as a process that can't be killed
  // would: passed straight through, that output would keep the caller waiting until it ended.
  const run = within("1", "bash", "-c", "echo started; sleep 120; echo late");
  expect(run.status).toBe(124);
  expect(run.out).toBe("started\n");
  expect(run.took).toBeLessThan(60_000);
});

test("it needs a limit and a command", () => {
  expect(within("5").status).toBe(2);
});
