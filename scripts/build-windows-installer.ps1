param(
    [Parameter(Mandatory=$true)][string]$Binary,
    [string]$OutputDirectory = 'dist',
    [string]$Compiler = $env:ISCC_PATH
)
$ErrorActionPreference = 'Stop'
$Binary = (Resolve-Path -LiteralPath $Binary).Path
$root = Split-Path $PSScriptRoot -Parent
if (-not $Compiler) {
    $command = Get-Command ISCC.exe -ErrorAction SilentlyContinue
    if ($command) { $Compiler = $command.Source }
    else { $Compiler = "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe" }
}
if (-not (Test-Path -LiteralPath $Compiler)) { throw 'Inno Setup 6.3+ is required. Set ISCC_PATH to ISCC.exe.' }
$metadata = cargo metadata --manifest-path "$root/Cargo.toml" --no-deps --format-version 1 | ConvertFrom-Json
if ($LASTEXITCODE -ne 0) { throw 'Could not read the app version' }
$version = ($metadata.packages | Where-Object name -eq 'lennette').version
if ($version -notmatch '^\d+\.\d+\.\d+$') { throw "Unsupported installer version: $version" }
New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
$output = (Resolve-Path -LiteralPath $OutputDirectory).Path
& $Compiler "/DAppVersion=$version" "/DSourceBinary=$Binary" "/DOutputDirectory=$output" "$root/packaging/windows/lennette.iss"
if ($LASTEXITCODE -ne 0) { throw "Installer compilation failed ($LASTEXITCODE)" }
$setup = Join-Path $output 'lennette-windows-x86_64-setup.exe'
if (-not (Test-Path -LiteralPath $setup)) { throw 'Installer output is missing' }
Write-Output "Built $setup"
