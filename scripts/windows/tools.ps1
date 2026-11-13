# Installs what building and testing Codealong on Windows takes, for windows-vm.sh, which runs it
# over SSH as dev (an administrator, elevated there): Visual Studio's C++ build tools for Arm64 with
# its clang (ring, which TLS needs, compiles its C for aarch64-pc-windows-msvc with clang only),
# Rust, Node 24, Git for Windows (Claude Code on Windows runs its commands in Git's bash.exe, and
# Codealong looks for it), Zig (which builds Ghostty's terminal library from its source), and
# Claude Code and Codex, signed in to nothing. Each is skipped when it's there already, so running
# this again finishes what an earlier run didn't. It downloads each installer itself rather than
# asking winget, which a fresh Windows registers only for an interactive logon, never over SSH.
#
# What's put on PATH is put on the machine's PATH (rustup and npm keep to dev's own), which every
# new SSH session reads. The WebView2 runtime Tauri needs comes with Windows 11.
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
$downloads = "$env:USERPROFILE\Downloads"
New-Item -ItemType Directory -Force $downloads | Out-Null

function Refresh-Path {
    $env:Path = [Environment]::GetEnvironmentVariable('Path', 'Machine') + ';' +
        [Environment]::GetEnvironmentVariable('Path', 'User')
}

function Add-MachinePath($dir) {
    $path = [Environment]::GetEnvironmentVariable('Path', 'Machine')
    if (($path -split ';') -notcontains $dir) {
        [Environment]::SetEnvironmentVariable('Path', "$path;$dir", 'Machine')
    }
    Refresh-Path
}

function Get-File($url, $name) {
    $file = Join-Path $downloads $name
    if (-not (Test-Path $file)) {
        Write-Output "  downloading $url"
        # curl.exe (in Windows since 2018) rather than Invoke-WebRequest, which is many times
        # slower for files of gigabytes in Windows PowerShell.
        curl.exe -fsSL --retry 5 -o "$file.part" $url
        if ($LASTEXITCODE) { throw "couldn't download $url" }
        Move-Item "$file.part" $file
    }
    $file
}

function Run($file, $arguments, $ok = @(0)) {
    $p = Start-Process -FilePath $file -ArgumentList $arguments -Wait -PassThru -NoNewWindow
    if ($ok -notcontains $p.ExitCode) { throw "$file exited with $($p.ExitCode)" }
}

function Step($name) { Write-Output "== $name ($(Get-Date -Format HH:mm:ss))" }

# Every copy of the checkout and the one folder cargo builds them all into, which Defender would
# otherwise scan file by file as they're written, several times slowing each build.
Step 'Defender exclusions'
$checkouts = "$env:USERPROFILE\checkouts"
$target = "$env:USERPROFILE\codealong-target"
New-Item -ItemType Directory -Force $checkouts, $target | Out-Null
Add-MpPreference -ExclusionPath $checkouts, $target, "$env:USERPROFILE\.cargo", "$env:USERPROFILE\.rustup",
    "$env:APPDATA\npm-cache", "$env:LOCALAPPDATA\npm-cache"
# Where cargo builds every copy windows-vm.sh makes, one folder for all of them as on the Mac.
[Environment]::SetEnvironmentVariable('CARGO_TARGET_DIR', $target, 'User')

Step 'Visual Studio Build Tools'
$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
$components = 'Microsoft.VisualStudio.Workload.VCTools', 'Microsoft.VisualStudio.Component.VC.Tools.ARM64',
    'Microsoft.VisualStudio.Component.Windows11SDK.26100', 'Microsoft.VisualStudio.Component.VC.Llvm.Clang'
