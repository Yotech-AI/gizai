# Gizai installer for Windows. Builds Gizai from source and installs it for you alone (no administrator rights):
#   %LOCALAPPDATA%\Programs\Gizai\   gizai.exe, gizai-mcp.exe (the helper chat uses) and gizai-installed.txt
#   Start menu\Programs\Gizai.lnk     the shortcut that starts Gizai and lets Windows show its notifications
# Your data lives in %APPDATA%\Gizai and is never touched by install or uninstall (unless -Purge).
# Gizai runs the coding CLIs installed on Windows itself; it never uses WSL.
#
# Usage, in PowerShell (Windows PowerShell 5.1 or PowerShell 7):
#   .\install.ps1                from a checkout: check, build and install
#   .\install.ps1 -Check         only report what is missing
#   .\install.ps1 -BuildOnly     check and build, install nothing (Gizai's own Update runs this, then -SkipBuild)
#   .\install.ps1 -SkipBuild     install the programs already built in target\release
#   .\install.ps1 -Uninstall     remove Gizai (add -Purge to delete your data too)
# When PowerShell won't run scripts: powershell -NoProfile -ExecutionPolicy Bypass -File .\install.ps1
# Run outside a checkout, it clones (or updates) the source into %LOCALAPPDATA%\gizai-src first.
# Environment: GIZAI_REPO (the git URL to clone), GIZAI_BRANCH (default production: the released code; main is
# development), GIZAI_PREFIX (default %LOCALAPPDATA%\Programs\Gizai), GIZAI_DATA_DIR (default %APPDATA%\Gizai).
[CmdletBinding()]
param(
  [switch]$Check,
  [switch]$BuildOnly,
  [switch]$SkipBuild,
  [switch]$Uninstall,
  [switch]$Purge,
  [switch]$Help
)
Set-StrictMode -Version 2
$ErrorActionPreference = 'Stop'

function Say([string]$Text) { Write-Output $Text }
function Step([string]$Text) { Write-Output ''; Write-Output "== $Text" }
function Have([string]$Name) { [bool](Get-Command $Name -ErrorAction SilentlyContinue) }
# A path made absolute from PowerShell's own folder (.NET would resolve a relative one from another folder).
function Full([string]$Path) { $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($Path) }

# Runs a step's program, its output (errors included) going out as plain lines: Windows PowerShell would turn each line
# a program writes to its error output into a PowerShell error. Stops the install when the program fails.
function Invoke-Step([string]$Exe, [string[]]$Arguments) {
  $ErrorActionPreference = 'Continue'
  & $Exe @Arguments 2>&1 | ForEach-Object { "$_" }
  if ($LASTEXITCODE -ne 0) {
    Say "$Exe $($Arguments -join ' ') failed (exit code $LASTEXITCODE)."
    exit 1
  }
}

# A program's output, errors included, as one text.
function Read-Program([string]$Exe, [string[]]$Arguments) {
  $ErrorActionPreference = 'Continue'
  (& $Exe @Arguments 2>&1 | ForEach-Object { "$_" }) -join "`n"
}

# Runs gizai.exe and says whether it worked, with what it printed. gizai.exe is a window app without a console of its
# own: run directly, PowerShell neither waits for it nor shows its output. Start-Process waits, with the output in files.
function Invoke-Gizai([string]$Exe, [string[]]$Arguments) {
  $out = [System.IO.Path]::GetTempFileName()
  $err = [System.IO.Path]::GetTempFileName()
  try {
    $p = Start-Process -FilePath $Exe -ArgumentList $Arguments -NoNewWindow -Wait -PassThru -RedirectStandardOutput $out -RedirectStandardError $err
    $text = (@(Get-Content -LiteralPath $out) + @(Get-Content -LiteralPath $err)) -join "`n"
    [pscustomobject]@{ Ok = ($p.ExitCode -eq 0); Text = $text.Trim() }
  } catch {
    [pscustomobject]@{ Ok = $false; Text = $_.Exception.Message }
  } finally {
    Remove-Item -LiteralPath $out, $err -Force -ErrorAction SilentlyContinue
  }
}

if ($Help) {
  Get-Content -LiteralPath $PSCommandPath -TotalCount 16 | ForEach-Object { $_ -replace '^# ?', '' }
  exit 0
}
if ($BuildOnly -and $SkipBuild) {
  [Console]::Error.WriteLine("-BuildOnly and -SkipBuild don't go together (see -Help)")
  exit 2
}
if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
  Say 'install.ps1 is for Windows. On Linux and macOS, install Gizai with install.sh.'
  exit 1
}

