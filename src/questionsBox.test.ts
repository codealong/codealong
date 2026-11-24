import { describe, expect, it } from 'vitest';
import type { Question } from './types';
import { askedOf, calledOf, foldQuestions, questionsFolded, showCall, shownCall, showsCall } from './questionsBox';

describe('questionsBox', () => {
  it('says what the box shows while it shows it, and whether it’s minimized', () => {
    expect(shownCall('s1')).toBeNull();
    const gone = showCall('s1', 'ask-1', 'Which color?');
    expect(shownCall('s1')).toEqual({ call: 'ask-1', asked: 'Which color?' });
    expect(questionsFolded('s1', 'ask-1')).toBe(false);
    foldQuestions('s1', 'ask-1', true);
    expect(questionsFolded('s1', 'ask-1')).toBe(true);
    foldQuestions('s1', 'ask-1', false);
    expect(questionsFolded('s1', 'ask-1')).toBe(false);
    gone();
    expect(shownCall('s1')).toBeNull();
  });

  it('stays minimized as the box goes and comes back for the same call, and not for the next', () => {
    showCall('s2', 'ask-1', 'Which color?')();
    foldQuestions('s2', 'ask-1', true);
    showCall('s2', 'ask-1', 'Which color?');
    expect(questionsFolded('s2', 'ask-1')).toBe(true);
    showCall('s2', 'ask-2', 'Which color?');
    expect(questionsFolded('s2', 'ask-2')).toBe(false);
    expect(questionsFolded('s2', null)).toBe(false);
  });

  it('leaves the next call’s box alone when the one before it goes', () => {
    const first = showCall('s3', 'ask-1', 'Which color?');
    showCall('s3', 'ask-2', 'Which days?');
    first();
    expect(shownCall('s3')?.call).toBe('ask-2');
  });

  it('knows questions by their words and their choices, whatever their headers', () => {
    const approach: Question = { question: 'Which approach?', options: [{ label: 'Keep the cache' }, { label: 'Remove the cache' }] };
    const asked = askedOf([approach]);
    const headed: Question = { ...approach, question: ' Which approach? ', header: 'Approach', multiSelect: true };
    expect(askedOf([headed])).toBe(asked);
    expect(askedOf([{ ...approach, options: [{ label: 'Use polling' }, { label: 'Use events' }] }])).not.toBe(asked);
    expect(askedOf([approach, { question: 'Which days?', options: [] }])).not.toBe(asked);
  });

  it('knows a call to allow by its label and its input, and never for questions', () => {
    const called = calledOf('Bash · `npm test`', 'npm test');
    expect(calledOf(' Bash · `npm test` ', 'npm test\n')).toBe(called);
    expect(calledOf('Bash · `npm run build`', 'npm run build')).not.toBe(called);
    // A call labeled as questions are asked still isn't them.
    const color: Question = { question: 'Which color?', options: [{ label: 'Red' }] };
    expect(calledOf('Asks you · Which color?', null)).not.toBe(askedOf([color]));
  });

  it('tells the line of the call the box shows from another call waiting with the same label', () => {
    // Two calls of one MCP tool, whose label is its name alone: the box shows the one asked first,
    // and the conversation marks the latest, which keeps its line.
    const label = 'mcp__linear__create_issue';
    const box = { call: 'ask-1', asked: calledOf(label, '{"title":"First"}') };
    expect(showsCall(box, { text: label, input: '{"title":"First"}' })).toBe(true);
    expect(showsCall(box, { text: label, input: '{"title":"Second"}' })).toBe(false);
    expect(showsCall(box, { text: label })).toBe(false);
    expect(showsCall(null, { text: label, input: '{"title":"First"}' })).toBe(false);
    // Questions are known by what they ask, whatever the call's label.
    const color: Question = { question: 'Which color?', options: [{ label: 'Red' }, { label: 'Blue' }] };
    const asking = { call: 'ask-2', asked: askedOf([color]) };
    expect(showsCall(asking, { text: 'AskUserQuestion', input: '{}', questions: [{ ...color, header: 'Color' }] })).toBe(true);
    expect(showsCall(asking, { text: 'AskUserQuestion', input: '{}', questions: [{ ...color, options: [{ label: 'Red' }] }] })).toBe(false);
    expect(showsCall(asking, { id: 'ask-2', text: 'Bash · `ls`', input: 'ls' })).toBe(false);
  });

  it('matches a call whose approval label words or clips the line differently from the transcript', () => {
    // Antigravity over ACP (`run_command`, 100-char single-line title, its own call id) vs its
    // transcript (`Run command`, 80-char first line, step index id).
    const cmd = 'python3 -m unittest discover -s scripts\npython3 -c "print(1)"';
    const acp = { call: 'call_4570', asked: calledOf('run_command · `python3 -m unittest discover -s scripts python3 -c "print(1)"`', cmd) };
    expect(showsCall(acp, { id: '24', text: 'Run command · `python3 -m unittest discover -s scripts`', input: cmd })).toBe(true);
    // Another run_command waiting beside it with a different command still keeps its line.
    expect(showsCall(acp, { id: '23', text: 'Run command · `git status`', input: 'git status' })).toBe(false);
    // Claude Code `Bash` with `cd … &&` stripped in the transcript, and OpenCode `Bash` vs `Shell`.
    expect(showsCall({ call: 'ask-3', asked: calledOf('Bash · `cd /app && npm test`', 'cd /app && npm test') }, { text: 'Bash · `npm test`', input: 'cd /app && npm test' })).toBe(true);
    expect(showsCall({ call: 'per_1', asked: calledOf('Bash · `ls`', 'ls') }, { text: 'Shell · `ls`', input: 'ls' })).toBe(true);
    // Non-command snake_case tool (`write_to_file`) vs its transcript line (`Write to file`).
    expect(showsCall({ call: 'call_2', asked: calledOf('write_to_file · `Fix notes.txt`', '{"path":"notes.txt"}') }, { id: '7', text: 'Write to file · Fix notes.txt', input: '{"path":"notes.txt"}' })).toBe(true);
    // MCP `mcp__server__tool` matches `server · tool`, while distinct tools on one server stay apart.
    expect(showsCall({ call: 'ask-4', asked: calledOf('mcp__linear__create_issue', '{}') }, { text: 'linear · create_issue', input: '{}' })).toBe(true);
    expect(showsCall({ call: 'ask-4', asked: calledOf('linear · create_issue', '{}') }, { text: 'linear · delete_issue', input: '{}' })).toBe(false);
    // Distinct tools with the same path stay apart unless they share a call id.
    expect(showsCall({ call: 'ask-5', asked: calledOf('Read · `a.ts`', '/app/a.ts') }, { id: 'toolu_2', text: 'Write · `a.ts`', input: '/app/a.ts' })).toBe(false);
    // Same non-empty call id (Claude ACP's `toolCallId` on `Read`/`Edit` whose raw JSON differs from
    // the transcript's summary input) matches directly; empty ids never do.
    expect(showsCall({ call: 'toolu_9', asked: calledOf('Read · `Read src/a.ts`', '{"file_path":"/app/src/a.ts"}') }, { id: 'toolu_9', text: 'Read · `a.ts`', input: '/app/src/a.ts' })).toBe(true);
    expect(showsCall({ call: '', asked: calledOf('Read · `a.ts`', '/app/a.ts') }, { id: '', text: 'Write · `b.ts`', input: '/app/b.ts' })).toBe(false);
  });
});
