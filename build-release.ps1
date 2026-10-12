<#
.SYNOPSIS
    Builds the release exe and packs it for distribution.

.DESCRIPTION
    cargo build --release, then dist\nvencFFX-<version>\ with nvencFFX.exe, the
    presets folder, LICENSE.txt and THIRD-PARTY-NOTICES.txt, zipped as
    dist\nvencFFX-<version>_no_ffmpeg.zip (ffmpeg / ffprobe are not included).
    THIRD-PARTY-NOTICES.txt lists the fonts and crates inside the exe with their
    licenses; cargo-about generates it from about.toml and about.hbs
    (cargo install --locked cargo-about --features cli).
    The help texts, fonts and icon are inside the exe; nff_settings.json is created
    next to it on the first run. ffmpeg.exe / ffprobe.exe placed next to nvencFFX.exe
    are found automatically.

.PARAMETER Upx
    Compress nvencFFX.exe with UPX (--best --lzma): about 8.5 MB -> 2.9 MB. The packed
    exe opens its window about 0.25 s later (it unpacks itself into memory first), and
    some antivirus scanners flag UPX-packed executables.

.PARAMETER UpxPath
    upx.exe to use (default: upx on PATH).

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File build-release.ps1
    powershell -ExecutionPolicy Bypass -File build-release.ps1 -Upx -UpxPath E:\ffmpeg\upx.exe
#>
param(
    [switch]$Upx,
    [string]$UpxPath = 'upx'
)

$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot

# Runs a native command; its stderr (cargo's progress) is output, not an error
function Invoke-Native([string]$Exe, [string[]]$Arguments) {
    $ErrorActionPreference = 'Continue'
    & $Exe @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$Exe failed with exit code $LASTEXITCODE" }
}

$version = (Select-String -Path Cargo.toml -Pattern '^version\s*=\s*"(.+)"' |
    Select-Object -First 1).Matches[0].Groups[1].Value
$name = "nvencFFX-$version"
$out = Join-Path 'dist' $name
$exe = Join-Path $out 'nvencFFX.exe'
$zip = Join-Path 'dist' "${name}_no_ffmpeg.zip"

if (-not (Get-Command cargo-about -ErrorAction SilentlyContinue)) {
    throw 'cargo-about not found: cargo install --locked cargo-about --features cli'
}

Invoke-Native cargo @('build', '--release')

if (Test-Path $out) { Remove-Item $out -Recurse -Force }
if (Test-Path $zip) { Remove-Item $zip -Force }
New-Item -ItemType Directory -Force (Join-Path $out 'presets') | Out-Null
Copy-Item 'target\release\nvencffx.exe' $exe
Copy-Item 'presets\*.json' (Join-Path $out 'presets')
Copy-Item 'LICENSE' (Join-Path $out 'LICENSE.txt')
# --fail: a crate whose license cannot be determined stops the build
Invoke-Native cargo @('about', 'generate', '--locked', '--fail', 'about.hbs',
    '-o', (Join-Path $out 'THIRD-PARTY-NOTICES.txt'))

if ($Upx) {
    Invoke-Native $UpxPath @('--best', '--lzma', '-q', $exe)
}

Compress-Archive -Path $out -DestinationPath $zip

# The exe creates nff_settings.json on the first run; a shipped one would carry local paths
Add-Type -AssemblyName System.IO.Compression.FileSystem
$archive = [IO.Compression.ZipFile]::OpenRead((Resolve-Path $zip))
try { $stray = @($archive.Entries | Where-Object { $_.Name -eq 'nff_settings.json' }) }
finally { $archive.Dispose() }
if ($stray) {
    Remove-Item $zip -Force
    throw "nff_settings.json must not be in the archive: $($stray.FullName -join ', ')"
}

Get-Item $exe, $zip | ForEach-Object {
    '{0,-28} {1,8:N2} MB' -f $_.Name, ($_.Length / 1MB)
}