$DefaultRepo = 'https://github.com/Yotech-AI/gizai.git'
$Repo = if ($env:GIZAI_REPO) { $env:GIZAI_REPO } else { $DefaultRepo }
$Prefix = if ($env:GIZAI_PREFIX) { $env:GIZAI_PREFIX } else { Join-Path $env:LOCALAPPDATA 'Programs\Gizai' }
$Prefix = Full $Prefix
$Data = if ($env:GIZAI_DATA_DIR) { $env:GIZAI_DATA_DIR } else { Join-Path $env:APPDATA 'Gizai' }
$Data = Full $Data
$SrcClone = Join-Path $env:LOCALAPPDATA 'gizai-src'
$StartMenu = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs'
$Shortcut = Join-Path $StartMenu 'Gizai.lnk'
$Exe = Join-Path $Prefix 'gizai.exe'
# An installed Gizai knows its install by this file (a build in target\release has none).
$Marker = Join-Path $Prefix 'gizai-installed.txt'
$Programs = @('gizai.exe', 'gizai-mcp.exe')

# ---------- uninstall ----------
if ($Uninstall) {
  Step 'Removing Gizai'
  try {
    foreach ($f in @($Programs) + @('gizai-installed.txt')) {
      $path = Join-Path $Prefix $f
      if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path -Force }
    }
  } catch {
    Say "Could not remove Gizai from ${Prefix}: is it open? Quit it (Quit Gizai completely, in its tray menu), then run this again."
    exit 1
  }
  Get-ChildItem -LiteralPath $Prefix -Force -Filter '.*.old.*' -ErrorAction SilentlyContinue | Remove-Item -Force -ErrorAction SilentlyContinue
  # The Start menu shortcut goes only when it starts this install's Gizai.
  if (Test-Path -LiteralPath $Shortcut) {
    $target = (New-Object -ComObject WScript.Shell).CreateShortcut($Shortcut).TargetPath
    if ($target -eq $Exe) { Remove-Item -LiteralPath $Shortcut -Force }
  }
  if (-not (Get-ChildItem -LiteralPath $Prefix -Force -ErrorAction SilentlyContinue)) {
    Remove-Item -LiteralPath $Prefix -Force -ErrorAction SilentlyContinue
  }
  Say 'Removed the app and the Start menu shortcut.'
  if ($Purge) {
    Remove-Item -LiteralPath $Data, $SrcClone -Recurse -Force -ErrorAction SilentlyContinue
    Say "Deleted your data ($Data) and the source copy ($SrcClone)."
  } else {
    Say "Your data stays in $Data (run with -Uninstall -Purge to delete it)."
  }
  exit 0
}

