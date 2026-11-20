// The page's clock on another day (PREVIEW_DATE in scripts/preview.sh, which sets
// window.__PREVIEW_DATE__ and loads this before anything else reads the time): `Date` as it is,
// but for "now", which starts at noon on the day asked for and runs on from there.
//
// Noon, and not the time of day it really is: counted from midnight, a day on which the clocks
// go back has 25 hours, and late that evening the page was on the day after the one asked for
// (on November 1, 2026 at 23:30 in Los Angeles, December 29 for December 28), where a check that
// takes what it expects from the same clock passes without the day having been looked at.
// src/checks.test.ts runs this on that evening.
(function () {
  var asked = window.__PREVIEW_DATE__;
  if (!asked) return;
  var Real = Date;
  var part = asked.split('-');
  var shift = new Real(+part[0], part[1] - 1, +part[2], 12).getTime() - Real.now();
  function Shifted(a, b, c, d, e, f, g) {
    var n = arguments.length;
    // Called without `new`, Date gives the time as text.
    if (!(this instanceof Shifted)) return new Real(Real.now() + shift).toString();
    if (n === 0) return new Real(Real.now() + shift);
    if (n === 1) return new Real(a);
    return new Real(a, b, n > 2 ? c : 1, n > 3 ? d : 0, n > 4 ? e : 0, n > 5 ? f : 0, n > 6 ? g : 0);
  }
  Shifted.prototype = Real.prototype;
  Shifted.now = function () { return Real.now() + shift; };
  Shifted.parse = Real.parse;
  Shifted.UTC = Real.UTC;
  window.Date = Shifted;
})();
