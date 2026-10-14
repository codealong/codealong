// Checks that what you'd typed in the quick panel outlasts a relaunch (Quick.tsx; quick.ts
// `nextDraft`, `draftCodec`): it's kept in storage as you type, as the next open would leave it,
// and a page that comes up with one kept has it as the draft ↑ brings back, before past prompts.
//
//   scripts/preview.sh quick '' scripts/preview/quick-draft-check.js
(function () {
  // This runs before the app's script, so the page comes up as a relaunched one does, with a
  // draft kept from before.
  var KEY = 'codealong.quick.draft';
  var KEPT = 'Rename the pangolin pen';
  var OLDER = 'An older prompt';
  localStorage.setItem(KEY, JSON.stringify({ text: KEPT, todos: [] }));
  localStorage.setItem('codealong.quick.history', JSON.stringify([OLDER]));

  var input = function () { return q('.ca-quick-input textarea'); };
  var stored = function () {
    var raw = localStorage.getItem(KEY);
    return raw ? JSON.parse(raw).text : null;
  };
  var up = function () { key(input(), { key: 'ArrowUp', code: 'ArrowUp' }); };

  say('quick draft check: waiting for the panel');
  until(function () { return calls('quick_ready').length > 0 && !!input(); }, 30000, 'the quick panel')
    .then(function () { return wait(200); })
    .then(function () {
      check('the panel opens empty', input().value === '', input().value);
      check('it says ↑ brings back the draft', input().placeholder.indexOf('brings back your draft') >= 0, input().placeholder);
      up();
      return wait(100);
    })
    .then(function () {
      check('↑ brings back the draft kept before the relaunch', input().value === KEPT, input().value);
      up();
      return wait(100);
    })
    .then(function () {
      check('then the prompts before it', input().value === OLDER, input().value);
      type(input(), 'Half a thought');
      return wait(100);
    })
    .then(function () {
      check('what you type is kept as you type it', stored() === 'Half a thought', stored());
      type(input(), '');
      return wait(100);
    })
    .then(function () {
      check('emptying the input keeps the draft there was', stored() === KEPT, stored());
      type(input(), 'Half a thought, finished');
      return wait(100);
    })
    .then(function () {
      window.__quickOpen({}, null);
      return wait(100);
    })
    .then(function () {
      check('the next open starts empty', input().value === '', input().value);
      check('with what was typed kept as the draft', stored() === 'Half a thought, finished', stored());
      up();
      return wait(100);
    })
    .then(function () {
      check('↑ brings it back', input().value === 'Half a thought, finished', input().value);
      done();
    })
    .catch(stopped);
})();
