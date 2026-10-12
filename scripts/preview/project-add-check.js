// Checks the project switcher's Add project… (ui/ProjectSwitcher.tsx): it sits above Edit
// projects… at the menu's foot, and opens Settings › Projects with a new project's form up and
// its name field taking the keyboard; Add project there saves config.toml with the new project
// after the others. Edit projects… opens the same section with no form up.
//   PREVIEW_WAIT=30000 scripts/preview.sh sessions '' scripts/preview/project-add-check.js
(function () {
  var item = function (label) { return all('.ca-projswitch-menu .ca-menu-item').filter(function (b) { return b.textContent.trim() === label; })[0]; };
  var button = function (label, within) { return all('button', within).filter(function (b) { return b.textContent.trim() === label; })[0]; };
  var form = function () { return q('.ca-settings-pane form.ca-acct-form'); };
  var openMenu = async function () {
    q('.ca-projswitch-btn').click();
    await until(function () { return q('.ca-projswitch-menu'); }, 3000, 'the project menu didn’t open');
  };
  var type = function (input, text) {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(input, text);
    input.dispatchEvent(new Event('input', { bubbles: true }));
  };

  (async function () {
    try {
      await until(function () { return q('.ca-projswitch-btn'); }, 20000, 'no project switcher');
      await openMenu();
      var labels = all('.ca-projswitch-menu .ca-menu-item').map(function (b) { return b.textContent.trim(); });
      var add = labels.indexOf('Add project…');
      check('the menu has Add project…, just above Edit projects…', add >= 0 && labels[add + 1] === 'Edit projects…', labels.join(' | '));

      item('Add project…').click();
      await until(form, 5000, 'Add project… didn’t open a new project’s form');
      check('which closes the menu', !q('.ca-projswitch-menu'));
      var on = q('.ca-settings-nav .is-active');
      check('in Settings › Projects', !!on && on.textContent.trim() === 'Projects', on ? on.textContent : 'no section picked');
      check('a new project’s form', q('.ca-ucard-name', form()).textContent === 'New project', q('.ca-ucard-name', form()).textContent);
      var name = q('input', form());
      check('its name field has the keyboard', document.activeElement === name, document.activeElement && document.activeElement.outerHTML.slice(0, 80));

      var before = (await window.__TAURI_INTERNALS__.invoke('get_config')).config.projects.map(function (p) { return p.name; });
      type(name, 'Preview check project');
      button('Add project', form()).click();
      await until(function () { return calls('save_config').length > 0; }, 3000, 'Add project saved nothing');
      var saved = calls('save_config')[0][1].config.projects.map(function (p) { return p.name; });
      check('Add project saves it after the others', JSON.stringify(saved) === JSON.stringify(before.concat(['Preview check project'])), saved.join(', '));

      button('Done').click();
      await until(function () { return !q('.ca-settings'); }, 3000, 'Settings didn’t close');
      await openMenu();
      item('Edit projects…').click();
      await until(function () { return q('.ca-settings-pane .ca-settings-intro'); }, 5000, 'Edit projects… didn’t open Settings');
      check('Edit projects… opens Projects with no form up', !form());
      done();
    } catch (e) {
      stopped(e);
    }
  })();
})();
