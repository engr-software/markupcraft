<#
.SYNOPSIS
  Build and package MarkupCraft for Windows.

.DESCRIPTION
  Produces, in $env:DIST (default: dist\):
    markupcraft-<version>-windows-<arch>.zip          portable: markupcraft.exe, markupcraft-cli.exe, docs
    markupcraft-<version>-windows-<arch>-setup.exe    NSIS installer (Start menu entry, uninstaller)

  The binaries link the C runtime statically (+crt-static), so neither package needs the Visual
  C++ redistributable. The installer needs NSIS (makensis): it is found through $env:MAKENSIS,
  PATH, or the default install folder. Without NSIS the installer is skipped with a warning,
  except under CI ($env:CI set), where it is an error.

  Runs on Windows PowerShell 5.1 and PowerShell 7.

.EXAMPLE
  powershell -NoProfile -ExecutionPolicy Bypass -File packaging\windows\package.ps1
  pwsh packaging/windows/package.ps1 -SkipBuild
#>
param(
  [ValidateSet('x64', 'arm64')] [string] $Arch = 'x64',
  [switch] $SkipBuild
)
$ErrorActionPreference = 'Stop'
$Root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path

function Invoke-Native([string] $What, [scriptblock] $Block) {
  Write-Output "==> $What"
  & $Block
  if ($LASTEXITCODE -ne 0) { throw "$What failed with exit code $LASTEXITCODE" }
}

function Write-Warn([string] $Message) {
  if ($env:GITHUB_ACTIONS) { Write-Output "::warning::$Message" } else { Write-Warning $Message }
}

# The version lives in one place: [workspace.package] version in the root Cargo.toml.
$Version = $env:MARKUPCRAFT_VERSION
if (-not $Version) {
  $inPkg = $false
  foreach ($line in Get-Content (Join-Path $Root 'Cargo.toml')) {
    if ($line -match '^\s*\[') { $inPkg = ($line.Trim() -eq '[workspace.package]'); continue }
    if ($inPkg -and $line -match '^\s*version\s*=\s*"([^"]+)"') { $Version = $Matches[1]; break }
  }
}
if (-not $Version) { throw 'could not read [workspace.package] version from Cargo.toml' }
# The installer's VIProductVersion is numeric X.Y.Z.W; a pre-release suffix is dropped there.
$NumericVersion = ($Version -split '-')[0] + '.0'

$Target = if ($Arch -eq 'arm64') { 'aarch64-pc-windows-msvc' } else { 'x86_64-pc-windows-msvc' }
$Dist = if ($env:DIST) { $env:DIST } else { Join-Path $Root 'dist' }
if (-not [IO.Path]::IsPathRooted($Dist)) { $Dist = Join-Path $Root $Dist }
$TargetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $Root 'target' }
if (-not [IO.Path]::IsPathRooted($TargetDir)) { $TargetDir = Join-Path $Root $TargetDir }
New-Item -ItemType Directory -Force -Path $Dist | Out-Null

Write-Output "MarkupCraft $Version for Windows $Arch ($Target)"

if (-not $SkipBuild) {
  # Static CRT, scoped to the target so build scripts and proc-macros are unaffected.
  $flagVar = 'CARGO_TARGET_' + ($Target.ToUpper() -replace '-', '_') + '_RUSTFLAGS'
  [Environment]::SetEnvironmentVariable($flagVar, '-C target-feature=+crt-static')
  Push-Location $Root
  try {
    Invoke-Native "cargo build ($Target)" { cargo build --release --locked -p markupcraft -p markupcraft-cli --target $Target }
  } finally { Pop-Location }
}

$Bin = Join-Path $TargetDir "$Target\release"
foreach ($exe in 'markupcraft.exe', 'markupcraft-cli.exe') {
  if (-not (Test-Path (Join-Path $Bin $exe))) { throw "$exe not found in $Bin (build first, or drop -SkipBuild)" }
}

# ---- stage ---------------------------------------------------------------------------------------
$Base = "markupcraft-$Version-windows-$Arch"
$Stage = Join-Path $TargetDir "windows-package\$Base"
if (Test-Path $Stage) { Remove-Item -Recurse -Force $Stage }
New-Item -ItemType Directory -Force -Path $Stage | Out-Null
Copy-Item (Join-Path $Bin 'markupcraft.exe'), (Join-Path $Bin 'markupcraft-cli.exe') $Stage
foreach ($f in 'README.md', 'LICENSE-MIT', 'LICENSE-APACHE', 'THIRD_PARTY.md') {
  $p = Join-Path $Root $f
  if (Test-Path $p) { Copy-Item $p $Stage }
}
Copy-Item (Join-Path $Root 'assets\markupcraft.ico') $Stage

# ---- portable zip --------------------------------------------------------------------------------
$Zip = Join-Path $Dist "$Base.zip"
if (Test-Path $Zip) { Remove-Item -Force $Zip }
Compress-Archive -Path $Stage -DestinationPath $Zip
Write-Output "wrote $Zip"

# ---- installer (NSIS) ----------------------------------------------------------------------------
$MakeNsis = $env:MAKENSIS
if (-not $MakeNsis) {
  $cmd = Get-Command makensis -ErrorAction SilentlyContinue
  if ($cmd) { $MakeNsis = $cmd.Source }
}
if (-not $MakeNsis) {
  $default = Join-Path ${env:ProgramFiles(x86)} 'NSIS\makensis.exe'
  if (Test-Path $default) { $MakeNsis = $default }
}
$Setup = Join-Path $Dist "$Base-setup.exe"
if ($MakeNsis) {
  if (Test-Path $Setup) { Remove-Item -Force $Setup }
  Invoke-Native 'makensis' {
    & $MakeNsis /V3 /INPUTCHARSET UTF8 "/DVERSION=$Version" "/DNUMERIC_VERSION=$NumericVersion" `
      "/DSTAGE=$Stage" "/DICON=$(Join-Path $Root 'assets\markupcraft.ico')" "/DOUTFILE=$Setup" `
      (Join-Path $PSScriptRoot 'installer.nsi')
  }
  Write-Output "wrote $Setup"
} elseif ($env:CI) {
  throw 'makensis not found (install NSIS, or set MAKENSIS to makensis.exe)'
} else {
  Write-Warn 'makensis not found: skipped the installer (install NSIS, or set MAKENSIS)'
  $Setup = $null
}

# ---- smoke test ----------------------------------------------------------------------------------
# The CLI must start from the staged folder. Without arguments it prints its usage and exits 2.
$HostArch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString().ToLowerInvariant()
if ($Arch -eq 'x64' -or $HostArch -eq 'arm64') {
  $cli = Join-Path $Stage 'markupcraft-cli.exe'
  $p = Start-Process -FilePath $cli -Wait -PassThru -NoNewWindow -RedirectStandardError (Join-Path $TargetDir 'windows-package\cli-usage.txt')
  if ($p.ExitCode -gt 2) { throw "markupcraft-cli.exe did not start from the package (exit $($p.ExitCode))" }
  Write-Output "ok: markupcraft-cli.exe starts (exit $($p.ExitCode))"
}

$out = @($Zip)
if ($Setup) { $out += $Setup }
Get-Item $out | Format-Table Name, Length
