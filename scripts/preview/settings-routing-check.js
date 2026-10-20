// Checks Settings › Routing's groups and buffer (`[routing] buffer`): Room kept for running
// sessions starts at 5% and saves the percent picked. A new group's accounts are added from the
// agent menu, each an account on a model (its own line said as Auto's) at an effort set in the
// menu's effort row (← → step it); on Fallback the words over them say they're tried in that order
// and how to reorder them, and no row has a grip or a number. One dragged by its row (useSort.ts)
// goes where it's let go, ⌃⌘↑ ⌃⌘↓ on its buttons move it a place, and on another strategy they
// drag the same. The group saves with its members in that order, each
// with its model and effort, its card listing them with arrows. Use this group for says it's the
// same setting as Auto routing and Automation's reviewers.
//   scripts/preview.sh sessions '' scripts/preview/settings-routing-check.js
(function () {
  var button = function (label, within) { return all('button', within).filter(function (b) { return b.textContent.trim() === label; })[0]; };
  var nav = function (label) { return all('.ca-settings-nav button').filter(function (b) { return b.textContent.trim() === label; })[0]; };
  var field = function (words, within) { return all('.ca-field', within || q('.ca-settings-pane')).filter(function (f) { var s = f.querySelector(':scope > span:not(.ca-checkbox)'); return s && words.test(s.textContent); })[0]; };
  var pick = function (select, value) {
    Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value').set.call(select, value);
    select.dispatchEvent(new Event('change', { bubbles: true }));
  };
  var rows = function (form) { return all('.ca-routing-member', form).filter(function (r) { return q('.ca-routing-member-start', r); }); };
  var chip = function (row) { return q('.ca-modelmenu-chip', row); };
  var adder = function (form) { return all('.ca-modelmenu-chip', form).filter(function (b) { return /Add account/.test(b.textContent); })[0]; };
  var pointer = function (el, type, x, y) {
    el.dispatchEvent(new PointerEvent(type, { bubbles: true, cancelable: true, button: type === 'pointermove' ? -1 : 0, buttons: type === 'pointerup' ? 0 : 1, clientX: x, clientY: y, pointerType: 'mouse', isPrimary: true }));
  };
  var middle = function (el) {
    var r = el.getBoundingClientRect();
    return { x: (r.left + r.right) / 2, y: (r.top + r.bottom) / 2 };
  };
  var lastSave = function () { var c = calls('save_config'); return c.length ? c[c.length - 1][1].config : null; };
  // The menu's lines that start somewhere, as account → [own line, a model's line].
  var lines = function () {
    var out = {};
    all('.ca-picker-opt').filter(function (o) { return !o.disabled; }).forEach(function (o) {
      var head = o.closest('.ca-picker-section').querySelector('.ca-quick-acct-name');
      if (!head) return;
      (out[head.textContent] = out[head.textContent] || []).push(o);
    });
    return out;
  };

  (async function () {
    try {
      (await until(function () { return q('.ca-side-config'); }, 20000, 'the settings button')).click();
      (await until(function () { return nav('Routing'); }, 20000, 'Routing in Settings')).click();
      var buffer = await until(function () { return field(/^Room kept for running sessions/); }, 20000, 'Room kept for running sessions');
      var select = q('select', buffer);
      check('the buffer is 5% to begin with', select.value === '5', select.value);
      pick(select, '10');
      await until(function () { return calls('save_config').length; }, 5000, 'the buffer saved');
      check('a percent picked saves to [routing] buffer', lastSave().routing.buffer === 10, JSON.stringify(lastSave().routing));

      button('Add routing group').click();
      var form = await until(function () { return q('.ca-acct-form'); }, 5000, 'the new group form');
      type(q('input', form), 'overflow');
      button('Fallback', form).click();
      await until(function () { return /first account in the order below/.test(form.textContent); }, 5000, 'what Fallback does');
      check('with none added, it says every account is in it', /None added: every enabled account/.test(form.textContent), field(/^Accounts in this group/, form).textContent);

      adder(form).click();
      var offered = await until(function () { var l = lines(); return Object.keys(l).length >= 2 ? l : null; }, 5000, 'two accounts in the menu');
      var names = Object.keys(offered);
      // The accounts in the order the rows list them, each by the longest name its chip starts with.
      var order = function () {
        return rows(form).map(function (r) {
          var t = chip(r).textContent;
          return names.filter(function (n) { return t.indexOf(n) === 0; }).sort(function (a, b) { return b.length - a.length; })[0];
        });
      };
      check('an account’s own line is Auto’s model', offered[names[0]][0].textContent.indexOf('Auto') === 0, offered[names[0]][0].textContent);
      offered[names[0]][0].click();
      await until(function () { return rows(form).length === 1; }, 5000, 'the first account added');
      adder(form).click();
      var model = await until(function () { var l = lines()[names[1]] || []; return l[1] || l[0]; }, 5000, 'the second account’s lines');
      var modelWords = model.querySelector('.ca-picker-title').textContent;
      model.click();
      await until(function () { return rows(form).length === 2; }, 5000, 'the second account added');
      var said = q('small', field(/^Accounts in this group/, form));
      check('accounts added are listed in the order they’re tried, the words over them saying how to reorder', order().join() === [names[0], names[1]].join() && /^Tried in this order\. Drag or/.test(said.textContent), said.textContent + ' ' + order().join());
      check('a row is its agent menu and its ✕: no grip, number or arrows', rows(form).every(function (r) { return q('.ca-routing-member-start', r).children.length === 1; }) && !q('button[aria-label$="sooner"], button[aria-label$="later"]', form), rows(form).map(function (r) { return r.innerHTML; }).join(' | '));

      // An effort for the first, from its menu's effort row, where its account takes one.
      chip(rows(form)[0]).click();
      var level = await until(function () { return q('.ca-picker-foot') ? (all('.ca-picker-foot button').filter(function (b) { return b.textContent.trim() === 'High'; })[0] || 'none') : null; }, 5000, 'the menu');
      var effort = level === 'none' ? '' : 'xhigh';
      if (level !== 'none') {
        level.click();
        await until(function () { return /High effort/.test(chip(rows(form)[0]).textContent); }, 5000, 'the effort on its chip');
        // → steps it, as the row's keys say.
        key(q('.ca-picker-filter'), { key: 'ArrowRight' });
        await until(function () { return /Extra/.test(chip(rows(form)[0]).textContent); }, 5000, 'the effort a step up');
      }
      // The menu opens inside the member's row: a drag or ⌃⌘↓ in it is the menu's, not the row's.
      var filter = q('.ca-picker-filter', rows(form)[0]);
      var inside = middle(filter);
      pointer(filter, 'pointerdown', inside.x, inside.y);
      pointer(filter, 'pointermove', inside.x, inside.y + 40);
      await wait(50);
      var sortedFromMenu = !!q('.ca-routing-members.is-sorting', form);
      pointer(filter, 'pointerup', inside.x, inside.y + 40);
      key(filter, { key: 'ArrowDown', ctrlKey: true, metaKey: true });
      await wait(50);
      check('a drag or ⌃⌘↓ in a member’s open menu leaves the order alone', !sortedFromMenu && order().join() === [names[0], names[1]].join(), sortedFromMenu + ' ' + order().join());
      chip(rows(form)[0]).click();
      var use = field(/^Use this group for/, form);
      check('Use this group for says it’s Auto routing’s and Automation’s setting', /same settings as Auto routing above and the reviewers in Automation/.test(use.textContent), use.textContent);

      // The first dragged down past the second's middle: it follows the pointer, the second makes
      // way, and letting go puts it there and opens nothing.
      var held = rows(form)[0];
      var from = middle(held);
      var past = middle(rows(form)[1]).y + 4;
      pointer(held, 'pointerdown', from.x, from.y);
      pointer(held, 'pointermove', from.x, from.y + 10);
      pointer(held, 'pointermove', from.x, past);
      await until(function () { return q('.ca-routing-members.is-sorting', form) && /translateY\(-/.test(rows(form)[1].style.transform); }, 5000, 'the second making way');
      check('the one dragged is lifted and follows the pointer', held.classList.contains('is-lifted') && /translateY\(\d/.test(held.style.transform), held.className + ' ' + held.style.transform);
      pointer(held, 'pointerup', from.x, past);
      chip(held).click();
      await until(function () { return order().join() === [names[1], names[0]].join(); }, 5000, 'the order changed');
      await wait(50);
      check('letting go puts it there, the rows standing still and no menu open', !q('.ca-routing-members.is-sorting', form) && rows(form).every(function (r) { return !r.style.transform; }) && !q('.ca-picker'), rows(form).map(function (r) { return r.style.transform; }).join('|'));

      // ⌃⌘↑ ⌃⌘↓ on a member's buttons move it too, no further than the ends.
      key(chip(rows(form)[1]), { key: 'ArrowUp', ctrlKey: true, metaKey: true });
      await until(function () { return order().join() === [names[0], names[1]].join(); }, 5000, '⌃⌘↑ moving the second up');
      key(chip(rows(form)[0]), { key: 'ArrowUp', ctrlKey: true, metaKey: true });
      await wait(50);
      check('⌃⌘↑ moves one up, and not past the first place', order().join() === [names[0], names[1]].join(), order().join());
      key(chip(rows(form)[0]), { key: 'ArrowDown', ctrlKey: true, metaKey: true });
      await until(function () { return order().join() === [names[1], names[0]].join(); }, 5000, '⌃⌘↓ moving the first down');

      // On another strategy they drag the same.
      button('Round robin', form).click();
      await until(function () { return /least-recently-used first/.test(form.textContent); }, 5000, 'what Round robin does');
      held = rows(form)[0];
      from = middle(held);
      past = middle(rows(form)[1]).y + 4;
      pointer(held, 'pointerdown', from.x, from.y);
      pointer(held, 'pointermove', from.x, from.y + 10);
      pointer(held, 'pointermove', from.x, past);
      pointer(held, 'pointerup', from.x, past);
      chip(held).click();
      await until(function () { return order().join() === [names[0], names[1]].join(); }, 5000, 'a drag on Round robin');
      check('on Round robin a member drags to another place too', !q('.ca-picker'), order().join());
      key(chip(rows(form)[0]), { key: 'ArrowDown', ctrlKey: true, metaKey: true });
      await until(function () { return order().join() === [names[1], names[0]].join(); }, 5000, '⌃⌘↓ on Round robin');
      button('Fallback', form).click();
      await until(function () { return /first account in the order below/.test(form.textContent); }, 5000, 'Fallback again');
      var saves = calls('save_config').length;
      button('Add group', form).click();
      await until(function () { return calls('save_config').length > saves; }, 5000, 'the group saved');
      var group = (lastSave().routing.groups || []).filter(function (g) { return g.name === 'overflow'; })[0];
      check('the group saves as fallback', group && group.strategy === 'fallback', JSON.stringify(group));
      var m = group ? group.members : [];
      check('its members save in the order set, with their models and efforts', m.length === 2 && m[0].account === names[1] && m[1].account === names[0] && m[1].model === '' && m[1].effort === effort && (m[0].model !== '' || /^Auto/.test(modelWords)), JSON.stringify(m) + ' picked ' + modelWords);
      check('its card lists them in order', all('.ca-ucard-plan').some(function (p) { return /^Fallback · /.test(p.textContent) && p.textContent.indexOf(' → ') > 0; }), all('.ca-ucard-plan').map(function (p) { return p.textContent; }).join(' | '));
      done();
    } catch (e) {
      stopped(e);
    }
  })();
})();
