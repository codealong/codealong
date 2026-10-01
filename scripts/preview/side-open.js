// A side conversation to look at: PREVIEW_INIT for a view that shows a session's reply box (the
// peek, a session's panel), with one answered question and a second being answered.
//   PREVIEW_INIT=scripts/preview/side-open.js PREVIEW_NAME=peek-side scripts/preview.sh peek
(function () {
  var tick = setInterval(function () {
    var asked = (window.__calls || []).filter(function (c) { return c[0] === 'side_get'; }).pop();
    if (!asked || !window.__emit) return;
    clearInterval(tick);
    window.__emit('side', {
      key: asked[1].key,
      running: true,
      ended: false,
      turns: [
        { question: 'Why does it queue sends instead of typing them at once?', answer: 'Two reasons, both in `send.rs`:\n\n- Messages to one session have to **arrive in the order you sent them**, and a paste can take a second to land.\n- A key and a paste that arrive in one read are taken as one paste, so each needs a moment to itself.', done: true },
        { question: 'Is that covered by a test?', answer: 'Partly.', tool: 'Bash · `grep -n "in_order" src-tauri/src/send.rs`', done: false },
      ],
    });
  }, 50);
})();
