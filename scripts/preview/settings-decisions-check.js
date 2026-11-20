// Checks Settings › Decisions (decisions.ts, config.rs `DecisionsCfg`): Jev is listed without an
// entry of its own and is the default; Add model offers each provider, starting on OpenAI's model,
// and saves it as a `[[decisions.models]]` entry, Jev still without one; a key field shows for
// OpenAI once a model of its is there; a project's Edit form offers the default, each model and
// none, saves its choice as the project's `decisions`, and the model's card names the project;
// removing the model takes it off the project too. Made-up projects, over whatever this Mac has.
//   scripts/preview.sh sessions '' scripts/preview/settings-decisions-check.js
(function () {
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'decision_keys') return Promise.resolve({ typesafe: true });
    return real(cmd, args, options);
  };
  var button = function (label, within) { return all('button').filter(function (b) { return b.textContent.trim() === label && (!within || within.contains(b)); })[0]; };
  var card = function (name) { return all('.ca-settings-pane .ca-ucard').filter(function (c) { var n = c.querySelector('.ca-ucard-name'); return n && n.textContent.trim() === name; })[0]; };
  var field = function (label, within) { return all('.ca-field', within || q('.ca-settings-pane')).filter(function (f) { var s = f.querySelector(':scope > span'); return s && s.textContent === label; })[0]; };
  var pick = function (select, value) {
    Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value').set.call(select, value);
    select.dispatchEvent(new Event('change', { bubbles: true }));
  };
  var saved = function () { var s = sent('save_config'); return s.length ? s[s.length - 1].config : null; };
  var saves = function () { return sent('save_config').length; };

  var run = async function () {
    var view = await (await fetch('./config.json')).json();
    var cfg = view.config;
    cfg.decisions = { default: 'jev', quick_panel: true, sessions: false, todos: false, pull_requests: false, prompts: false };
    cfg.projects = [
      { name: 'Atlas', repos: ['github.com/atlas/*'], folders: [] },
      { name: 'Northwind', repos: ['github.com/northwind/*'], folders: [] },
    ];
    window.__keepConfig = true;
    window.__keptConfig = cfg;
    (await until(function () { return q('.ca-side-config'); }, 20000, 'the settings button')).click();
    (await until(function () { return button('Decisions'); }, 20000, 'Settings › Decisions')).click();
    var jev = await until(function () { return card('jev'); }, 10000, 'Jev’s card');
    check('Jev is listed, on TypeSafe, as the default', /TypeSafe · jev-preview/.test(jev.textContent) && /Default/.test(jev.textContent), jev.textContent);
    check('Jev can’t be removed', !button('Remove', jev));
    check('only TypeSafe’s key is asked for with only Jev', !!field('TypeSafe API key') && !field('OpenAI API key'));

    button('Add model').click();
    var form = await until(function () { return q('.ca-settings-pane form.ca-acct-form'); }, 5000, 'the new model’s form');
    var providers = all('.ca-segctl button', form).map(function (b) { return b.textContent.trim(); });
    check('each provider is offered', providers.join(',') === 'TypeSafe,OpenAI,Gemini', providers.join(','));
    var inputs = all('input', form);
    check('it starts on OpenAI’s model, named for it', inputs[0].value === 'luna' && inputs[1].value === 'gpt-6-luna', inputs.map(function (i) { return i.value; }).join(','));
    var before = saves();
    button('Add model', form).click();
    await until(function () { return saves() > before; }, 5000, 'the save');
    var models = saved().decisions.models || [];
    check('saves it as an entry, Jev still without one', JSON.stringify(models) === JSON.stringify([{ name: 'luna', provider: 'openai', model: 'gpt-6-luna' }]), JSON.stringify(models));
    await until(function () { return card('luna'); }, 5000, 'luna’s card');
    check('a key field shows for OpenAI now', !!field('OpenAI API key'));
    check('luna has no key yet', /No key/.test(card('luna').textContent), card('luna').textContent);

    // A project's choice.
    button('Projects').click();
    var atlas = await until(function () { return card('Atlas'); }, 10000, 'Atlas’s card');
    button('Edit', atlas).click();
    var f = await until(function () { return field('Decision model'); }, 5000, 'the Decision model field');
    var select = f.querySelector('select');
    var options = Array.prototype.map.call(select.options, function (o) { return o.value; });
    check('offers the default, each model and none', options.join(',') === ',jev,luna,none', options.join(','));
    check('goes by the default until picked', select.value === '', select.value);
    pick(select, 'luna');
    before = saves();
    button('Save', f.closest('form')).click();
    await until(function () { return saves() > before; }, 5000, 'the project’s save');
    var p = saved().projects.filter(function (x) { return x.name === 'Atlas'; })[0];
    check('saves it as the project’s decisions', p && p.decisions === 'luna', JSON.stringify(p));
    var other = saved().projects.filter(function (x) { return x.name === 'Northwind'; })[0];
    check('another project still goes by the default', other && !other.decisions, JSON.stringify(other));

    button('Decisions').click();
    var luna = await until(function () { return card('luna') && /Atlas/.test(card('luna').textContent) && card('luna'); }, 5000, 'luna’s card naming Atlas');
    check('the model’s card names the project that chose it', /Projects\s*Atlas/.test(luna.textContent), luna.textContent);
    button('Remove', luna).click();
    var confirm = await until(function () { return q('.ca-acct-confirm', card('luna')); }, 5000, 'the confirmation');
    before = saves();
    button('Remove', confirm).click();
    await until(function () { return saves() > before; }, 5000, 'the removal’s save');
    var after = saved();
    check('removing it takes it off the project', !(after.decisions.models || []).length && !after.projects.filter(function (x) { return x.name === 'Atlas'; })[0].decisions, JSON.stringify(after.projects));
    done();
  };
  run().catch(stopped);
})();
