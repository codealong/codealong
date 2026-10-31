// Checks what a usage chip says when the usage endpoint turned its read away: with no numbers to
// show it says so rather than that usage is unavailable, why is on hover, and with numbers kept
// from before it shows them dimmed. A read that failed some other way keeps its own words.
//   PREVIEW_USAGE=scripts/preview/usage-rate-limited.json scripts/preview.sh sessions '' scripts/preview/usage-rate-limited-check.js
(function () {
  var chip = function (name) {
    return all('.ca-uchip').find(function (c) { return q('.ca-uchip-name', c).textContent === name; });
  };

  until(function () { return chip('antigravity'); }, 20000, 'the usage bar')
    .then(function () {
      var limited = q('.ca-uchip-note', chip('claude'));
      check('a chip with no numbers says it was rate limited', limited && limited.textContent === 'Rate limited', limited && limited.textContent);
      check('and says when it tries again on hover', limited && /trying again in a few minutes/.test(limited.title), limited && limited.title);

      var kept = chip('claude-work');
      check('a chip with numbers kept shows them', !q('.ca-uchip-note', kept) && /16%/.test(kept.textContent), kept.textContent);
      check('dimmed, with why on hover', kept.classList.contains('is-stale') && /rate limited/.test(kept.title), kept.title);

      var other = q('.ca-uchip-note', chip('antigravity'));
      check('a read that failed some other way still says usage is unavailable', other && other.textContent === 'Usage unavailable', other && other.textContent);
    })
    .then(done)
    .catch(stopped);
})();
