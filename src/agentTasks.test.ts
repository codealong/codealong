import { describe, expect, it } from 'vitest';
import { devSetupTask, devUpdateTask, installCliTask, iphoneTask, syncProjectsTask } from './agentTasks';
import { IPHONE_REPO } from './devices';
import { CODEALONG_REPO } from './improve';

describe('iphoneTask', () => {
  it('asks an agent to install the iPhone app from its repo and link it here, in a chat of its own', () => {
    const { prompt, laneId } = iphoneTask();
    expect(laneId).toBeNull();
    // The steps are the iOS README's: the prompt points there and names only what links the phone.
    expect(prompt).toContain(`https://${IPHONE_REPO}`);
    expect(prompt).toContain('“Get it on your iPhone”');
    expect(prompt).toContain('`scripts/install-device.sh`');
    expect(prompt).toContain('`codealong devices code`');
    // iOS lists the developer to trust only once the app is on the phone.
    expect(prompt.indexOf('trusting the developer')).toBeGreaterThan(prompt.indexOf('`scripts/install-device.sh`'));
  });
});

describe('devSetupTask', () => {
  it('builds from the source this machine has, in its worktree', () => {
    const { prompt, laneId } = devSetupTask('/Users/me/projects/codealong', ['~/projects'], '/Users/me/projects/codealong');
    expect(laneId).toBe('/Users/me/projects/codealong');
    expect(prompt).toContain('Codealong’s source is already in /Users/me/projects/codealong: build from it');
    expect(prompt).not.toContain('Clone');
    expect(prompt).toContain('docs/development.md');
    expect(prompt).toContain('scripts/relaunch.sh');
  });

  it('clones into the folder Codealong looks for repos in, or asks where, from a chat', () => {
    expect(devSetupTask(null, ['~/projects'], null).laneId).toBeNull();
    expect(devSetupTask(null, ['~/projects'], null).prompt).toContain(`Clone ${CODEALONG_REPO} into ~/projects, the folder Codealong looks for repos in`);
    expect(devSetupTask(null, ['~/projects', '~/work'], null).prompt).toContain('one of the folders Codealong looks for repos in (~/projects, ~/work)');
    expect(devSetupTask(null, [], null).prompt).toContain('ask me if you can’t tell');
  });
});

describe('devUpdateTask', () => {
  it('pulls origin/main into the source’s checkout, where it starts, and relaunches, losing nothing', () => {
    const { prompt, laneId } = devUpdateTask({ built: '1a2b3c4d5e6f', commits: 1, source: '/Users/me/projects/codealong', checkedAt: 0 });
    expect(laneId).toBe('/Users/me/projects/codealong');
    expect(prompt).toMatch(/^Update Codealong Dev to the latest origin\/main and relaunch it\. It was built from 1a2b3c4d, and origin\/main has 1 commit since\./);
    expect(prompt).toContain('rather than stash, reset or merge them');
    expect(prompt).toContain('scripts/relaunch.sh');
    expect(prompt).toContain('`git log --oneline 1a2b3c4d..origin/main`');
  });
});

describe('installCliTask', () => {
  it('names the program, its harness and the account’s command, and leaves the steps to the harness’s docs', () => {
    const { prompt, laneId } = installCliTask({ name: 'cursor', harness: 'cursor', command: 'cursor-agent', cli: 'cursor-agent' });
    expect(laneId).toBeNull();
    expect(prompt).toMatch(/^Install cursor-agent, Cursor’s command line, on this machine\./);
    expect(prompt).toContain('my Cursor account cursor with `cursor-agent`');
    expect(prompt).toContain('the way Cursor’s own docs say');
    expect(prompt).toContain('`command -v cursor-agent`');
    expect(prompt).toContain('Settings › Accounts has Sign in for cursor');
  });

  it('keeps an account’s own command, which sets its folder, for any harness', () => {
    const { prompt } = installCliTask({ name: 'claude-work', harness: 'claude', command: 'CLAUDE_CONFIG_DIR=~/.claude-work claude', cli: 'claude' });
    expect(prompt).toMatch(/^Install claude, Claude Code’s command line/);
    expect(prompt).toContain('with `CLAUDE_CONFIG_DIR=~/.claude-work claude`');
  });
});

describe('syncProjectsTask', () => {
  it('lists the other machine’s projects and asks before changing anything here', () => {
    const { prompt, laneId } = syncProjectsTask('Mac mini', [
      { name: 'Northwind', repos: ['github.com/northwind/app', 'github.com/northwind/api'] },
      { name: 'Atlas', repos: [] },
    ]);
    expect(laneId).toBeNull();
    expect(prompt).toMatch(/^Make this machine’s Codealong projects match my Mac mini’s/);
    expect(prompt).toContain('\n- Northwind: github.com/northwind/app, github.com/northwind/api\n- Atlas: no repos yet\n');
    expect(prompt).toContain('before changing anything tell me what you’d change here');
    expect(prompt).toContain('don’t rename or remove a project without asking');
    expect(prompt).toContain('`codealong config check`');
  });
});