# ---------- check ----------
Step 'Checking what Gizai needs'
$notes = New-Object System.Collections.Generic.List[string]
if (-not (Have 'git')) {
  $notes.Add('Git for Windows is missing (Claude Code needs it too): winget install --id Git.Git -e   (then open a new terminal)')
}
if (-not $SkipBuild) {
  if (Have 'cargo') {
    $rustc = Read-Program 'rustc' @('-vV')
    if ($rustc -match 'host: (\S+)' -and $Matches[1] -notlike '*-windows-msvc') {
      $notes.Add("Rust builds for $($Matches[1]) here; Gizai needs Rust's MSVC toolchain: rustup default stable-msvc")
    }
  } else {
    $notes.Add('Rust is missing: winget install --id Rustlang.Rustup -e, or rustup-init.exe from https://rustup.rs   (then open a new terminal)')
  }
  $nodeFrom = 'winget install --id OpenJS.NodeJS.LTS -e, or from https://nodejs.org'
  if (Have 'node') {
    $nodeVersion = Read-Program 'node' @('-v')
    if (-not ($nodeVersion -match '^v(\d+)\.' -and [int]$Matches[1] -ge 20)) {
      $notes.Add("Node.js 20 or newer is needed (found $nodeVersion): $nodeFrom")
    }
  } else {
    $notes.Add("Node.js 20 or newer is missing: $nodeFrom")
  }
  if (-not (Have 'npm.cmd')) { $notes.Add('npm is missing (it comes with Node.js)') }
  # Rust's MSVC toolchain links with the Microsoft C++ Build Tools (Visual Studio's "Desktop development with C++").
  $vc = ''
  if (${env:ProgramFiles(x86)}) {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (Test-Path -LiteralPath $vswhere) {
      $vc = Read-Program $vswhere @('-latest', '-products', '*', '-requires', 'Microsoft.VisualStudio.Component.VC.Tools.x86.x64',
        'Microsoft.VisualStudio.Component.VC.Tools.ARM64', '-requiresAny', '-property', 'installationPath')
    }
  }
  if (-not $vc.Trim()) {
    $notes.Add('The Microsoft C++ Build Tools are missing: install them with "Desktop development with C++" from https://visualstudio.microsoft.com/visual-cpp-build-tools/ (rustup-init offers to as well)')
  }
}
$ok = $notes.Count -eq 0
foreach ($n in $notes) { Say $n }
$claude = Get-Command 'claude' -ErrorAction SilentlyContinue | Select-Object -First 1
if ($claude) {
  Say "Claude Code: found ($($claude.Source)). Make sure you are logged in: run 'claude' once."
} else {
  Say "Claude Code: not found. Gizai's agents and chat run it; install it from https://docs.claude.com/en/docs/claude-code and log in."
}
# Gizai's window is WebView2, which comes with Windows 11. Building doesn't need it; starting Gizai does.
$webView = '{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}'
$hasWebView = $false
foreach ($key in @("HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\$webView", "HKLM:\SOFTWARE\Microsoft\EdgeUpdate\Clients\$webView",
    "HKCU:\Software\Microsoft\EdgeUpdate\Clients\$webView")) {
  $pv = Get-ItemProperty -LiteralPath $key -Name 'pv' -ErrorAction SilentlyContinue
  if ($pv -and $pv.pv -and $pv.pv -ne '0.0.0.0') { $hasWebView = $true }
}
if (-not $hasWebView) {
  Say "WebView2: not found, so Gizai's window can't open. Windows 11 comes with it; get the Evergreen Runtime from https://developer.microsoft.com/microsoft-edge/webview2/"
}
if ($ok) { Say 'Everything Gizai needs to build is here.' }
if ($Check) { if ($ok) { exit 0 } else { exit 1 } }
if (-not $ok -and -not $SkipBuild) {
  Say ''
  Say 'Install the missing pieces above, then run this again.'
  exit 1
}

# ---------- source ----------
$conf = if ($PSScriptRoot) { Join-Path $PSScriptRoot 'src-tauri\tauri.conf.json' } else { '' }
if ($conf -and (Test-Path -LiteralPath $conf) -and (Select-String -LiteralPath $conf -Pattern '"productName": "Gizai"' -SimpleMatch -Quiet)) {
  $Src = $PSScriptRoot
} else {
  Step 'Getting the source'
  if (Test-Path -LiteralPath (Join-Path $SrcClone '.git')) {
    Invoke-Step 'git' @('-C', $SrcClone, 'pull', '--ff-only')
  } else {
    $branch = if ($env:GIZAI_BRANCH) { $env:GIZAI_BRANCH } else { 'production' }
    Invoke-Step 'git' @('clone', '--depth', '1', '--branch', $branch, $Repo, $SrcClone)
  }
  $Src = $SrcClone
}
Say "Source: $Src"

# ---------- build ----------
$Release = Join-Path $Src 'target\release'
if (-not $SkipBuild) {
  Step 'Building (this takes a few minutes the first time)'
  $quiet = @{ TAURI_TELEMETRY_DISABLED = '1'; npm_config_update_notifier = 'false'; npm_config_fund = 'false'; npm_config_audit = 'false' }
  $before = @{}
  foreach ($name in @($quiet.Keys)) {
    $before[$name] = [Environment]::GetEnvironmentVariable($name)
    [Environment]::SetEnvironmentVariable($name, $quiet[$name])
  }
  Push-Location -LiteralPath $Src
  try {
    # npm.cmd rather than npm: no npm.ps1, which the execution policy may refuse.
    Invoke-Step 'npm.cmd' @('ci')
    # The Tauri CLI, never a plain cargo build --release: that can make a gizai.exe that loads the dev URL. Called
    # directly rather than through npm run with "--", which Windows PowerShell may drop before npm sees it.
    Invoke-Step (Join-Path $Src 'node_modules\.bin\tauri.cmd') @('build', '--no-bundle')
    Invoke-Step 'cargo' @('build', '--release', '-p', 'gizai-mcp')
  } finally {
    Pop-Location
    foreach ($name in @($before.Keys)) { [Environment]::SetEnvironmentVariable($name, $before[$name]) }
  }
}
foreach ($p in $Programs) {
  if (-not (Test-Path -LiteralPath (Join-Path $Release $p))) {
    Say "$(Join-Path $Release $p) is missing: build first (run without -SkipBuild)."
    exit 1
  }
}
$NewGizai = Join-Path $Release 'gizai.exe'
if ($BuildOnly) {
  $built = Invoke-Gizai $NewGizai @('--version')
  $name = if ($built.Ok -and $built.Text) { $built.Text } else { 'gizai' }
  Step "Built: $name, in $Release"
  Say "Nothing was installed. Install it with: $(Join-Path $Src 'install.ps1') -SkipBuild"
  exit 0
}

