# GizAi's one-line install for Windows, in PowerShell (Windows PowerShell 5.1 or PowerShell 7):
#   irm https://raw.githubusercontent.com/Yotech-AI/gizai/production/scripts/get.ps1 | iex
# It gets the source of the latest release, the tag Gizai's own update fetches (docs/RELEASING.md), into
# %LOCALAPPDATA%\gizai-src, or updates it there. Then it runs that release's install.ps1 with Windows PowerShell:
# install.ps1 -Check first, then install.ps1, which builds and installs Gizai as it does from a clone. Run it again to
# update. Nothing goes into Gizai's data folder. The source folder is this script's own: changes in it are replaced.
#
# Options go to install.ps1:  & ([scriptblock]::Create((irm <url>))) -Check   (see -Help)
# Environment: GIZAI_PREFIX goes to install.ps1 too ($env:GIZAI_PREFIX = 'D:\Apps\Gizai' first). GIZAI_REPO is the git
# repository, GIZAI_RELEASES_URL what it asks for the latest release (by default GitHub's; also http and file URLs),
# and GIZAI_REF a tag, branch or commit to get instead of the latest release (for tests and CI).
# Piped into iex, it runs in your own PowerShell session: it changes none of the session's settings, and it ends
# without exit, which would close the window (its result is in $LASTEXITCODE). Run as a file, it exits with it.
# Plain ASCII only: Windows PowerShell 5.1 reads a file without a byte order mark in the ANSI code page.
param(
  [switch]$Check,
  [switch]$BuildOnly,
  [switch]$SkipBuild,
  [switch]$Uninstall,
  [switch]$Purge,
  [switch]$Help
)

