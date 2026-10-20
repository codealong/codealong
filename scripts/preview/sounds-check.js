// Checks Settings › Sounds (`[sounds]`, sounds.rs): with a config.toml from before there were
// sounds, it shows the defaults (Pop when a session needs you, Tink when a turn ends, none for a
// review), lists the sounds the engine found with None first, and saves a sound picked as
// config.toml has it, playing it as it's picked; a sound's button plays it and None's can't; and
// the volume saves when it's let go of, playing the first sound at it. The sounds listed are put
// in the engine's answer, since the engine the preview runs may be older than the page.
//   scripts/preview.sh sessions '' scripts/preview/sounds-check.js
(function () {
  // What was last saved comes back as config.toml would have it, until then nothing.
  var saved = null;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args) {
    var got = real.apply(this, arguments);
    if (cmd === 'save_config') saved = args.config.sounds;
    if (cmd !== 'get_config') return got;
    return got.then(function (view) {
      if (saved) view.config.sounds = saved;
      else delete view.config.sounds;
      view.sounds = ['Glass', 'Pop', 'Purr', 'Tink'];
      return view;
    });
  };

  var nav = function (label) { return all('.ca-settings-nav button').filter(function (b) { return b.textContent.trim() === label; })[0]; };
  var field = function (label) { return all('.ca-field').filter(function (f) { var s = f.querySelector(':scope > span:not(.ca-checkbox)'); return s && s.textContent === label; })[0]; };
  var select = function (label) { return field(label).querySelector('select'); };
  var play = function (label) { return field(label).querySelector('button'); };
  var last = function (cmd) { var c = calls(cmd); return c.length ? c[c.length - 1][1] : null; };

  (async function () {
    try {
      await until(function () { return !!q('.ca-side-config'); }, 20000, 'no Settings button in the sidebar');
      q('.ca-side-config').click();
      await until(function () { return !!nav('Sounds'); }, 5000, 'no Sounds in Settings');
      nav('Sounds').click();
      await until(function () { return !!field('A session needs you'); }, 5000, 'no sounds in Settings › Sounds');

      check('a session needing you sounds Pop by default', select('A session needs you').value === 'Pop', select('A session needs you').value);
      check('a turn that ended sounds Tink by default', select('A session’s turn ended').value === 'Tink', select('A session’s turn ended').value);
      check('a review asked of you sounds nothing by default', select('A pull request asks for your review').value === '', select('A pull request asks for your review').value);
      var names = all('option', select('A session needs you')).map(function (o) { return o.textContent; });
      check('it lists None, then the sounds found', names.join(',') === 'None,Glass,Pop,Purr,Tink', names.join(','));
      check('None can’t be played', play('A pull request asks for your review').disabled);

      var review = select('A pull request asks for your review');
      review.value = 'Purr';
      review.dispatchEvent(new Event('change', { bubbles: true }));
      await until(function () { return calls('save_config').length > 0; }, 3000, 'picking a sound saved nothing');
      var saved = last('save_config').config.sounds;
      check('a sound picked saves [sounds] whole', JSON.stringify(saved) === JSON.stringify({ needs_you: 'Pop', your_turn: 'Tink', review_requested: 'Purr', volume: 60 }), JSON.stringify(saved));
      var heard = last('play_sound');
      check('and plays it as it’s picked', heard && heard.name === 'Purr' && heard.volume === 60, JSON.stringify(heard));

      play('A session needs you').click();
      await until(function () { return (last('play_sound') || {}).name === 'Pop'; }, 3000, 'its button played nothing');
      check('a sound’s button plays it', true);

      var slider = q('.ca-settings-volume');
      check('the volume starts at 60%', slider.value === '60' && q('.ca-settings-volume-at').textContent === '60%', slider.value);
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(slider, '30');
      slider.dispatchEvent(new Event('input', { bubbles: true }));
      await until(function () { return q('.ca-settings-volume-at').textContent === '30%'; }, 3000, 'moving the volume didn’t show it');
      var saves = calls('save_config').length;
      check('moving it saves nothing yet', saves === 1, String(saves));
      slider.dispatchEvent(new PointerEvent('pointerup', { bubbles: true }));
      await until(function () { return calls('save_config').length > saves; }, 3000, 'letting go of the volume saved nothing');
      check('letting go saves it', last('save_config').config.sounds.volume === 30, JSON.stringify(last('save_config').config.sounds));
      heard = last('play_sound');
      check('and plays the first sound at it', heard && heard.name === 'Pop' && heard.volume === 30, JSON.stringify(heard));
      check('keeping the sound picked before', last('save_config').config.sounds.review_requested === 'Purr', JSON.stringify(last('save_config').config.sounds));
    } catch (e) {
      return stopped(e);
    }
    done();
  })();
})();