# ---------- back up ----------
# Before replacing a Gizai you use, snapshot its data (the new build opens it, and may upgrade it, on its next start).
$db = Join-Path $Data 'gizai.db'
if (Test-Path -LiteralPath $db) {
  Step 'Backing up your data'
  $dataBefore = $env:GIZAI_DATA_DIR
  $env:GIZAI_DATA_DIR = $Data
  try { $snap = Invoke-Gizai $NewGizai @('--backup', 'before-install') } finally { $env:GIZAI_DATA_DIR = $dataBefore }
  if ($snap.Ok) {
    Say "Backed up your data to $($snap.Text)"
  } else {
    Say "Could not back up ${db}: $($snap.Text)"
    Say 'So nothing was installed. Move that file aside (or fix it), then run this again.'
    exit 1
  }
}

# ---------- install ----------
Step 'Installing'
New-Item -ItemType Directory -Force -Path $Prefix | Out-Null
# Programs an earlier install renamed aside go now (one that still runs stays until the next install).
Get-ChildItem -LiteralPath $Prefix -Force -Filter '.*.old.*' -ErrorAction SilentlyContinue | Remove-Item -Force -ErrorAction SilentlyContinue
# Each program goes in under a temporary name of this installer's own first, then replaces the old one with renames: a
# copy that fails (a full disk) leaves the installed Gizai as it was. Windows won't overwrite a program that runs (an
# open Gizai, or the helper a chat uses) but can rename it, so the old one is renamed aside first.
$new = @{}; $old = @{}
foreach ($p in $Programs) { $new[$p] = Join-Path $Prefix ".$p.new.$PID"; $old[$p] = Join-Path $Prefix ".$p.old.$PID" }
try {
  foreach ($p in $Programs) { Copy-Item -LiteralPath (Join-Path $Release $p) -Destination $new[$p] -Force }
} catch {
  foreach ($p in $Programs) { Remove-Item -LiteralPath $new[$p] -Force -ErrorAction SilentlyContinue }
  Say "Could not copy Gizai into ${Prefix}, so nothing was installed."
  exit 1
}
try {
  foreach ($p in $Programs) {
    $dst = Join-Path $Prefix $p
    if (Test-Path -LiteralPath $dst) { [System.IO.File]::Move($dst, $old[$p]) }
  }
  foreach ($p in $Programs) { [System.IO.File]::Move($new[$p], (Join-Path $Prefix $p)) }
} catch {
  $why = $_.Exception.Message
  # Put back what was renamed aside, so the Gizai you had stays as it was.
  foreach ($p in $Programs) {
    $dst = Join-Path $Prefix $p
    if ((Test-Path -LiteralPath $old[$p]) -and -not (Test-Path -LiteralPath $dst)) { [System.IO.File]::Move($old[$p], $dst) }
    Remove-Item -LiteralPath $new[$p] -Force -ErrorAction SilentlyContinue
  }
  Say "Could not replace Gizai in ${Prefix}: $why"
  Say 'So nothing was installed. If Gizai is open, quit it (Quit Gizai completely, in its tray menu), then run this again.'
  exit 1
}
foreach ($p in $Programs) { Remove-Item -LiteralPath $old[$p] -Force -ErrorAction SilentlyContinue }
$installed = Invoke-Gizai $Exe @('--version')
$version = if ($installed.Ok -and $installed.Text) { $installed.Text } else { 'gizai' }
Set-Content -LiteralPath $Marker -Value $version -Encoding Ascii

# The Start menu shortcut carries Gizai's AppUserModelID, ai.gizai.app: Windows shows a desktop app's notifications only
# for an ID that a Start menu shortcut has. WScript.Shell can't set one, so this C# sets it through IPropertyStore.
$shortcutCode = @'
using System;
using System.Runtime.InteropServices;
using System.Runtime.InteropServices.ComTypes;

namespace GizaiInstall
{
    [ComImport, Guid("00021401-0000-0000-C000-000000000046")]
    class CShellLink { }

