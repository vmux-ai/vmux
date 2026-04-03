#Requires -Version 5.1
[CmdletBinding()]
param(
    [switch]$Install,
    [switch]$CefOnly,
    [switch]$BuildDeps
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Continue'

$DX_VERSION = '0.7.4'
$EXPORT_CEF_VERSION = '145.6.1+145.0.28'

$CargoDefault = Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe'
$RustupDefault = Join-Path $env:USERPROFILE '.cargo\bin\rustup.exe'
$CargoBin = if ($env:CARGO_BIN) { $env:CARGO_BIN } else { $CargoDefault }
$RustupBin = if ($env:RUSTUP_BIN) { $env:RUSTUP_BIN } else { $RustupDefault }

$ShareRoot = Join-Path $env:USERPROFILE '.local\share'
$CefDir = Join-Path $ShareRoot 'cef'
if ($env:CEF_DIR) {
    $cefOverride = $env:CEF_DIR.Trim()
    if ($cefOverride -and (Test-Path -LiteralPath $cefOverride)) {
        $CefDir = $cefOverride
    }
}
$RenderExe = Join-Path $CefDir 'bevy_cef_render_process.exe'
$RenderExeBin = Join-Path $CefDir 'bin\bevy_cef_render_process.exe'

function Test-Executable([string]$Path) {
    return ($Path -and (Test-Path -LiteralPath $Path) -and ((Get-Item -LiteralPath $Path).Extension -match '\.(exe|bat|cmd)?$'))
}

function Sync-DoctorPathFromRegistry {
    try {
        $machine = [Environment]::GetEnvironmentVariable('Path', 'Machine')
        $user = [Environment]::GetEnvironmentVariable('Path', 'User')
        $segments = [System.Collections.Generic.List[string]]::new()
        foreach ($src in @($machine, $user, $env:Path)) {
            if ([string]::IsNullOrWhiteSpace($src)) { continue }
            foreach ($piece in $src.Split(';', [StringSplitOptions]::RemoveEmptyEntries)) {
                $t = $piece.Trim()
                if ($t) { $segments.Add($t) | Out-Null }
            }
        }
        $seen = @{}
        $deduped = foreach ($s in $segments) {
            $k = $s.ToLowerInvariant()
            if (-not $seen.ContainsKey($k)) {
                $seen[$k] = $true
                $s
            }
        }
        $env:Path = $deduped -join ';'
    } catch {
    }
}

function Resolve-CMake {
    $cmd = Get-Command cmake -ErrorAction SilentlyContinue
    if ($cmd) { return $cmd.Source }
    foreach ($candidate in @(
            (Join-Path $env:ProgramFiles 'CMake\bin\cmake.exe')
            (Join-Path (${env:ProgramFiles(x86)}) 'CMake\bin\cmake.exe')
        )) {
        if ($candidate -and (Test-Path -LiteralPath $candidate)) {
            return $candidate
        }
    }
    return $null
}

function Resolve-Ninja {
    $cmd = Get-Command ninja -ErrorAction SilentlyContinue
    if ($cmd) { return $cmd.Source }
    foreach ($candidate in @(
            (Join-Path $env:ProgramFiles 'Ninja\ninja.exe')
            (Join-Path $env:LOCALAPPDATA 'Microsoft\WinGet\Links\ninja.exe')
            (Join-Path $env:USERPROFILE 'scoop\shims\ninja.exe')
        )) {
        if ($candidate -and (Test-Path -LiteralPath $candidate)) {
            return $candidate
        }
    }
    return $null
}

function Resolve-Cargo {
    if (Test-Executable $CargoBin) { return $CargoBin }
    $cmd = Get-Command cargo -ErrorAction SilentlyContinue
    if ($cmd) { return $cmd.Source }
    return $null
}

function Resolve-Rustup {
    if (Test-Executable $RustupBin) { return $RustupBin }
    $cmd = Get-Command rustup -ErrorAction SilentlyContinue
    if ($cmd) { return $cmd.Source }
    return $null
}

function Resolve-Dx {
    $cmd = Get-Command dx -ErrorAction SilentlyContinue
    if ($cmd) { return $cmd.Source }
    $dxPath = Join-Path $env:USERPROFILE '.cargo\bin\dx.exe'
    if (Test-Executable $dxPath) { return $dxPath }
    return $null
}

function Resolve-ExportCefDir {
    $cmd = Get-Command export-cef-dir -ErrorAction SilentlyContinue
    if ($cmd) { return $cmd.Source }
    $p = Join-Path $env:USERPROFILE '.cargo\bin\export-cef-dir.exe'
    if (Test-Path -LiteralPath $p) { return $p }
    return $null
}

function Use-Color {
    return (-not $env:NO_COLOR) -and [Console]::IsOutputRedirected -eq $false
}

$color = Use-Color
function W([string]$s) { if ($color) { Write-Host $s } else { Write-Host ($s -replace '\x1b\[[0-9;]*m', '') } }

$pass = 0; $fail = 0; $warn = 0
$current = 0
$total = 9

function Bar { W ''; W ('-' * 44); W '' }

function Section([string]$title) { W "`n>> $title`n" }
function Tip([string]$t) { W "   -> $t" }

function OkLine([string]$msg) {
    $script:current++
    $script:pass++
    W ("  [OK] [{0,2}/{1}] {2}" -f $script:current, $script:total, $msg)
}
function WarnLine([string]$msg) {
    $script:current++
    $script:warn++
    W ("  [!!] [{0,2}/{1}] {2}" -f $script:current, $script:total, $msg)
}
function BadLine([string]$msg) {
    $script:current++
    $script:fail++
    W ("  [XX] [{0,2}/{1}] {2}" -f $script:current, $script:total, $msg)
}

function Test-RustupTargetInstalled {
    param(
        [Parameter(Mandatory = $true)][string]$RustupExe,
        [Parameter(Mandatory = $true)][string]$TargetTriple
    )
    $lines = @(& $RustupExe target list --installed 2>$null)
    if (-not $lines) {
        return $false
    }
    foreach ($line in $lines) {
        $t = ($line -replace '\s+', ' ').Trim()
        if ($t -match '^\s*([A-Za-z0-9_.-]+)') {
            $triple = $Matches[1]
            if ($triple -eq $TargetTriple) {
                return $true
            }
        }
    }
    return $false
}

function Invoke-VmuxCefBootstrap {
    param([string]$CargoExe)
    W 'Installing export-cef-dir...'
    & $CargoExe install "export-cef-dir@$EXPORT_CEF_VERSION" --force
    $exportCef2 = Resolve-ExportCefDir
    if (-not $exportCef2) {
        W 'export-cef-dir still not on PATH. Add %USERPROFILE%\.cargo\bin to PATH and re-run.'
        exit 1
    }
    New-Item -ItemType Directory -Force -Path $CefDir | Out-Null
    W "Downloading CEF into $CefDir ..."
    & $exportCef2 --force $CefDir
    W 'Installing bevy_cef_render_process into CEF dir...'
    & $CargoExe install bevy_cef_render_process --root $CefDir --force
}

if ($CefOnly) {
    Sync-DoctorPathFromRegistry
    W ''
    W 'Vmux CEF setup (Windows) - export-cef-dir + render process'
    W ''
    $cargoCef = Resolve-Cargo
    if (-not $cargoCef) {
        W 'cargo missing; install Rust from https://rustup.rs/ first.'
        exit 1
    }
    Invoke-VmuxCefBootstrap -CargoExe $cargoCef
    W ''
    W 'Done. Run .\scripts\doctor-windows.ps1 for a full prerequisite report.'
    W ''
    exit 0
}

if ($BuildDeps) {
    Sync-DoctorPathFromRegistry
    $cargo = Resolve-Cargo
    $rustup = Resolve-Rustup
    if (-not $cargo) {
        W 'cargo missing; install Rust first.'
        exit 1
    }
    if (-not $rustup) {
        W 'rustup missing; install Rust first.'
        exit 1
    }
    if (-not (Test-RustupTargetInstalled -RustupExe $rustup -TargetTriple 'wasm32-unknown-unknown')) {
        W 'Adding wasm32-unknown-unknown...'
        & $rustup target add wasm32-unknown-unknown
    }
    $dxNow = Resolve-Dx
    $needDx = $true
    if ($dxNow) {
        $v = & $dxNow --version 2>$null
        if ($v -match $DX_VERSION) { $needDx = $false }
    }
    if ($needDx) {
        W "Installing dioxus-cli $DX_VERSION..."
        & $cargo install dioxus-cli --locked --version $DX_VERSION
    }
    exit 0
}

if ($Install) {
    Sync-DoctorPathFromRegistry
    W ''
    W '-Install: wasm target, dioxus-cli, CEF, render process'
    W ''
    $cargo = Resolve-Cargo
    $rustup = Resolve-Rustup
    if (-not $cargo) {
        W 'cargo missing; install Rust first.'
        exit 1
    }
    if (-not $rustup) {
        W 'rustup missing; install Rust first.'
        exit 1
    }
    if (-not (Test-RustupTargetInstalled -RustupExe $rustup -TargetTriple 'wasm32-unknown-unknown')) {
        W 'Adding wasm32-unknown-unknown...'
        & $rustup target add wasm32-unknown-unknown
    }
    $dxNow = Resolve-Dx
    $needDx = $true
    if ($dxNow) {
        $v = & $dxNow --version 2>$null
        if ($v -match $DX_VERSION) { $needDx = $false }
    }
    if ($needDx) {
        W "Installing dioxus-cli $DX_VERSION..."
        & $cargo install dioxus-cli --locked --version $DX_VERSION
    }
    Invoke-VmuxCefBootstrap -CargoExe $cargo
    W ''
    W 'Re-running prerequisite checks...'
    W ''
    & $PSCommandPath
    exit $LASTEXITCODE
}

W "`nVmux Doctor (Windows)"
W "Checking prerequisites for building vmux_desktop..."
Sync-DoctorPathFromRegistry
Bar

$cargo = Resolve-Cargo
$rustup = Resolve-Rustup
$dx = Resolve-Dx
$exportCef = Resolve-ExportCefDir

Section 'CEF & paths'
if ($exportCef) {
    OkLine "export-cef-dir - $exportCef"
} else {
    WarnLine 'export-cef-dir not installed yet (expected before CEF download)'
    Tip ('After -Install, or run: cargo install export-cef-dir@' + $EXPORT_CEF_VERSION + ' --force')
    Tip 'Then: export-cef-dir --force "$env:USERPROFILE\.local\share\cef"'
    Tip 'Or set CEF_DIR to an existing CEF directory and rebuild.'
}

if (-not (Test-Path -LiteralPath $ShareRoot)) {
    BadLine "CEF base dir missing: $ShareRoot"
    Tip "Create it: New-Item -ItemType Directory -Force -Path `"$ShareRoot`""
} else {
    try {
        $testFile = Join-Path $ShareRoot '.vmux-doctor-write-test'
        [IO.File]::WriteAllText($testFile, 'ok')
        Remove-Item -LiteralPath $testFile -Force
        OkLine "CEF install base is writable - $ShareRoot"
    } catch {
        BadLine "CEF install base not writable: $ShareRoot"
    }
}

Section 'Rust toolchain'
if ($cargo) {
    OkLine "cargo - $cargo"
} else {
    BadLine 'cargo not found'
    Tip 'Install Rust: https://rustup.rs/'
}

if ($rustup) {
    if (Test-RustupTargetInstalled -RustupExe $rustup -TargetTriple 'wasm32-unknown-unknown') {
        OkLine 'rust target wasm32-unknown-unknown'
    } else {
        BadLine 'rust target wasm32-unknown-unknown missing'
        Tip "Run: `"$rustup`" target add wasm32-unknown-unknown"
    }
} else {
    BadLine 'rustup not found'
    Tip 'Install: https://rustup.rs/'
}

