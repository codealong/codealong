// A side conversation to look at: PREVIEW_INIT for a view that shows a session's reply box (the
// peek, a session's panel), with one answered question and a second being answered, and two kept
// from before (Earlier lists them).
//   PREVIEW_INIT=scripts/preview/side-open.js PREVIEW_NAME=peek-side scripts/preview.sh peek
// With `window.__sideList = true` in front of it, it shows the list of the kept ones instead, as
// `/side` alone does:
//   (echo 'window.__sideList = true;'; cat scripts/preview/side-open.js) > "$TMPDIR/side-list.js"
//   PREVIEW_INIT="$TMPDIR/side-list.js" PREVIEW_NAME=peek-side-list scripts/preview.sh peek
(function () {
  var tick = setInterval(function () {
    var asked = (window.__calls || []).filter(function (c) { return c[0] === 'side_get'; }).pop();
    if (!asked || !window.__emit) return;
    clearInterval(tick);
    var key = asked[1].key;
    var hour = 3600000;
    window.__past = [
      { id: 'kept-1', key: key, title: '', startedAt: Date.now() - 3 * hour, lastAt: Date.now() - 3 * hour, turns: [{ question: 'Which test covers the retry?', answer: '`retries_a_paste_that_didnt_land` in `send.rs`.', tools: ['Grep · `retry`'], done: true }] },
      { id: 'kept-2', key: key, title: '', startedAt: Date.now() - 26 * hour, lastAt: Date.now() - 25 * hour, turns: [{ question: 'What did we decide about the socket path?', answer: 'To keep it.', done: true }, { question: 'And the lock file?', answer: 'Gone.', done: true }] },
    ];
    if (window.__sideList) {
      var box = document.querySelector('.ca-composer textarea');
      Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set.call(box, '/side');
      box.dispatchEvent(new Event('input', { bubbles: true }));
      setTimeout(function () { box.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true })); }, 50);
      return;
    }
    window.__emit('side', {
      id: 'open-1',
      key: key,
      running: true,
      ended: false,
      turns: [
        { question: 'Why does it queue sends instead of typing them at once?', answer: 'Two reasons, both in `send.rs`:\n\n- Messages to one session have to **arrive in the order you sent them**, and a paste can take a second to land.\n- A key and a paste that arrive in one read are taken as one paste, so each needs a moment to itself.', tools: ['Read · `src-tauri/src/send.rs`', 'Bash · `git log --oneline -5 -- src-tauri/src/send.rs`'], done: true },
        { question: 'Is that covered by a test?', answer: 'Partly.', tools: ['Bash · `grep -n "in_order" src-tauri/src/send.rs`'], tool: 'Bash · `grep -n "in_order" src-tauri/src/send.rs`', done: false },
      ],
    });
  }, 50);
})();
