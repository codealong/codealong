// Opens Settings › Accounts at the first account with MCP servers (PREVIEW_MCP), for a screenshot
// (preview.sh's sessions view).
//   PREVIEW_MCP=scripts/preview/mcp-sample.json PREVIEW_INIT=scripts/preview/settings-accounts-init.js PREVIEW_NAME=settings-accounts scripts/preview.sh sessions
//   PREVIEW_SCHEME=dark PREVIEW_MCP=scripts/preview/mcp-sample.json PREVIEW_INIT=scripts/preview/settings-accounts-init.js PREVIEW_NAME=settings-accounts scripts/preview.sh sessions
(function () {
  var button = function (label, within) { return Array.prototype.filter.call((within || document).querySelectorAll('button'), function (b) { return b.textContent.trim() === label; })[0]; };
  var step = 0;
  var tries = 0;
  var go = setInterval(function () {
    tries += 1;
    var nav = document.querySelector('.ca-settings-nav');
    var row = document.querySelector('.ca-acct-mcp');
    if (step === 0 && document.querySelector('.ca-side-config')) {
      document.querySelector('.ca-side-config').click();
      step = 1;
    } else if (step === 1 && nav && button('Accounts', nav)) {
      button('Accounts', nav).click();
      step = 2;
    } else if (step === 2 && row) {
      row.closest('.ca-acct').scrollIntoView({ block: 'start' });
      clearInterval(go);
    }
    if (tries > 300) clearInterval(go);
  }, 50);
})();
