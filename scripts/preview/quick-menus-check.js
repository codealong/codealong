// Checks going from one of the quick panel's menus to another (Quick.tsx, Picker.tsx): a press on
// another menu's button closes the open one and opens that one, with the keyboard in its filter,
// even when the click comes before the next frame, when the closed menu hands focus back to the
// prompt; and a press on its own button closes it.
//   PREVIEW_WAIT=60000 scripts/preview.sh quick '' scripts/preview/quick-menus-check.js
(function () {
  localStorage.setItem('codealong.quick.project', 'All projects');
  var input = function () { return q('.ca-quick-input textarea'); };
  var project = function () { return q('.ca-quick-project'); };
  var worktree = function () { return all('.ca-quick-chip').filter(function (b) { return /Worktree \(⌘T\)/.test(b.title); })[0]; };
  var press = function (el) {
    el.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, cancelable: true, button: 0, buttons: 1 }));
    el.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true, button: 0 }));
  };
  var filter = function () { return q('.ca-picker-filter'); };

  say('quick menus check: waiting for the panel');
  until(function () { return !!input() && !!project() && !!worktree() && !worktree().disabled; }, 20000, 'the menus')
    .then(function () {
      project().click();
      return until(function () { return filter() && filter().placeholder === 'Project'; }, 3000, 'the project menu');
    })
    .then(function () {
      // The press and the click in one go, before a frame: the project menu's close asks for the
      // prompt's focus on the next one, after the worktree menu has opened.
      var chip = worktree();
      press(chip);
      chip.click();
      return wait(150);
    })
    .then(function () {
      var open = all('.ca-picker');
      check('one menu is open, the worktree menu', open.length === 1 && !!filter() && filter().placeholder === 'Worktree, branch or repo', open.length + ' open');
      check('with the keyboard in its filter', document.activeElement === filter(), document.activeElement ? document.activeElement.className : 'nothing');
      var before = input().value;
      type(filter(), 'zzqx');
      check('so what’s typed filters it, and the prompt keeps its text', input().value === before && /Nothing matches/.test(q('.ca-picker').textContent), q('.ca-picker').textContent);
      var chip = worktree();
      press(chip);
      chip.click();
      return wait(150);
    })
    .then(function () {
      check('a press on its own button closes it', all('.ca-picker').length === 0, all('.ca-picker').length + ' open');
    })
    .then(done)
    .catch(stopped);
})();
