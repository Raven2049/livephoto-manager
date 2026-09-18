# Assemble the portable ("green") Windows zip for LivePorter.
#
# Layout inside the zip:
#   LivePorter/
#     LivePorter.exe
#     resources/ffmpeg.exe
#     resources/ffprobe.exe
#     LICENSE, THIRD_PARTY_NOTICES.md, README.txt
#
# Usage:
#   powershell -ExecutionPolicy Bypass -File scripts/package-portable.ps1 -FfmpegDir <dir>
#   (or set LIVEPORTER_FFMPEG and omit -FfmpegDir)
param(
    [string]$FfmpegDir = "",
    [string]$OutDir = "dist-portable",
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $root

# --- locate ffmpeg/ffprobe -------------------------------------------------
if (-not $FfmpegDir) {
    if ($env:LIVEPORTER_FFMPEG) {
        $FfmpegDir = Split-Path -Parent $env:LIVEPORTER_FFMPEG
    } else {
        Write-Error "Provide -FfmpegDir or set the LIVEPORTER_FFMPEG environment variable."
        exit 1
    }
}
$ffmpeg = Join-Path $FfmpegDir "ffmpeg.exe"
$ffprobe = Join-Path $FfmpegDir "ffprobe.exe"
if (-not (Test-Path -LiteralPath $ffmpeg)) { Write-Error "ffmpeg.exe not found in: $FfmpegDir"; exit 1 }
if (-not (Test-Path -LiteralPath $ffprobe)) { Write-Error "ffprobe.exe not found in: $FfmpegDir"; exit 1 }

# --- version ---------------------------------------------------------------
$confPath = Join-Path $root "src-tauri/tauri.conf.json"
$version = (Get-Content -LiteralPath $confPath -Raw | ConvertFrom-Json).version
if (-not $version) { Write-Error "Cannot read version from $confPath"; exit 1 }
Write-Host "==> LivePorter $version (portable)"

# --- build -----------------------------------------------------------------
if (-not $SkipBuild) {
    Write-Host "==> npm run build"
    npm run build
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    Write-Host "==> tauri build --no-bundle"
    npx tauri build --no-bundle
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}

$exe = Join-Path $root "target/release/liveporter.exe"
if (-not (Test-Path -LiteralPath $exe)) { Write-Error "Release exe not found: $exe"; exit 1 }

# --- stage -----------------------------------------------------------------
$stage = Join-Path $root $OutDir
$appDir = Join-Path $stage "LivePorter"
if (Test-Path -LiteralPath $appDir) { Remove-Item -LiteralPath $appDir -Recurse -Force }
New-Item -ItemType Directory -Force -Path (Join-Path $appDir "resources") | Out-Null

Copy-Item -LiteralPath $exe -Destination (Join-Path $appDir "LivePorter.exe") -Force
Copy-Item -LiteralPath $ffmpeg -Destination (Join-Path $appDir "resources/ffmpeg.exe") -Force
Copy-Item -LiteralPath $ffprobe -Destination (Join-Path $appDir "resources/ffprobe.exe") -Force

Copy-Item -LiteralPath (Join-Path $root "LICENSE") -Destination $appDir -Force
Copy-Item -LiteralPath (Join-Path $root "THIRD_PARTY_NOTICES.md") -Destination $appDir -Force

$readme = Join-Path $root "docs/portable-readme.txt"
if (Test-Path -LiteralPath $readme) {
    Copy-Item -LiteralPath $readme -Destination (Join-Path $appDir "README.txt") -Force
} else {
    Write-Warning "docs/portable-readme.txt not found; README.txt will be missing from the zip."
}

# Runtime DLLs: deps-check.ps1 currently reports none. If that changes,
# add the names here and copy them from $FfmpegDir (or a DLL dir).
$runtimeDlls = @()
foreach ($dll in $runtimeDlls) {
    $src = Join-Path $FfmpegDir $dll
    if (Test-Path -LiteralPath $src) {
        Copy-Item -LiteralPath $src -Destination $appDir -Force
    } else {
        Write-Warning "runtime dll not found: $src"
    }
}

# --- size report -----------------------------------------------------------
$bundleBytes = (Get-Item (Join-Path $appDir "resources/ffmpeg.exe")).Length +
    (Get-Item (Join-Path $appDir "resources/ffprobe.exe")).Length
$bundleMB = [math]::Round($bundleBytes / 1MB, 1)
Write-Host "==> ffmpeg + ffprobe = $bundleMB MB (target 15-25 MB)"
if ($bundleMB -gt 40) {
    Write-Warning "ffmpeg bundle looks large ($bundleMB MB). Is this a trimmed build?"
}

# --- zip + checksum --------------------------------------------------------
$zip = Join-Path $stage "LivePorter-$version-windows-x64.zip"
if (Test-Path -LiteralPath $zip) { Remove-Item -LiteralPath $zip -Force }
Compress-Archive -LiteralPath $appDir -DestinationPath $zip -CompressionLevel Optimal

$hash = (Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash
"$hash  $(Split-Path -Leaf $zip)" | Set-Content -LiteralPath "$zip.sha256" -Encoding ASCII

Write-Host "==> $zip"
Write-Host "    SHA256 $hash"
