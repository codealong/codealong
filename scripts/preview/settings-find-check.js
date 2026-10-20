// Checks Settings' search (SettingsFind.tsx): its field has the keyboard as Settings opens, and
// what's typed turns the pane into every section's settings that match, the fields themselves,
// each section under its name with the words marked; the sections down the left count what
// matches and fade where nothing does; a section's name finds all of it; a setting changed there
// saves and stays; nothing found says so; ⌘F comes back to the field, Escape from anywhere in
// Settings clears it before it closes Settings, a project found by one of its repos keeps its
// form when Edit opens it, and a section's name over its settings opens it.
//   scripts/preview.sh sessions '' scripts/preview/settings-find-check.js
//   PREVIEW_PLATFORM=linux scripts/preview.sh sessions '' scripts/preview/settings-find-check.js
(function () {
  var linux = window.__CODEALONG_PLATFORM__ === 'linux' || /\bLinux\b/.test(navigator.userAgent);
  var field = function () { return q('.ca-settings-find input'); };
  var shown = function (el) { return !el.closest('[data-unmatched]') && el.getClientRects().length > 0; };
  // The labels of the settings that show, by section.
  var labels = function (section) {
    var root = q('.ca-settings-found [data-section="' + section + '"]');
    if (!root) return [];
    return all('.ca-field', root).filter(shown).map(function (f) {
      var s = f.querySelector(':scope > span:not(.ca-checkbox)');
      return s ? s.textContent.trim() : '';
    });
  };
  var nav = function (label) {
    return all('.ca-settings-nav .ca-nav').filter(function (b) { return b.querySelector('.ca-nav-label').textContent === label; })[0];
  };
  var count = function (label) {
    var c = nav(label).querySelector('.ca-nav-count');
    return c ? Number(c.textContent) : 0;
  };
  var marked = function () {
    var h = typeof CSS !== 'undefined' && CSS.highlights ? CSS.highlights.get('ca-settings-find') : null;
    return h ? h.size : -1;
  };
  var search = function (text) {
    type(field(), text);
    return wait(100);
  };

  until(function () { return q('.ca-side-config'); }, 20000, 'the settings button')
    .then(function (gear) {
      gear.click();
      return until(function () { return field() && all('.ca-settings-pane .ca-field').length; }, 20000, 'Settings and its search');
    })
    .then(function () {
      check('the search has the keyboard as Settings opens', document.activeElement === field());
      return search('nudge');
    })
    .then(function () {
      return until(function () { return labels('automation').length; }, 5000, 'what the search finds in Automation');
    })
    .then(function () {
      var auto = labels('automation');
      check('finds the settings whose words match', auto.some(function (l) { return /^Nudge the session that opened a pull request/.test(l); }) && auto.some(function (l) { return /^What a nudge tells it about/.test(l); }), auto.join(' | '));
      check('finds one by what it says under its label', auto.some(function (l) { return /^Resume a stopped session for its pull request/.test(l); }), auto.join(' | '));
      check('and leaves out the rest', !auto.some(function (l) { return /^Review pull requests that wait on your review/.test(l); }), auto.join(' | '));
      check('hides the section intros', !all('.ca-settings-found .ca-settings-lede, .ca-settings-found .ca-settings-intro').some(shown));
      check('hides sections with nothing that matches', !shown(q('.ca-settings-found [data-section="sounds"]')));
      check('counts what each section has that matches', count('Automation') === auto.length, count('Automation') + ' for ' + auto.length);
      check('fades the sections that have none', nav('Sounds').classList.contains('is-unmatched') && !nav('Automation').classList.contains('is-unmatched'));
      check('no section is picked while searching', !q('.ca-settings-nav .ca-nav.is-active'));
      if (marked() < 0) note('no CSS highlights here: the words aren’t marked');
      else check('marks the words', marked() >= auto.length, marked() + ' marks');
      var box = all('.ca-settings-found .ca-field').filter(function (f) { return shown(f) && /^Nudge the session/.test(f.textContent); })[0].querySelector('input[type="checkbox"]');
      var was = box.checked;
      box.click();
      return until(function () { return calls('save_config').length; }, 5000, 'the change saved').then(function () {
        var saved = sent('save_config')[0].config.pull_requests;
        check('a setting changes where the search found it', saved.nudge_by_default === !was, JSON.stringify(saved));
        return wait(300);
      });
    })
    .then(function () {
      check('and stays found', labels('automation').some(function (l) { return /^Nudge the session/.test(l); }));
      return search('sounds');
    })
    .then(function () {
      var sounds = labels('sounds');
      check('a section’s name finds all of it', sounds.length >= 3 && count('Sounds') >= 3, sounds.join(' | '));
      return search('zzqx nothing');
    })
    .then(function () {
      check('says when nothing matches', !!q('.ca-settings-found-none') && /No settings match/.test(q('.ca-settings-found-none').textContent));
      field().blur();
      key(document.body, { key: 'f', metaKey: !linux, ctrlKey: linux });
      check('⌘F gives the search the keyboard', document.activeElement === field());
      return search('nudge');
    })
    .then(function () {
      // Escape from a field the search found, not the search's own.
      var box = all('.ca-settings-found input[type="checkbox"]').filter(shown)[0];
      box.focus();
      var e = new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true });
      box.dispatchEvent(e);
      return wait(200);
    })
    .then(function () {
      check('Escape from a setting it found clears what’s typed', field() && field().value === '' && !q('.ca-settings-found'));
      check('and gives the search the keyboard back', document.activeElement === field());
      check('and leaves Settings open', !!q('.ca-settings'));
      check('back on the section it was on', !!q('.ca-settings-nav .ca-nav.is-active'));
      return fetch('./config.json').then(function (r) { return r.json(); });
    })
    .then(function (view) {
      var project = (view.config.projects || []).filter(function (p) { return p.repos.length; })[0];
      if (!project) return note('no project with repos here: Edit on a project the search found, skipped');
      var word = project.repos[0].split('/').filter(function (w) { return w && w !== '*' && w !== 'github.com'; }).pop();
      var card = function () {
        return all('.ca-settings-found [data-section="projects"] .ca-ucard').filter(function (c) { return shown(c) && (q('.ca-ucard-name', c) || {}).textContent === project.name; })[0];
      };
      return search(word)
        .then(function () { return until(card, 5000, 'the project ' + project.name + ' found by its repo'); })
        .then(function (found) {
          all('button', found).filter(function (b) { return b.textContent.trim() === 'Edit'; })[0].click();
          return wait(300);
        })
        .then(function () {
          var form = all('.ca-settings-found [data-section="projects"] .ca-acct-form').filter(shown)[0];
          check('Edit on a project it found keeps the project’s form in sight', !!form);
          check('with the keyboard in it', !!form && form.contains(document.activeElement));
        });
    })
    .then(function () {
      return search('nudge');
    })
    .then(function () {
      q('.ca-settings-found [data-section="automation"] .ca-settings-found-head').click();
      return wait(200);
    })
    .then(function () {
      check('a section’s name over what it found opens the section', !q('.ca-settings-found') && field().value === '' && nav('Automation').classList.contains('is-active'));
      done();
    })
    .catch(stopped);
})();