function Find-VS {
    if (Test-Path $vswhere) {
        & $vswhere -products * -requires $components -property installationPath | Select-Object -First 1
    }
}
$vs = Find-VS
if (-not $vs) {
    $installer = Get-File 'https://aka.ms/vs/17/release/vs_buildtools.exe' 'vs_buildtools.exe'
    # 3010: installed, and Windows wants a restart, which nothing here needs.
    Run $installer ((@('--quiet', '--wait', '--norestart', '--nocache') +
        ($components | ForEach-Object { '--add', $_ })) -join ' ') @(0, 3010)
    $vs = Find-VS
    if (-not $vs) { throw "Visual Studio's installer finished, but vswhere doesn't find the tools" }
}
Write-Output "  $vs"
# Its clang for Arm64 hosts, which isn't on PATH outside a Developer prompt.
$clang = Get-ChildItem "$vs\VC\Tools\Llvm" -Recurse -Filter clang.exe |
    Where-Object { $_.DirectoryName -match '\\ARM64\\bin$' } | Select-Object -First 1
if (-not $clang) { throw "no clang.exe for Arm64 under $vs\VC\Tools\Llvm" }
Add-MachinePath $clang.DirectoryName

Step 'Git for Windows'
if (-not (Test-Path "$env:ProgramFiles\Git\cmd\git.exe")) {
    $release = Invoke-RestMethod 'https://api.github.com/repos/git-for-windows/git/releases/latest'
    $asset = $release.assets | Where-Object { $_.name -match '^Git-[\d.]+-arm64\.exe$' } | Select-Object -First 1
    $installer = Get-File $asset.browser_download_url $asset.name
    Run $installer '/VERYSILENT /NORESTART /NOCANCEL /SP- /SUPPRESSMSGBOXES'
}
Add-MachinePath "$env:ProgramFiles\Git\cmd"

Step 'Node 24'
$node = "$env:ProgramFiles\nodejs\node.exe"
if (-not ((Test-Path $node) -and ((& $node --version) -match '^v24\.'))) {
    # The newest 24 (the Node the Linux VM and CI use). Node's index doesn't list its Arm64 MSIs,
    # though every release since 20 has one.
    $version = (Invoke-RestMethod 'https://nodejs.org/dist/index.json' |
        Where-Object { $_.version -like 'v24.*' } | Select-Object -First 1).version
    $msi = Get-File "https://nodejs.org/dist/$version/node-$version-arm64.msi" "node-$version-arm64.msi"
    Run msiexec.exe "/i `"$msi`" /qn /norestart" @(0, 3010)
}
Refresh-Path

Step 'Rust'
$cargo = "$env:USERPROFILE\.cargo\bin\cargo.exe"
if (-not (Test-Path $cargo)) {
    $rustup = Get-File 'https://static.rust-lang.org/rustup/dist/aarch64-pc-windows-msvc/rustup-init.exe' 'rustup-init.exe'
    Run $rustup '-y --default-host aarch64-pc-windows-msvc --profile minimal'
}
Refresh-Path

Step 'Zig'
# The version the pinned Ghostty asks for (docs/development.md), as in the Linux VM.
$zigVersion = '0.17.0'
$zig = "$env:SystemDrive\tools\zig"
if (-not ((Test-Path "$zig\zig.exe") -and ((& "$zig\zig.exe" version) -eq $zigVersion))) {
    $zip = Get-File "https://ziglang.org/download/$zigVersion/zig-aarch64-windows-$zigVersion.zip" "zig-$zigVersion.zip"
    Remove-Item -Recurse -Force $zig, "$zig.unpacked" -ErrorAction SilentlyContinue
    Expand-Archive $zip "$zig.unpacked"
    Move-Item (Get-ChildItem "$zig.unpacked" | Select-Object -First 1).FullName $zig
    Remove-Item -Recurse -Force "$zig.unpacked"
}
Add-MachinePath $zig

Step 'Claude Code and Codex'
foreach ($package in '@anthropic-ai/claude-code', '@openai/codex') {
    npm ls -g --depth=0 $package | Out-Null
    if ($LASTEXITCODE) {
        npm install -g --no-audit --no-fund $package
        if ($LASTEXITCODE) { throw "npm couldn't install $package" }
    }
}

Step 'Versions'
cargo --version
node --version
zig version
clang --version | Select-Object -First 1
git --version
