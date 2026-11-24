param(
    [Parameter(Mandatory = $false)]
    [string] $Url = "https://github.com/shinchiro/mpv-winbuild-cmake/releases/download/20261111/mpv-dev-x86_64-20261111-git-bcde6b93cc.7z",

    [Parameter(Mandatory = $false)]
    [string] $Sha256 = "273C079B11EA5575822F972756F4C9A71149A5C10130353ABEB8586738938452"
)

$ErrorActionPreference = 'Stop'

if ([string]::IsNullOrWhiteSpace($Url) -or [string]::IsNullOrWhiteSpace($Sha256)) {
    throw "Both Url and Sha256 parameters must be provided"
}

if (-not (Get-Command 7z -ErrorAction SilentlyContinue)) {
    throw "7z was not found on PATH. Please install 7-Zip and ensure 7z is accessible"
}

$root = Split-Path -Parent $PSScriptRoot
$lib  = Join-Path $root 'native/mpv/lib'
$tmp  = Join-Path $env:TEMP 'ansimuz-mpv'

New-Item -ItemType Directory -Force $lib, $tmp | Out-Null

$archive = Join-Path $tmp 'mpv-dev.7z'
Write-Host "Downloading libmpv dev archive from $Url"
Invoke-WebRequest $Url -OutFile $archive

$actual = (Get-FileHash $archive -Algorithm SHA256).Hash
if ($actual -ne $Sha256.ToUpper()) {
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
    throw "SHA256 mismatch: expected $($Sha256.ToUpper()), got $actual"
}

Write-Host "Extracting archive..."
7z x $archive "-o$tmp" -y | Out-Null

# Copy runtime DLL files to the native lib directory
Copy-Item (Join-Path $tmp '*.dll') $lib -Force

# Locate the primary mpv dynamic library
$dllItem = Get-ChildItem $lib -Filter '*mpv*.dll' | Select-Object -First 1
if (-not $dllItem) {
    throw "Could not find mpv dll in extracted files"
}
$dll = $dllItem.Name
$def = Join-Path $lib 'mpv.def'
$out = Join-Path $lib 'mpv.lib'

# Copy def file if provided by archive or generate it via exports
if (Test-Path (Join-Path $tmp 'mpv.def')) {
    Copy-Item (Join-Path $tmp 'mpv.def') $lib -Force
} elseif (-not (Test-Path $def)) {
    Write-Host "Generating mpv.def module definition from $dll exports..."
    $dumpbinCmd = Get-Command dumpbin -ErrorAction SilentlyContinue
    if (-not $dumpbinCmd) {
        $dumpbinExe = Get-ChildItem "C:\Program Files*\Microsoft Visual Studio\*\*\VC\Tools\MSVC\*\bin\Hostx64\x64\dumpbin.exe" -ErrorAction SilentlyContinue |
            Select-Object -First 1 -ExpandProperty FullName
    } else {
        $dumpbinExe = $dumpbinCmd.Source
    }

    if ($dumpbinExe) {
        $dllPath = Join-Path $lib $dll
        $exports = & $dumpbinExe /exports $dllPath |
            Where-Object { $_ -match '^\s+\d+\s+[0-9A-Fa-f]+\s+[0-9A-Fa-f]+\s+(\S+)$' } |
            ForEach-Object { $matches[1] }
        $defLines = @("LIBRARY `"$dll`"", "EXPORTS") + $exports
        Set-Content -Path $def -Value $defLines -Encoding Ascii
    } else {
        throw "mpv.def not present and dumpbin is not available to generate it"
    }
}

# Locate MSVC lib tool to create the import library
$libCmd = Get-Command lib -ErrorAction SilentlyContinue
if (-not $libCmd) {
    $libExe = Get-ChildItem "C:\Program Files*\Microsoft Visual Studio\*\*\VC\Tools\MSVC\*\bin\Hostx64\x64\lib.exe" -ErrorAction SilentlyContinue |
        Select-Object -First 1 -ExpandProperty FullName
} else {
    $libExe = $libCmd.Source
}

if (-not $libExe) {
    throw "MSVC lib.exe not found. Please run in an x64 Native Tools prompt or add MSVC tools to PATH"
}

Write-Host "Generating import library $out..."
& $libExe /def:$def /name:$dll /out:$out /MACHINE:X64 | Out-Null

# Clean up temporary archive directory
Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue

Write-Host "libmpv ready in $lib ($dll)"

