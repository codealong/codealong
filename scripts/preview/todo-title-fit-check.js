// Checks that a todo's title in its panel (TodoPanel.tsx `TitleField`) shows every line it wraps
// to, as typed and when the field narrows with the words unchanged: the chevron comes in beside it
// as the first worktree or session is linked, so the field fits itself to its width, not only to
// what's typed. Narrowing stands in for the chevron here; it needs frames, so the real clock.
//   PREVIEW_WAIT=30000 PREVIEW_TODOS=scripts/preview/todos-sample.json scripts/preview.sh todo '' scripts/preview/todo-title-fit-check.js
(function () {
  var LONG = 'Cut the board push size in half by sending only what changed since the last push, with the whole board again whenever a device asks for it or a push is refused';
  var row = function (title) {
    return Array.prototype.find.call(document.querySelectorAll('.ca-todorow'), function (r) { return r.querySelector('.ca-todorow-title').textContent.indexOf(title) === 0; });
  };
  var field = function () { return q('.ca-todoinsp-title'); };
  var fits = function (f) { return f.scrollHeight <= f.clientHeight + 1; };
  var says = function (f) { return f.clientHeight + 'px shown of ' + f.scrollHeight + 'px'; };

  say('todo title fit check: waiting for todos');
  until(function () { return row('Cut the board push size'); }, 20000, 'the sample todo with nothing linked')
    .then(function (r) {
      r.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0 }));
      return until(function () { var f = field(); return f && f.value.indexOf('Cut the board push size') === 0 && f; }, 6000, 'its title');
    })
    .then(function (f) {
      f.focus();
      Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set.call(f, LONG);
      f.dispatchEvent(new Event('input', { bubbles: true }));
      return wait(300).then(function () { return f; });
    })
    .then(function (f) {
      check('a long title shows every line as it’s typed', f.value === LONG && fits(f), says(f));
      var before = f.clientHeight;
      q('.ca-todoinsp-tools').style.width = '160px';
      return wait(500).then(function () { return { f: f, before: before }; });
    })
    .then(function (got) {
      check('and every line once the field narrows, the words the same', fits(got.f) && got.f.clientHeight > got.before, says(got.f) + ', ' + got.before + 'px before');
      q('.ca-todoinsp-tools').style.width = '';
      return wait(500).then(function () { return got; });
    })
    .then(function (got) {
      check('and no more than it needs once it widens again', fits(got.f) && got.f.clientHeight === got.before, says(got.f) + ', ' + got.before + 'px at first');
      got.f.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    })
    .then(done)
    .catch(stopped);
})();
