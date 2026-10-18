# Security

Please report a security problem privately: on GitHub, open the repository's **Security** tab
and choose **Report a vulnerability**. Don't open an issue or a pull request for it.

Say what's affected, how to reproduce it, and what it lets someone do. You'll hear back within a
few days, and the fix ships as a release of its own, with credit to you unless you'd rather not.

Areas where a problem matters most: linked devices and remote control (what another device may
ask a Mac to do), the `codealong` command and the app's socket, the Claude Code plugin's hooks,
and the update (`latest.json` and the signed archive it points at).
