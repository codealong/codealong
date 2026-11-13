# Runs the app `windows-vm.sh build` made on dev's desktop, for `windows-vm.sh run`, or with -Stop
# ends it and the server it started. A program started over SSH runs in no one's desktop (session
# 0), so the app starts from a scheduled task in dev's session, dev being logged on by itself; with
# a home of its own, which the engine reads from HOME before Windows' own (util.rs `home`), so
# nothing of an earlier run's is there.
param([switch]$Stop)
$ErrorActionPreference = 'Stop'
if ($Stop) {
    Get-Process codealong -ErrorAction SilentlyContinue | Stop-Process -Force
    schtasks /delete /tn codealong-run /f 2>$null | Out-Null
    exit 0
}
$exe = Join-Path ([Environment]::GetEnvironmentVariable('CARGO_TARGET_DIR', 'User')) 'release\codealong.exe'
if (-not (Test-Path $exe)) { throw "There's no build to run: scripts/windows-vm.sh build" }
$runHome = "$env:USERPROFILE\run-home"
Remove-Item -Recurse -Force $runHome -ErrorAction SilentlyContinue
New-Item -ItemType Directory $runHome | Out-Null
Set-Content -Encoding ascii "$env:USERPROFILE\run.cmd" "@set HOME=$runHome`r`n@start `"`" `"$exe`""
schtasks /create /tn codealong-run /tr "$env:USERPROFILE\run.cmd" /sc once /st 00:00 /it /f | Out-Null
schtasks /run /tn codealong-run | Out-Null
