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
    [switch]$SkipBuild,
    [switch]$SkipDepsCheck,
    [switch]$SkipFfmpegHashCheck,
    # Compute the current ffmpeg/ffprobe hashes and (re)write scripts/ffmpeg.sha256.
    [switch]$RecordFfmpegHash
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

# --- ffmpeg provenance: version/hash pinning ---------------------------------
$manifestPath = Join-Path $PSScriptRoot "ffmpeg.sha256"
$ffHash = (Get-FileHash -LiteralPath $ffmpeg -Algorithm SHA256).Hash
$fpHash = (Get-FileHash -LiteralPath $ffprobe -Algorithm SHA256).Hash

if ($RecordFfmpegHash) {
    $manifest = @(
        "# Expected SHA-256 of the trimmed FFmpeg binaries bundled by the portable build.",
        "# Verified by scripts/package-portable.ps1 before staging (fail on mismatch).",
        "#",
        "# Regenerate after rebuilding ffmpeg:",
        "#   powershell -ExecutionPolicy Bypass -File scripts/package-portable.ps1 ``",
        "#       -RecordFfmpegHash -FfmpegDir <dir> -SkipBuild -SkipDepsCheck",
        "#",
        "# Format: <sha256>  <filename>",
        "$ffHash  ffmpeg.exe",
        "$fpHash  ffprobe.exe"
    )
    Set-Content -LiteralPath $manifestPath -Value $manifest -Encoding ASCII
    Write-Host "==> wrote $manifestPath"
} elseif ($SkipFfmpegHashCheck) {
    Write-Warning "Skipping ffmpeg hash verification (-SkipFfmpegHashCheck)."
} elseif (Test-Path -LiteralPath $manifestPath) {
    $expected = @{}
    foreach ($line in Get-Content -LiteralPath $manifestPath) {
        if ($line -match '^\s*#') { continue }
        if ($line -match '^([0-9A-Fa-f]{64})\s+(\S+)\s*$') { $expected[$Matches[2]] = $Matches[1].ToUpper() }
    }
    foreach ($item in @(@('ffmpeg.exe', $ffHash), @('ffprobe.exe', $fpHash))) {
        $name = $item[0]
        $actual = $item[1]
        if (-not $expected.ContainsKey($name)) {
            Write-Error "scripts/ffmpeg.sha256 has no entry for $name. Regenerate with -RecordFfmpegHash."
            exit 1
        }
        if ($actual -ne $expected[$name]) {
            Write-Error ("ffmpeg hash mismatch for ${name}:`n  expected $($expected[$name])`n  actual   $actual`n" +
                "Rebuild ffmpeg or update scripts/ffmpeg.sha256 with -RecordFfmpegHash.")
            exit 1
        }
        Write-Host "==> $name SHA256 OK"
    }
    $ffVersion = (& $ffmpeg -version 2>&1 | Select-Object -First 1)
    Write-Host "==> $ffVersion"
} else {
    Write-Warning "scripts/ffmpeg.sha256 not found; skipping ffmpeg hash verification."
}

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

# --- dependency gate: every non-system DLL must be bundled -------------------
if ($SkipDepsCheck) {
    Write-Warning "Skipping dependency check (-SkipDepsCheck)."
} else {
    Write-Host "==> deps-check.ps1 $exe"
    & (Join-Path $PSScriptRoot "deps-check.ps1") -Exe $exe -FailOnFound
    if ($LASTEXITCODE -ne 0) {
        Write-Error "deps-check found non-system DLLs; add them to `$runtimeDlls below and re-run."
        exit $LASTEXITCODE
    }
}

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
