# List non-system DLL dependencies of an exe.
# Run before a release: if the output is not empty, those DLLs must ship in the zip.
#
# Usage: powershell -ExecutionPolicy Bypass -File scripts/deps-check.ps1 target/release/liveporter.exe
param(
    [Parameter(Mandatory = $true)][string]$Exe,
    # When set, exit non-zero if any non-system DLL is found (for release gating).
    [switch]$FailOnFound
)

$ErrorActionPreference = "Stop"
if (-not (Test-Path -LiteralPath $Exe)) {
    Write-Error "File not found: $Exe"
    exit 1
}

function Find-Dumpbin {
    $cmd = Get-Command dumpbin.exe -ErrorAction SilentlyContinue
    if ($cmd) { return $cmd.Source }

    $vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
    if (Test-Path -LiteralPath $vswhere) {
        $vs = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath 2>$null
        if ($vs) {
            $db = Get-ChildItem (Join-Path $vs "VC\Tools\MSVC") -Recurse -Filter dumpbin.exe -ErrorAction SilentlyContinue |
                Select-Object -First 1 -ExpandProperty FullName
            if ($db) { return $db }
        }
    }
    return $null
}

$dumpbin = Find-Dumpbin
if (-not $dumpbin) {
    Write-Error "dumpbin.exe not found (install the Visual Studio C++ build tools)"
    exit 1
}

$output = & $dumpbin /dependents $Exe
$output | Write-Output

$system = @(
    'kernel32', 'user32', 'gdi32', 'advapi32', 'shell32', 'ole32', 'oleaut32',
    'combase', 'comctl32', 'shlwapi', 'propsys', 'ntdll', 'bcryptprimitives',
    'dwmapi', 'rpcrt4', 'sechost', 'ucrtbase', 'msvcrt', 'ws2_32', 'winmm',
    'version', 'imm32', 'crypt32', 'cfgmgr32', 'setupapi', 'userenv', 'win32u'
)

$deps = $output |
    Select-String -Pattern '^\s+([\w\-.]+\.dll)\s*$' |
    ForEach-Object { $_.Matches[0].Groups[1].Value } |
    Sort-Object -Unique

$nonSystem = $deps | Where-Object {
    $n = $_.ToLower()
    if ($n -like 'api-ms-win-*') { return $false }
    if ($n -like 'ext-ms-*') { return $false }
    return -not ($system -contains ($n -replace '\.dll$', ''))
}

Write-Output ""
if ($nonSystem) {
    Write-Output "Non-system DLLs (must ship in the zip):"
    $nonSystem | ForEach-Object { Write-Output "  $_" }
    if ($FailOnFound) { exit 2 }
    exit 0
}

Write-Output "OK: no non-system DLL dependency, nothing to bundle."
