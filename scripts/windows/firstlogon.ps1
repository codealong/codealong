# Runs once, elevated, at dev's first logon (autounattend.xml's FirstLogonCommands), from the medium
# windows-vm.sh made: turns on the OpenSSH server everything else goes through, with the VM's own
# key as the one that lets dev in. Its log is C:\Windows\Temp\firstlogon.log.
Start-Transcript -Path "$env:SystemRoot\Temp\firstlogon.log" -Append
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$ProgressPreference = 'SilentlyContinue'

# A VM that sleeps drops SSH, and one that hibernates writes its 16 GB of memory to its disk.
powercfg /change standby-timeout-ac 0
powercfg /change monitor-timeout-ac 0
powercfg /hibernate off

# Updates are left to be asked for: one that restarts the VM by itself drops the build or the run
# going on in it, and a VM that's out of date is made again rather than kept up.
$au = 'HKLM:\SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate\AU'
New-Item -Path $au -Force | Out-Null
New-ItemProperty -Path $au -Name NoAutoUpdate -PropertyType DWord -Value 1 -Force | Out-Null

# OpenSSH Server: Windows' own optional feature, which comes from Windows Update, else Microsoft's
# MSI that windows-vm.sh put beside this script, for when Windows Update can't be reached.
$cap = Get-WindowsCapability -Online -Name 'OpenSSH.Server*'
if ($cap.State -ne 'Installed') {
    try { Add-WindowsCapability -Online -Name $cap.Name -ErrorAction Stop | Out-Null }
    catch {
        Write-Output "Windows Update didn't give OpenSSH ($_); installing the MSI"
        $msi = Get-ChildItem "$here\OpenSSH-ARM64*.msi" | Select-Object -First 1
        Start-Process msiexec.exe -Wait -ArgumentList "/i `"$($msi.FullName)`" ADDLOCAL=Server /qn /norestart"
    }
}
Set-Service -Name sshd -StartupType Automatic
# Its first start makes C:\ProgramData\ssh: the host keys and sshd_config.
Start-Service sshd

# Commands sent over SSH run in Windows PowerShell rather than cmd.exe.
New-Item -Path 'HKLM:\SOFTWARE\OpenSSH' -Force | Out-Null
New-ItemProperty -Path 'HKLM:\SOFTWARE\OpenSSH' -Name DefaultShell -PropertyType String -Force `
    -Value "$env:SystemRoot\System32\WindowsPowerShell\v1.0\powershell.exe" | Out-Null

# Windows' own OpenSSH makes this rule for private networks alone, and QEMU's network is public
# to Windows, so a rule already there is opened to every profile too.
if (Get-NetFirewallRule -Name 'OpenSSH-Server-In-TCP' -ErrorAction SilentlyContinue) {
    Set-NetFirewallRule -Name 'OpenSSH-Server-In-TCP' -Enabled True -Profile Any
} else {
    New-NetFirewallRule -Name 'OpenSSH-Server-In-TCP' -DisplayName 'OpenSSH Server (sshd)' -Enabled True `
        -Direction Inbound -Protocol TCP -Action Allow -LocalPort 22 -Profile Any | Out-Null
}

# sshd reads an administrator's keys from one file for all of them, and only when Administrators
# and SYSTEM alone may read it (granted by SID, which doesn't depend on Windows' language).
$keys = "$env:ProgramData\ssh\administrators_authorized_keys"
Copy-Item "$here\authorized_keys" $keys -Force
icacls.exe $keys /inheritance:r /grant '*S-1-5-32-544:F' /grant '*S-1-5-18:F'

Restart-Service sshd
Stop-Transcript