    [ComImport, Guid("000214F9-0000-0000-C000-000000000046"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    interface IShellLinkW
    {
        void GetPath(IntPtr pszFile, int cch, IntPtr pfd, uint fFlags);
        void GetIDList(out IntPtr ppidl);
        void SetIDList(IntPtr pidl);
        void GetDescription(IntPtr pszName, int cch);
        void SetDescription([MarshalAs(UnmanagedType.LPWStr)] string pszName);
        void GetWorkingDirectory(IntPtr pszDir, int cch);
        void SetWorkingDirectory([MarshalAs(UnmanagedType.LPWStr)] string pszDir);
        void GetArguments(IntPtr pszArgs, int cch);
        void SetArguments([MarshalAs(UnmanagedType.LPWStr)] string pszArgs);
        void GetHotkey(out ushort pwHotkey);
        void SetHotkey(ushort wHotkey);
        void GetShowCmd(out int piShowCmd);
        void SetShowCmd(int iShowCmd);
        void GetIconLocation(IntPtr pszIconPath, int cch, out int piIcon);
        void SetIconLocation([MarshalAs(UnmanagedType.LPWStr)] string pszIconPath, int iIcon);
        void SetRelativePath([MarshalAs(UnmanagedType.LPWStr)] string pszPathRel, uint dwReserved);
        void Resolve(IntPtr hwnd, uint fFlags);
        void SetPath([MarshalAs(UnmanagedType.LPWStr)] string pszFile);
    }

    [StructLayout(LayoutKind.Sequential, Pack = 4)]
    struct PropertyKey
    {
        public Guid FormatId;
        public uint PropertyId;
    }

    [StructLayout(LayoutKind.Explicit, Size = 24)]
    struct PropVariant
    {
        [FieldOffset(0)] public ushort VarType;
        [FieldOffset(8)] public IntPtr Pointer;
    }

    [ComImport, Guid("886D8EEB-8CF2-4446-8D02-CDBA1DBDCF99"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    interface IPropertyStore
    {
        void GetCount(out uint cProps);
        void GetAt(uint iProp, out PropertyKey pkey);
        void GetValue(ref PropertyKey key, out PropVariant pv);
        void SetValue(ref PropertyKey key, ref PropVariant pv);
        void Commit();
    }

    public static class Shortcut
    {
        // Saves a shortcut at lnkPath that starts target, with the AppUserModelID appId (PKEY_AppUserModel_ID).
        public static void Save(string lnkPath, string target, string workingDir, string description, string appId)
        {
            IShellLinkW link = (IShellLinkW)new CShellLink();
            link.SetPath(target);
            link.SetWorkingDirectory(workingDir);
            link.SetDescription(description);
            link.SetIconLocation(target, 0);
            PropertyKey key = new PropertyKey();
            key.FormatId = new Guid("9F4C2855-9F79-4B39-A8D0-E1D42DE1D5F3");
            key.PropertyId = 5;
            PropVariant value = new PropVariant();
            value.VarType = 31; // VT_LPWSTR
            value.Pointer = Marshal.StringToCoTaskMemUni(appId);
            try
            {
                IPropertyStore store = (IPropertyStore)link;
                store.SetValue(ref key, ref value);
                store.Commit();
            }
            finally
            {
                Marshal.FreeCoTaskMem(value.Pointer);
            }
            ((IPersistFile)link).Save(lnkPath, true);
            Marshal.ReleaseComObject(link);
        }
    }
}
'@
try {
  if (-not ('GizaiInstall.Shortcut' -as [type])) { Add-Type -TypeDefinition $shortcutCode }
  New-Item -ItemType Directory -Force -Path $StartMenu | Out-Null
  [GizaiInstall.Shortcut]::Save($Shortcut, $Exe, $Prefix, 'Clients, projects and tasks, worked on by local AI agents', 'ai.gizai.app')
} catch {
  # Without the C# (a locked-down PowerShell), a plain shortcut: Gizai starts from the Start menu, without notifications.
  $why = $_.Exception.Message
  try {
    $link = (New-Object -ComObject WScript.Shell).CreateShortcut($Shortcut)
    $link.TargetPath = $Exe
    $link.WorkingDirectory = $Prefix
    $link.Save()
    Say "Note: the Start menu shortcut has no app ID ($why), so Windows may not show Gizai's notifications."
  } catch {
    Say "Could not make the Start menu shortcut ($why). Start Gizai with: $Exe"
  }
}

Step "Done: $version"
Say "Start Gizai from the Start menu, or run: & '$Exe'"
Say "Your data: $Data"
Say 'Update: Gizai offers new releases above Company in its sidebar (or run this installer again). Remove: run it with -Uninstall.'