Section 'Native build tools'
$cmakeExe = Resolve-CMake
if ($cmakeExe) {
    OkLine "cmake - $cmakeExe"
} else {
    BadLine 'cmake not found'
    Tip 'Install CMake: winget install Kitware.CMake  (or https://cmake.org/download/)'
}

$ninjaExe = Resolve-Ninja
if ($ninjaExe) {
    OkLine "ninja - $ninjaExe"
} else {
    BadLine 'ninja not found'
    Tip 'Install Ninja: winget install Ninja-build.Ninja'
}

Section "Dioxus CLI (dx) - version $DX_VERSION"
if ($dx) {
    $verLine = & $dx --version 2>$null
    if ($verLine -match $DX_VERSION) {
        OkLine "dx - $dx ($verLine)"
    } else {
        BadLine "dx wrong or unknown version ($verLine)"
        Tip "Run: `"$cargo`" install dioxus-cli --locked --version $DX_VERSION"
    }
} else {
    BadLine 'dx not found'
    Tip "Run: cargo install dioxus-cli --locked --version $DX_VERSION"
}

Section 'CEF runtime & render process'
$cefDll = Join-Path $CefDir 'libcef.dll'
if ((Test-Path -LiteralPath $cefDll)) {
    OkLine "CEF binaries - $CefDir"
} else {
    BadLine 'CEF bundle missing (expected libcef.dll under .local\share\cef)'
    Tip 'Run: .\scripts\doctor-windows.ps1 -CefOnly   OR   make setup-windows (if you have make)'
}

if ((Test-Path -LiteralPath $RenderExe) -or (Test-Path -LiteralPath $RenderExeBin)) {
    OkLine 'bevy_cef_render_process.exe'
} else {
    BadLine 'bevy_cef_render_process.exe missing (avoids subprocess window flash)'
    Tip "Run: cargo install bevy_cef_render_process --root `"$CefDir`" --force"
}

$RepoRoot = Split-Path -Parent $PSScriptRoot
foreach ($rel in @('target\debug', 'target\release')) {
    $outDir = Join-Path $RepoRoot $rel
    $desktopExe = Join-Path $outDir 'vmux_desktop.exe'
    if (-not (Test-Path -LiteralPath $desktopExe)) { continue }
    if (-not (Test-Path -LiteralPath (Join-Path $outDir 'libcef.dll'))) {
        W ''
        W "[!!] $rel\vmux_desktop.exe exists but libcef.dll is missing next to it."
        W '     Run: cargo build -p vmux_desktop (copies from CEF_DIR or .local\share\cef).'
        W '     Close any running vmux_desktop before rebuilding if copy fails (file in use).'
    }
}

Bar
W ("Summary:  {0} ok" -f $pass)
if ($warn -gt 0) { W ("          {0} notes" -f $warn) }
if ($fail -gt 0) { W ("          {0} fix" -f $fail) }
Bar

if ($fail -eq 0) {
    W "`nAll required checks passed."
    W "Build debug:  cargo build -p vmux_desktop --features debug"
    W "Run:          .\target\debug\vmux_desktop.exe"
    W ""
    exit 0
}

W ''
W 'Fix the [XX] items above, then run this script again.'
W 'To auto-install Rust/Web/CEF pieces:  .\scripts\doctor-windows.ps1 -Install'
W '(Open a new prompt to run tip commands; this script only diagnoses.)'
if (-not $env:VMUX_DOCTOR_NO_WAIT) {
    try {
        if ([Environment]::UserInteractive -and -not [Console]::IsInputRedirected -and -not [Console]::IsOutputRedirected) {
            Read-Host 'Press Enter to exit'
        }
    } catch {
    }
}
exit 1