# Everything runs in a scope of its own. {}.File is this script's file when it runs as one, and empty from irm | iex.
& {
  param([bool]$AsFile)
  Set-StrictMode -Version 2
  $ErrorActionPreference = 'Stop'
  $ProgressPreference = 'SilentlyContinue'
  # A program that fails is no PowerShell error here (PowerShell 7.3 and later can make it one): its exit code says.
  $PSNativeCommandUseErrorActionPreference = $false

  $DefaultRepo = 'https://github.com/Yotech-AI/gizai.git'
  $GetPs1 = 'https://raw.githubusercontent.com/Yotech-AI/gizai/production/scripts/get.ps1'
  $GetSh = 'https://raw.githubusercontent.com/Yotech-AI/gizai/production/scripts/get.sh'

  function Say([string]$Text) { Write-Output $Text }
  function Step([string]$Text) { Write-Output ''; Write-Output "== $Text" }
  function Have([string]$Name) { [bool](Get-Command $Name -ErrorAction SilentlyContinue) }
  # The result: as a file, the exit code; from iex only $LASTEXITCODE, as exit would close your PowerShell window.
  function Finish([int]$Code) {
    $global:LASTEXITCODE = $Code
    if ($AsFile) { exit $Code }
  }
  # Runs a program, its output (errors included) going out as plain lines: Windows PowerShell would turn each line a
  # program writes to its error output into a PowerShell error. Its exit code is in $LASTEXITCODE.
  function Invoke-Shown([string]$Exe, [string[]]$Arguments) {
    $ErrorActionPreference = 'Continue'
    & $Exe @Arguments 2>&1 | ForEach-Object { "$_" }
  }

  if ($Help) {
    Say "GizAi's one-line install for Windows, in PowerShell:"
    Say "  irm $GetPs1 | iex"
    Say 'It gets the source of the latest release into %LOCALAPPDATA%\gizai-src (or updates it there), then runs that'
    Say "release's install.ps1: -Check first, then the install. Run it again to update."
    Say ''
    Say 'Options go to install.ps1:'
    Say '  & ([scriptblock]::Create((irm <url>))) -Check       get the source and show what is missing'
    Say '  & ([scriptblock]::Create((irm <url>))) -Uninstall   remove GizAi; add -Purge to delete your data and the source'
    Say '  -BuildOnly and -SkipBuild work as they do for install.ps1 (see install.ps1 -Help).'
    Say 'Environment: GIZAI_PREFIX (default %LOCALAPPDATA%\Programs\Gizai), as for install.ps1; GIZAI_REPO,'
    Say 'GIZAI_RELEASES_URL, and GIZAI_REF (a tag, branch or commit to get instead of the latest release, for tests).'
    Finish 0; return
  }

  try {
    if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
      Say "This installs GizAi on Windows. On Linux and macOS, run: curl -fsSL $GetSh | sh"
      Finish 1; return
    }
    if (-not (Have 'git')) {
      Say 'To install GizAi, first install Git for Windows (winget install --id Git.Git -e), then open a new PowerShell window and run this again.'
      Finish 1; return
    }

    $repo = if ($env:GIZAI_REPO) { $env:GIZAI_REPO } else { $DefaultRepo }
    # a folder of the repository: as a full path, as git runs in the source folder
    if (Test-Path -LiteralPath $repo -PathType Container) { $repo = (Resolve-Path -LiteralPath $repo).ProviderPath }
    $ref = "$env:GIZAI_REF"
    $tag = ''
    if (-not $ref) {
      $url = "$env:GIZAI_RELEASES_URL"
      if (-not $url) {
        if ($repo -match '^(https://github\.com/|git@github\.com:|ssh://git@github\.com/)(.+?)(\.git)?/?$') {
          $url = "https://api.github.com/repos/$($Matches[2])/releases/latest"
        } else {
          Say "$repo isn't on GitHub, so there is no latest release to ask it for: set GIZAI_RELEASES_URL or GIZAI_REF."
          Finish 1; return
        }
      }
      Step "Finding GizAi's latest release"
      try {
        if ($url -like 'file:*') {
          $release = Get-Content -LiteralPath ([Uri]$url).LocalPath -Raw | ConvertFrom-Json
        } else {
          # GitHub takes TLS 1.2 or newer, which Windows PowerShell 5.1 may not offer by itself. A locked-down PowerShell
          # may refuse this; the request may still work.
          try { [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12 } catch { }
          $release = Invoke-RestMethod -Uri $url -UseBasicParsing -TimeoutSec 60
        }
        $tag = "$($release.tag_name)"
      } catch {
        Say "Could not read the latest release from ${url}: $($_.Exception.Message)"
        Say 'Check your internet connection, then run this again.'
        Finish 1; return
      }
      if ($tag -notmatch '^v[0-9]+\.[0-9]+\.[0-9]+$') {
        $shown = if ($tag) { $tag } else { 'missing' }
        Say "The latest release's tag, $shown, isn't a version like v1.2.3, so there is nothing to install."
        Finish 1; return
      }
      Say "Latest release: $tag"
    }
    $what = if ($tag) { $tag } else { $ref }

    $dir = Join-Path $env:LOCALAPPDATA 'gizai-src'
    $gitDir = Join-Path $dir '.git'
    Step "Getting $what into $dir"
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    # git in the source folder's own repository only, never in one around it. A .git that git can't use (one cut
    # short) is made again.
    $git = @('-C', $dir, "--git-dir=$gitDir", "--work-tree=$dir")
    $null = Invoke-Shown 'git' ($git + @('rev-parse', '--git-dir'))
    if ($LASTEXITCODE -ne 0) {
      if (Test-Path -LiteralPath $gitDir) { Remove-Item -LiteralPath $gitDir -Recurse -Force }
      Invoke-Shown 'git' @('-c', 'init.defaultBranch=main', 'init', '--quiet', $dir)
      if ($LASTEXITCODE -ne 0) { Say "Could not make a git repository in $dir."; Finish 1; return }
    }
    # As Gizai's own update: a shallow fetch of the tag alone, checked out as it is, and everything else in the folder
    # goes, except the build's target\, so the next build is quicker. No -- before the repository, as get.sh has:
    # Windows PowerShell may drop it.
    $spec = if ($ref) { $ref } else { "+refs/tags/${tag}:refs/tags/${tag}" }
    Invoke-Shown 'git' ($git + @('fetch', '--depth', '1', '--no-tags', '--force', $repo, $spec))
    if ($LASTEXITCODE -ne 0) { Say "Could not get $what from $repo."; Finish 1; return }
    Invoke-Shown 'git' ($git + @('checkout', '--quiet', '--force', '--detach', 'FETCH_HEAD'))
    if ($LASTEXITCODE -eq 0) { Invoke-Shown 'git' ($git + @('clean', '-ffdxq', '--exclude=/target/')) }
    if ($LASTEXITCODE -ne 0) { Say "Could not put $what in place in $dir."; Finish 1; return }
    $installer = Join-Path $dir 'install.ps1'
    if (-not (Test-Path -LiteralPath $installer)) {
      Say "$what has no install.ps1, so it can't be installed this way."
      Finish 1; return
    }

    $flags = @()
    if ($BuildOnly) { $flags += '-BuildOnly' }
    if ($SkipBuild) { $flags += '-SkipBuild' }
    if ($Uninstall) { $flags += '-Uninstall' }
    if ($Purge) { $flags += '-Purge' }
    # With Windows PowerShell, as Gizai's own update runs it, and Bypass, as the release's script isn't signed.
    $run = @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $installer)
    # The release's check first, which says what is missing. Not before -Uninstall: install.ps1 -Check -Uninstall
    # would uninstall.
    if (-not $Uninstall) {
      Invoke-Shown 'powershell.exe' ($run + @('-Check') + $flags)
      $code = $LASTEXITCODE
      if ($code -ne 0) {
        if ($code -eq 1 -and -not $Check) {
          Say ''
          Say 'Install what is missing above, then run this again.'
        }
        Finish $code; return
      }
      if ($Check) { Finish 0; return }
    }
    Invoke-Shown 'powershell.exe' ($run + $flags)
    Finish $LASTEXITCODE
  } catch {
    Say "GizAi could not be installed: $($_.Exception.Message)"
    Finish 1
  }
} ($null -ne {}.File)
