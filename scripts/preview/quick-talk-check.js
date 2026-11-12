// Checks talking into the quick panel (Quick.tsx; dictation.rs sends the events): with voice on
// the box has a microphone, and a click on it begins a press and another ends it; what's heard
// shows in the box as it's heard, selected, the microphone showing it listens, while what the
// box holds is untouched (nothing looked for, no draft kept); what was said is typed in its
// place as the press ends, the caret after it; said over a selection, it shows in its place, and
// coming to nothing leaves the selection as it was; an earlier press ending says nothing over
// the one being heard; a press that fails says why; opened only to hear, with nothing said, the
// panel puts itself away; with Shift down as the key comes up, what was said starts a session
// at once, said to be dictated; a press the panel has been opened for something else since is
// typed where the keyboard is and starts nothing, and so is one begun while another field had
// the keyboard; while a press is heard nothing is done with the prompt, and ⏎ ends it; two
// presses one after the other are typed in the order they were said, with what follows them
// kept; an earlier press coming to nothing doesn't put the panel away under a later one, nor a
// press it was opened to hear once it's been opened for something else; and the panel put away
// while it hears drops that press, by its id, whichever of its fields had the keyboard.
//   PREVIEW_VOICE=1 PREVIEW_WAIT=60000 scripts/preview.sh quick '' scripts/preview/quick-talk-check.js
(function () {
  ['codealong.quick.draft', 'codealong.quick.history'].forEach(function (k) { localStorage.removeItem(k); });
  localStorage.setItem('codealong.quick.project', 'All projects');
  var input = function () { return q('.ca-quick-input textarea'); };
  var mic = function () { return q('.ca-quick-mic'); };
  var id = 0;
  var talk = function (over) { window.__emit('talk', Object.assign({ id: id, phase: 'listening', to: 'looking', text: '', level: 0.2 }, over)); };
  var said = function (text, more) { window.__emit('talked', Object.assign({ id: id, text: text }, more)); };
  var selected = function () { return input().value.slice(input().selectionStart, input().selectionEnd); };
  var looked = function () { return sent('search_text').map(function (a) { return a.words; }).concat(sent('route_quick').map(function (a) { return a.text; })); };
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
      check('and the box isn’t typed into while it’s heard into, from the first', input().readOnly === true);
      talk({ text: 'Fix the login' });
      return until(function () { return input().value === 'Fix the login'; }, 3000, 'the words as they’re heard');
    })
    .then(function () {
      check('what’s heard shows in the box, selected', selected() === 'Fix the login' && document.activeElement === input(), selected());
      talk({ text: 'Fix the login redirect on' });
      return until(function () { return input().value === 'Fix the login redirect on'; }, 3000, 'the words as they change');
    })
    .then(function () {
      check('and changes as it’s heard', selected() === 'Fix the login redirect on', selected());
      // Long enough for a search of what's typed to have gone, had these been typed.
      return wait(900);
    })
    .then(function () {
      check('what the box holds is untouched meanwhile: nothing is looked for, no draft kept', looked().length === 0 && localStorage.getItem('codealong.quick.draft') === null, looked().join(' | ') + ' ' + localStorage.getItem('codealong.quick.draft'));
      mic().click();
      return until(function () { return sent('talk').length === 2; }, 3000, 'the press ended');
    })
    .then(function () {
      check('another click ends the press', last('talk').on === false, JSON.stringify(last('talk')));
      talk({ phase: 'settling', text: 'Fix the login redirect on the' });
      said('Fix the login redirect on the settings page.');
      talk({ phase: 'asked', text: 'Fix the login redirect on the settings page.' });
      return until(function () { return !mic().classList.contains('is-on'); }, 3000, 'the microphone at rest');
    })
    .then(function () {
      var text = 'Fix the login redirect on the settings page.';
      check('what was said is typed in its place, the caret after it', input().value === text && input().selectionStart === text.length && input().selectionEnd === text.length && !input().readOnly, input().value + ' @' + input().selectionStart);
      // Said after what's written: on from the caret, a space before it.
      id = 2;
      talk({ text: 'and add a test' });
      return until(function () { return /and add a test$/.test(input().value); }, 3000, 'more, after what’s written');
    })
    .then(function () {
      check('more shows on from the caret, a space before it', input().value === 'Fix the login redirect on the settings page. and add a test' && selected() === ' and add a test', input().value);
      talk({ phase: 'nothing', message: 'Nothing heard. Hold the keys down while you talk.' });
      return until(function () { return input().value === 'Fix the login redirect on the settings page.' && q('.ca-quick-error.is-info'); }, 3000, 'what came to nothing gone, and said');
    })
    .then(function (note) {
      check('what came to nothing goes, and the panel says so and stays with what it holds', calls('hide_quick').length === 0 && /Nothing heard/.test(note.textContent), note.textContent);
      // Said over a selection: in its place while it's heard, and the selection as it was after.
      type(input(), 'Keep this draft');
      input().setSelectionRange(5, 9);
      id = 3;
      talk({ text: 'that' });
      return until(function () { return input().value === 'Keep that draft'; }, 3000, 'the words over the selection');
    })
    .then(function () {
      check('over a selection, what’s heard shows in its place', selected() === 'that', selected());
      // An earlier press's words settling say nothing over the one being heard.
      window.__emit('talk', { id: 2, phase: 'typed', to: 'looking', text: 'Earlier.', level: 0 });
      return wait(200);
    })
    .then(function () {
      check('an earlier press ending leaves what’s being heard', input().value === 'Keep that draft' && mic().classList.contains('is-on'), input().value);
      talk({ phase: 'failed', message: 'macOS isn’t letting Codealong use the microphone. Allow it in System Settings › Privacy & Security › Microphone.' });
      return until(function () { return q('.ca-quick-error:not(.is-info)'); }, 3000, 'why it failed');
    })
    .then(function (error) {
      check('a press that fails says why', /isn’t letting Codealong use the microphone/.test(error.textContent), error.textContent);
      check('and leaves the box as it was, what was selected selected', input().value === 'Keep this draft' && selected() === 'this', input().value + ' [' + selected() + ']');
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
      said('Summarize the open PRs.\n\n' + NOTE, { start: true });
      talk({ phase: 'started', text: 'Summarize the open PRs.' });
      return until(function () { return sent('start_from_quick').length > starts; }, 5000, 'the session started');
    })
    .then(function () {
      var spec = last('start_from_quick').spec;
      check('with Shift down as the key comes up, it starts a session with it', spec.prompt.indexOf('Summarize the open PRs.') === 0, spec.prompt);
      check('said to be dictated', spec.prompt.indexOf(NOTE) > 0, spec.prompt);
      check('and the box is empty for the next', input().value === '', input().value);
      // Opened for something else while a press was being heard: what was said then goes where
      // the keyboard is, as typing would, and starts nothing, Shift or not.
      window.__quickOpen({}, board);
      id = 6;
      talk({ text: 'Deploy' });
      return until(function () { return input().value === 'Deploy'; }, 3000, 'the words heard');
    })
    .then(function () {
      window.__quickOpen({}, board);
      type(input(), 'Another prompt');
      input().setSelectionRange(14, 14);
      return wait(200);
    })
    .then(function () {
      check('opened again, the box holds only what’s written in it', input().value === 'Another prompt' && !input().readOnly, input().value);
      var starts = sent('start_from_quick').length;
      said('Deploy it.\n\n' + NOTE, { start: true });
      talk({ phase: 'started', text: 'Deploy it.' });
      return wait(600).then(function () { return starts; });
    })
    .then(function (starts) {
      check('a press from before the panel was opened again starts nothing', sent('start_from_quick').length === starts, String(sent('start_from_quick').length - starts));
      check('and is typed where the keyboard is', input().value.indexOf('Another prompt Deploy it.') === 0, input().value);
      // Begun while the box didn't have the keyboard, then the panel opened again: not the
      // box's to show, in this open or the next, and nothing of it starts.
      window.__quickOpen({}, board);
      input().blur();
      id = 7;
      talk({ text: 'Ship' });
      window.__quickOpen({}, board);
      talk({ text: 'Ship it' });
      return wait(300);
    })
    .then(function () {
      check('a press begun outside the box isn’t shown in it, in a later open either', input().value === '' && !input().readOnly, input().value);
      var starts = sent('start_from_quick').length;
      said('Ship it.\n\n' + NOTE, { start: true });
      talk({ phase: 'started', text: 'Ship it.' });
      return wait(600).then(function () { return starts; });
    })
    .then(function (starts) {
      check('and starts nothing', sent('start_from_quick').length === starts && input().value.indexOf('Ship it.') === 0, input().value);
      // While a press is heard into the box, nothing is done with the prompt; ⏎ ends the press.
      window.__quickOpen({}, board);
      type(input(), 'An old draft');
      input().setSelectionRange(0, 12);
      id = 8;
      talk({ text: 'A new prompt' });
      return until(function () { return input().value === 'A new prompt'; }, 3000, 'the words over the draft');
    })
    .then(function () {
      var starts = sent('start_from_quick').length;
      var presses = sent('talk').length;
      key(input(), { key: 'Enter', metaKey: true });
      return wait(400).then(function () { return [starts, presses]; });
    })
    .then(function (was) {
      check('while a press is heard, ⌘⏎ starts nothing', sent('start_from_quick').length === was[0], String(sent('start_from_quick').length - was[0]));
      check('and ⏎ ends the press', sent('talk').length === was[1] + 1 && last('talk').on === false && !last('talk').drop, JSON.stringify(last('talk')));
      said('A new prompt.');
      talk({ phase: 'asked', text: 'A new prompt.' });
      return until(function () { return input().value === 'A new prompt.' && !input().readOnly; }, 3000, 'what was said typed over the draft');
    })
    .then(function () {
      // Two presses, the second begun while the first's words still show: typed in the order
      // they were said, and what follows them in the box is kept.
      type(input(), 'Keep suffix');
      input().setSelectionRange(5, 5);
      id = 9;
      talk({ text: 'one phrase' });
      return until(function () { return input().value === 'Keep one phrasesuffix'; }, 3000, 'the first press shown');
    })
    .then(function () {
      id = 10;
      talk({ text: 'two' });
      return until(function () { return input().value === 'Keep twosuffix'; }, 3000, 'the second press shown from the same place');
    })
    .then(function () {
      window.__emit('talked', { id: 9, text: 'One.' });
      window.__emit('talk', { id: 9, phase: 'asked', to: 'looking', text: 'One.', level: 0 });
      return until(function () { return input().value === 'Keep One. twosuffix'; }, 3000, 'the first typed, the second after it');
    })
    .then(function () {
      check('an earlier press is typed first, and the one being heard goes on after it', selected() === ' two' && mic().classList.contains('is-on'), input().value + ' [' + selected() + ']');
      said('Two.');
      talk({ phase: 'asked', text: 'Two.' });
      return until(function () { return input().value === 'Keep One. Two.suffix'; }, 3000, 'both typed, in order');
    })
    .then(function () {
      check('both are typed in the order they were said, with what follows kept', input().value === 'Keep One. Two.suffix', input().value);
      // Opened to hear a press that came to nothing while a later one is being heard: it stays.
      window.__quickOpen({ talk: true }, board);
      var hides = calls('hide_quick').length;
      id = 11;
      talk({});
      id = 12;
      talk({ text: 'Second press' });
      window.__emit('talk', { id: 11, phase: 'nothing', to: 'looking', text: '', level: 0, message: 'Nothing heard. Hold the keys down while you talk.' });
      return wait(400).then(function () { return hides; });
    })
    .then(function (hides) {
      check('an earlier press coming to nothing doesn’t put the panel away under a later one', calls('hide_quick').length === hides && input().value === 'Second press' && mic().classList.contains('is-on'), input().value);
      // Put away while it hears: the press is dropped, not left listening.
      var presses = sent('talk').length;
      window.__emit('quick-hidden', null);
      return until(function () { return sent('talk').length > presses; }, 3000, 'the press dropped');
    })
    .then(function () {
      check('put away while it hears, the panel drops that press', last('talk').drop === 12, JSON.stringify(last('talk')));
      // A press begun with the keyboard on the microphone itself isn't shown in the box, and is
      // the panel's to drop all the same.
      window.__quickOpen({}, board);
      mic().focus();
      id = 13;
      talk({ text: 'Heard for the panel' });
      return wait(300);
    })
    .then(function () {
      check('a press begun with the keyboard on the microphone isn’t drawn into the box', input().value === '' && !input().readOnly && mic().classList.contains('is-on'), input().value);
      var presses = sent('talk').length;
      window.__emit('quick-hidden', null);
      return until(function () { return sent('talk').length > presses; }, 3000, 'that press dropped too');
    })
    .then(function () {
      check('and is dropped as the panel is put away', last('talk').drop === 13, JSON.stringify(last('talk')));
      // Opened to hear a press, then opened for something else: that press coming to nothing
      // leaves the panel as it is now.
      window.__quickOpen({ talk: true }, board);
      id = 14;
      talk({});
      window.__quickOpen({}, board);
      var hides = calls('hide_quick').length;
      talk({ phase: 'nothing', message: 'Nothing heard. Hold the keys down while you talk.' });
      return wait(400).then(function () { return hides; });
    })
    .then(function (hides) {
      check('a press the panel was opened to hear doesn’t put away what it was opened for since', calls('hide_quick').length === hides, String(calls('hide_quick').length - hides));
      done();
    })
    .catch(stopped);
})();
