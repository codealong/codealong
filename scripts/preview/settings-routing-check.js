// Checks Settings › Routing's fallback groups and buffer (`[routing] buffer`): Room kept for
// running sessions starts at 5% and saves the percent picked; a new group on Fallback numbers its
// accounts in the order they're checked, Later moves one down, and the group saves with its
// members in that order, its card listing them with arrows. The group's Use this group for says
// it's the same setting as Auto routing and Automation's reviewers.
//   scripts/preview.sh sessions '' scripts/preview/settings-routing-check.js
(function () {
  var button = function (label, within) { return all('button', within).filter(function (b) { return b.textContent.trim() === label; })[0]; };
  var nav = function (label) { return all('.ca-settings-nav button').filter(function (b) { return b.textContent.trim() === label; })[0]; };
  var field = function (words, within) { return all('.ca-field', within || q('.ca-settings-pane')).filter(function (f) { var s = f.querySelector(':scope > span'); return s && words.test(s.textContent); })[0]; };
  var pick = function (select, value) {
    Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value').set.call(select, value);
    select.dispatchEvent(new Event('change', { bubbles: true }));
  };
  var rows = function (form) { return all('.ca-routing-member', form); };
  var named = function (row) { return row.querySelector('.ca-check > span:last-child').textContent.trim(); };
  var lastSave = function () { var c = calls('save_config'); return c.length ? c[c.length - 1][1].config : null; };

  (async function () {
    try {
      (await until(function () { return q('.ca-side-config'); }, 20000, 'the settings button')).click();
      (await until(function () { return nav('Routing'); }, 20000, 'Routing in Settings')).click();
      var buffer = await until(function () { return field(/^Room kept for running sessions/); }, 20000, 'Room kept for running sessions');
      var select = q('select', buffer);
      check('the buffer is 5% to begin with', select.value === '5', select.value);
      check('it says what it is for', /fallback group moves on/.test(buffer.textContent), buffer.textContent);
      pick(select, '10');
      await until(function () { return calls('save_config').length; }, 5000, 'the buffer saved');
      check('a percent picked saves to [routing] buffer', lastSave().routing.buffer === 10, JSON.stringify(lastSave().routing));

      button('Add routing group').click();
      var form = await until(function () { return q('.ca-acct-form'); }, 5000, 'the new group form');
      type(q('input', form), 'overflow');
      button('Fallback', form).click();
      await until(function () { return /first account in the order below/.test(form.textContent); }, 5000, 'what Fallback does');
      if (rows(form).length < 2) throw new Error('fewer than two accounts to order: ' + rows(form).length);
      var first = named(rows(form)[0]);
      var second = named(rows(form)[1]);
      q('input[type=checkbox]', rows(form)[0]).click();
      await until(function () { return /^1\. /.test(named(rows(form)[0])); }, 5000, 'the first account numbered');
      q('input[type=checkbox]', rows(form)[1]).click();
      await until(function () { return /^2\. /.test(named(rows(form)[1])); }, 5000, 'the second account numbered');
      check('checked accounts are numbered in the order they’re tried', true, rows(form).map(named).join(' | '));
      check('the first can’t go sooner, the last can’t go later', q('button[aria-label$="sooner"]', rows(form)[0]).disabled && q('button[aria-label$="later"]', rows(form)[1]).disabled);
      var use = field(/^Use this group for/, form);
      check('Use this group for says it’s Auto routing’s and Automation’s setting', /same settings as Auto routing above and the reviewers in Automation/.test(use.textContent), use.textContent);

      q('button[aria-label$="later"]', rows(form)[0]).click();
      await until(function () { return named(rows(form)[0]) === '1. ' + second; }, 5000, 'the order changed');
      var saves = calls('save_config').length;
      button('Add group', form).click();
      await until(function () { return calls('save_config').length > saves; }, 5000, 'the group saved');
      var group = (lastSave().routing.groups || []).filter(function (g) { return g.name === 'overflow'; })[0];
      check('the group saves as fallback', group && group.strategy === 'fallback', JSON.stringify(group));
      var order = group ? group.members.map(function (m) { return m.account; }) : [];
      check('its members save in the order set', order.length === 2 && second.indexOf(order[0] + ' (') === 0 && first.indexOf(order[1] + ' (') === 0, JSON.stringify(order) + ' from ' + first + ', ' + second);
      check('its card lists them in order', all('.ca-ucard-plan').some(function (p) { return /^Fallback · /.test(p.textContent) && p.textContent.indexOf(order.join(' → ')) > 0; }), all('.ca-ucard-plan').map(function (p) { return p.textContent; }).join(' | '));
      done();
    } catch (e) {
      stopped(e);
    }
  })();
})();
