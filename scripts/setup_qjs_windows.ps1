param(
    [Parameter(Mandatory = $false)]
    [string] $Url,

    [Parameter(Mandatory = $false)]
    [string] $Sha256
)

$ErrorActionPreference = 'Stop'

$root = Split-Path -Parent $PSScriptRoot
$manifest = Join-Path $root 'runtime/QUICKJS'

if (Test-Path $manifest) {
    Get-Content $manifest | ForEach-Object {
        if ($_ -match '^\s*QJS_WIN_X64_URL\s*=\s*(.+)$' -and [string]::IsNullOrWhiteSpace($Url)) {
            $Url = $matches[1].Trim()
        }
        if ($_ -match '^\s*QJS_WIN_X64_SHA256\s*=\s*(.+)$' -and [string]::IsNullOrWhiteSpace($Sha256)) {
            $Sha256 = $matches[1].Trim()
        }
    }
}

if ([string]::IsNullOrWhiteSpace($Url) -or [string]::IsNullOrWhiteSpace($Sha256)) {
    throw "Missing QJS_WIN_X64_URL or QJS_WIN_X64_SHA256 in runtime/QUICKJS"
}

$runtimeDir = Join-Path $root 'runtime'
New-Item -ItemType Directory -Force $runtimeDir | Out-Null

$target = Join-Path $runtimeDir 'qjs.exe'
$tmp = Join-Path $runtimeDir 'qjs.exe.tmp'

if (Test-Path $target) {
    $existing = (Get-FileHash $target -Algorithm SHA256).Hash
    if ($existing -eq $Sha256.ToUpper()) {
        Write-Host "QuickJS already present and verified at $target"
        exit 0
    }
}

Write-Host "Downloading QuickJS from $Url"
Invoke-WebRequest $Url -OutFile $tmp

$actual = (Get-FileHash $tmp -Algorithm SHA256).Hash
if ($actual -ne $Sha256.ToUpper()) {
    Remove-Item -Force $tmp -ErrorAction SilentlyContinue
    throw "SHA256 mismatch: expected $($Sha256.ToUpper()), got $actual"
}

Move-Item -Force $tmp $target
Write-Host "QuickJS ready at $target"

