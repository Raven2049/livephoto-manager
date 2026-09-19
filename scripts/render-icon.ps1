# Render src-tauri/app-icon.svg to a 1024x1024 transparent PNG (src-tauri/app-icon.png),
# then regenerate the Tauri icon set.
#
# Requires Microsoft Edge (headless) for SVG rasterization. The committed PNG + icons/
# mean normal builds do NOT need to run this.
#
# Usage: powershell -ExecutionPolicy Bypass -File scripts/render-icon.ps1
$ErrorActionPreference = "Continue"
$root = Split-Path -Parent $PSScriptRoot

$edge = "C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe"
if (-not (Test-Path $edge)) { throw "msedge.exe not found (needed to rasterize the SVG)" }

$svg = Join-Path $root "src-tauri/app-icon.svg"
$png = Join-Path $root "src-tauri/app-icon.png"
$html = Join-Path $env:TEMP "lpm-icon.html"
if (-not (Test-Path $svg)) { throw "missing $svg" }

$doc = '<!doctype html><html><head><meta charset="utf-8">' +
    '<style>html,body{margin:0;padding:0;background:transparent}' +
    'img{display:block;width:1024px;height:1024px}</style></head><body>' +
    "<img src=`"file:///$($svg -replace '\\','/')`"></body></html>"
Set-Content -LiteralPath $html -Value $doc -Encoding ASCII

$url = "file:///" + ($html -replace '\\', '/')
Remove-Item -LiteralPath $png -ErrorAction SilentlyContinue
& $edge --headless=new --disable-gpu --no-first-run --user-data-dir="$env:TEMP\edge-lpm-icon" `
    --hide-scrollbars --force-device-scale-factor=1 `
    --default-background-color=00000000 --window-size=1024,1024 `
    "--screenshot=$png" $url 2>$null | Out-Null
if (-not (Test-Path $png)) { throw "render failed" }
Write-Output "rendered $png"

Push-Location $root
try { npx tauri icon "src-tauri/app-icon.png" } finally { Pop-Location }
