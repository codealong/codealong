// Checks talking into the quick panel (Quick.tsx, talk.ts; dictation.rs sends the events): with
// voice on the box has a microphone, and a click on it begins a press and another ends it; what's
// heard sits in the box as it's heard, selected, the microphone showing it listens; what was
// said takes its place as the press ends, the caret after it; said after something written, it
// goes on from the caret, and coming to nothing it's taken out again; a press that fails says
// why; opened only to hear, with nothing said, the panel puts itself away; and with Shift down
// as the key comes up, what was said starts a session at once, said to be dictated.
//   PREVIEW_VOICE=1 PREVIEW_WAIT=60000 scripts/preview.sh quick '' scripts/preview/quick-talk-check.js
(function () {
  ['codealong.quick.draft', 'codealong.quick.history'].forEach(function (k) { localStorage.removeItem(k); });
  localStorage.setItem('codealong.quick.project', 'All projects');
  var input = function () { return q('.ca-quick-input textarea'); };
  var mic = function () { return q('.ca-quick-mic'); };
  var id = 0;
  var talk = function (over) { window.__emit('talk', Object.assign({ id: id, phase: 'listening', to: 'looking', text: '', level: 0.2 }, over)); };
  var selected = function () { return input().value.slice(input().selectionStart, input().selectionEnd); };
  var NOTE = '(Dictated, and sent as I let go of the key: some words may be misheard.)';
  var board;

  say('quick talk check: waiting for the panel');
  until(function () { return window.__quickOpen && input(); }, 20000, 'the quick panel’s page')
    .then(function () { return window.__TAURI_INTERNALS__.invoke('get_board'); })
    .then(function (b) {
      board = Object.assign({}, b, { talkShortcut: 'Option+Cmd+[' });
      window.__quickOpen({}, board);
      return until(mic, 5000, 'the microphone');
    })
    .then(function () {
      check('with voice on, the box has a microphone', mic().getAttribute('aria-pressed') === 'false' && /hold ⌥⌘\[ from any app/.test(mic().title), mic().title);
      mic().click();
      return until(function () { return sent('talk').length === 1; }, 3000, 'the press begun');
    })
    .then(function () {
      check('a click on it begins a press', last('talk').on === true, JSON.stringify(last('talk')));
      id = 1;
      talk({});
      return until(function () { return mic().classList.contains('is-on'); }, 3000, 'the microphone listening');
    })
    .then(function () {
      check('the microphone says it’s listening, the box still empty', mic().getAttribute('aria-pressed') === 'true' && !!mic().querySelector('.ca-spoken-level') && input().value === '' && input().placeholder === 'Listening…', input().placeholder);
      talk({ text: 'Fix the login' });
      return until(function () { return input().value === 'Fix the login'; }, 3000, 'the words as they’re heard');
    })
    .then(function () {
      check('what’s heard sits in the box, selected', selected() === 'Fix the login' && document.activeElement === input(), selected());
      talk({ text: 'Fix the login redirect on' });
      return until(function () { return input().value === 'Fix the login redirect on'; }, 3000, 'the words as they change');
    })
    .then(function () {
      check('and changes as it’s heard', selected() === 'Fix the login redirect on', selected());
      mic().click();
      return until(function () { return sent('talk').length === 2; }, 3000, 'the press ended');
    })
    .then(function () {
      check('another click ends the press', last('talk').on === false, JSON.stringify(last('talk')));
      talk({ phase: 'settling', text: 'Fix the login redirect on the' });
      window.__emit('talked', { text: 'Fix the login redirect on the settings page.' });
      talk({ phase: 'asked', text: 'Fix the login redirect on the settings page.' });
      return until(function () { return !mic().classList.contains('is-on'); }, 3000, 'the microphone at rest');
    })
    .then(function () {
      var said = 'Fix the login redirect on the settings page.';
      check('what was said takes its place, the caret after it', input().value === said && input().selectionStart === said.length && input().selectionEnd === said.length, input().value + ' @' + input().selectionStart);
      // Said after what's written: on from the caret, and taken out again when it comes to nothing.
      id = 2;
      talk({ text: 'and add a test' });
      return until(function () { return /and add a test$/.test(input().value); }, 3000, 'more, after what’s written');
    })
    .then(function () {
      check('more goes on from the caret, a space before it', input().value === 'Fix the login redirect on the settings page. and add a test' && selected() === ' and add a test', input().value);
      talk({ phase: 'nothing', message: 'Nothing heard. Hold the keys down while you talk.' });
      return until(function () { return input().value === 'Fix the login redirect on the settings page.' && q('.ca-quick-error.is-info'); }, 3000, 'what came to nothing taken out, and said');
    })
    .then(function (said) {
      check('what came to nothing is taken out, and the panel says so and stays with what it holds', calls('hide_quick').length === 0 && /Nothing heard/.test(said.textContent), said.textContent);
      id = 3;
      talk({ phase: 'failed', message: 'macOS isn’t letting Codealong use the microphone. Allow it in System Settings › Privacy & Security › Microphone.' });
      return until(function () { return q('.ca-quick-error:not(.is-info)'); }, 3000, 'why it failed');
    })
    .then(function (error) {
      check('a press that fails says why', /isn’t letting Codealong use the microphone/.test(error.textContent), error.textContent);
      // Opened only to hear it, from another app, and nothing was said: it goes away again.
      window.__quickOpen({ talk: true }, board);
      id = 4;
      talk({});
      return wait(200);
    })
    .then(function () {
      check('opened to hear, it’s an ordinary open, listening', input().value === '' && mic().classList.contains('is-on') && !!q('[data-row="compose"]'), input().value);
      talk({ phase: 'nothing', message: 'Nothing heard. Hold the keys down while you talk.' });
      return until(function () { return calls('hide_quick').length === 1; }, 3000, 'the panel put away');
    })
    .then(function () {
      check('with nothing said, it puts itself away', true);
      // Shift down as the key came up: it starts a session with what was said, as ⌘⏎ would.
      window.__quickOpen({ talk: true }, board);
      id = 5;
      talk({ text: 'Summarize the open PRs' });
      return until(function () { return input().value === 'Summarize the open PRs'; }, 3000, 'the words heard');
    })
    .then(function () {
      var starts = sent('start_from_quick').length;
      window.__emit('talked', { text: 'Summarize the open PRs.\n\n' + NOTE, start: true });
      talk({ phase: 'started', text: 'Summarize the open PRs.' });
      return until(function () { return sent('start_from_quick').length > starts; }, 5000, 'the session started');
    })
    .then(function () {
      var spec = last('start_from_quick').spec;
      check('with Shift down as the key comes up, it starts a session with it', spec.prompt.indexOf('Summarize the open PRs.') === 0, spec.prompt);
      check('said to be dictated', spec.prompt.indexOf(NOTE) > 0, spec.prompt);
      check('and the box is empty for the next', input().value === '', input().value);
      done();
    })
    .catch(stopped);
})();
