// @vitest-environment jsdom
import { describe, expect, it, vi } from 'vitest';
import { act, engine, settle } from '../testing/dom';
import { createElement } from 'react';
import { createRoot } from 'react-dom/client';
import * as fx from '../testing/fixtures';
import type { Pending, Question } from '../types';
import { Reply } from './SessionReply';

// A form an agent asks a session Codealong runs to fill in (acp/forms.rs), as the engine sends it:
// what it says it wants, and a question for each field, with what each takes.
const region: Question = { question: 'Region', options: [{ label: 'United States' }, { label: 'Europe' }], form: { choicesOnly: true, required: true } };
const email: Question = { question: 'Where to send the report', header: 'Email', options: [], form: { required: true, hint: 'An email address' } };
const token: Question = { question: 'Paste the deploy token', options: [], form: { secret: true } };
const form: Pending = { id: 'form-7', label: 'Asks you · The deploy server needs a few details.', input: 'The deploy server needs a few details.', cannot: null, questions: [region, email, token] };

// Claude Code's AskUserQuestion in a session Codealong runs, which claude-agent-acp asks as a form.
const fixWhere: Question = { question: 'What should I change?', header: 'Fix where', options: [{ label: 'Main repo' }, { label: 'Phone only' }], form: {} };
const asked: Pending = { id: 'form-9', label: 'Asks you · What should I change?', input: null, cannot: null, questions: [fixWhere] };

function mount(pending: Pending = form, session = fx.session({ key: 'codex:codex:t1', harness: 'codex', account: 'codex', status: 'needs', need: 'question', live: true, hosted: true, asks: false })) {
  engine.answer('device_start_options', () => ({ accounts: [], codexProjects: [], claudeModels: [] }));
  engine.answer('session_pending', () => pending);
  const host = document.body.appendChild(document.createElement('div'));
  const root = createRoot(host);
  act(() => root.render(createElement(Reply, { session, terminal: 'Ghostty', onOpen: vi.fn() })));
  return {
    host,
    unmount: () => {
      act(() => root.unmount());
      host.remove();
    },
  };
}

const click = (el: Element | null | undefined) => act(() => (el as HTMLElement).click());
const button = (host: HTMLElement, words: string) => [...host.querySelectorAll('.ca-composer-foot button')].find((b) => b.textContent?.includes(words)) as HTMLButtonElement | undefined;
// Types into a box as a person does, so React hears it.
const write = (input: HTMLInputElement, text: string) =>
  act(() => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!.call(input, text);
    input.dispatchEvent(new Event('input', { bubbles: true }));
  });
const step = (host: HTMLElement, words: string) => [...host.querySelectorAll('.ca-question-steps button')].find((b) => b.textContent?.includes(words));
// Types into the session's message box and presses Return there.
const sendFromBox = (host: HTMLElement, text: string) => {
  const box = host.querySelector('.ca-composer textarea[aria-label^="Message to"]') as HTMLTextAreaElement;
  act(() => {
    Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value')!.set!.call(box, text);
    box.dispatchEvent(new Event('input', { bubbles: true }));
  });
  act(() => {
    box.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }));
  });
};

describe('a form in the reply box', () => {
  it('says what it wants, asks each field for what it takes, and goes once what it needs is answered', async () => {
    const { host, unmount } = mount();
    await settle();
    await settle();
    const box = host.querySelector('.ca-composer.is-questions');
    expect(box?.querySelector('.ca-approval-plan')?.textContent).toContain('The deploy server needs a few details.');
    // Its choices are all the region takes: no box for words of your own.
    expect(box?.querySelector('.ca-question-own')).toBeNull();
    expect(button(host, 'Decline')?.title).toContain('its agent is told and goes on');
    click([...host.querySelectorAll('.ca-question-choice')].find((c) => c.textContent?.includes('Europe')));
    // On to the email, whose box says what it takes.
    const own = () => host.querySelector('.ca-question-own') as HTMLInputElement | null;
    expect(own()?.placeholder).toBe('An email address');
    expect(own()?.type).toBe('text');
    // The token's is hidden as it's typed.
    click(step(host, 'Question 3'));
    expect(own()?.type).toBe('password');
    write(own()!, 'abc');
    // The email is needed: with only it left, Answer says so until it's given.
    click(step(host, 'Email'));
    expect(button(host, 'Answer')?.disabled).toBe(true);
    expect(button(host, 'Answer')?.title).toBe('Answer “Email” first: the form needs it');
    write(own()!, 'me@example.com');
    engine.answer('session_answer', () => 'Answered.');
    expect(button(host, 'Answer')?.disabled).toBe(false);
    click(button(host, 'Answer'));
    await settle();
    const sent = engine.calls.filter((c) => c.cmd === 'session_answer').at(-1);
    expect(sent?.args).toMatchObject({ key: 'codex:codex:t1', id: 'form-7', answer: 'allow', answers: { Region: 'Europe', 'Where to send the report': 'me@example.com', 'Paste the deploy token': 'abc' } });
    unmount();
  });

  // Its questions hold the turn, so a message sent instead would wait behind them for good: it
  // declines them with what you wrote, as in any other session.
  it('declines the questions with what you write in the message box, rather than queueing it behind them', async () => {
    const session = fx.session({ key: 'claude:claude:s9', harness: 'claude', account: 'claude', status: 'needs', need: 'question', excerpt: asked.label, live: true, hosted: true, asks: false });
    const { host, unmount } = mount(asked, session);
    await settle();
    await settle();
    expect(host.querySelector('.ca-composer textarea[aria-label^="Message to"]')?.getAttribute('aria-placeholder')).toBe('Or decline it, and write to claude instead');
    engine.answer('session_answer', () => 'Sent what you wrote as its answer.');
    sendFromBox(host, 'Use the main repo, but look at the helper triage first');
    await settle();
    const answered = engine.calls.filter((c) => c.cmd === 'session_answer').at(-1);
    expect(answered?.args).toMatchObject({ key: 'claude:claude:s9', id: 'form-9', answer: 'deny', answers: null, message: 'Use the main repo, but look at the helper triage first' });
    expect(engine.calls.some((c) => c.cmd === 'send_to_session')).toBe(false);
    unmount();
  });
});
