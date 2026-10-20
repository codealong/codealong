// Checks Settings › Routing's groups and buffer (`[routing] buffer`): Room kept for running
// sessions starts at 5% and saves the percent picked. A new group's accounts are added from the
// agent menu, each an account on a model (its own line said as Auto's) at an effort set in the
// menu's effort row (← → step it); on Fallback they're numbered in the order they're tried, Later moves one
// down, and the group saves with its members in that order, each with its model and effort, its
// card listing them with arrows. Use this group for says it's the same setting as Auto routing
// and Automation's reviewers.
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
      check('an account’s own line is Auto’s model', offered[names[0]][0].textContent.indexOf('Auto') === 0, offered[names[0]][0].textContent);
      offered[names[0]][0].click();
      await until(function () { return rows(form).length === 1; }, 5000, 'the first account added');
      adder(form).click();
      var model = await until(function () { var l = lines()[names[1]] || []; return l[1] || l[0]; }, 5000, 'the second account’s lines');
      var modelWords = model.querySelector('.ca-picker-title').textContent;
      model.click();
      await until(function () { return rows(form).length === 2; }, 5000, 'the second account added');
      check('accounts added are numbered in the order they’re tried', /^1\./.test(rows(form)[0].textContent) && /^2\./.test(rows(form)[1].textContent), rows(form).map(function (r) { return r.textContent; }).join(' | '));
      check('the first can’t go sooner, the last can’t go later', q('button[aria-label$="sooner"]', rows(form)[0]).disabled && q('button[aria-label$="later"]', rows(form)[1]).disabled);

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
      chip(rows(form)[0]).click();
      var use = field(/^Use this group for/, form);
      check('Use this group for says it’s Auto routing’s and Automation’s setting', /same settings as Auto routing above and the reviewers in Automation/.test(use.textContent), use.textContent);

      q('button[aria-label$="later"]', rows(form)[0]).click();
      await until(function () { return chip(rows(form)[0]).textContent.indexOf(names[1]) === 0; }, 5000, 'the order changed');
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
