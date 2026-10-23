# Native Windows setup: irm https://lennette.itssingularity.com/install.ps1 | iex
# After setup: lenny [update|doctor|uninstall]. PowerShell 5.1+, no Node.js.
[CmdletBinding()]
param(
    [string]$Version = 'latest',
    [switch]$NoLaunch,
    [switch]$NoModifyPath,
    [switch]$Uninstall
)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
$repo = if ($env:LENNETTE_REPO) { $env:LENNETTE_REPO } else { 'parazeeknova/lennette' }
$releases = if ($env:LENNETTE_RELEASES_URL) { $env:LENNETTE_RELEASES_URL.TrimEnd('/') } else { "https://github.com/$repo/releases" }
$site = if ($env:LENNETTE_SITE_URL) { $env:LENNETTE_SITE_URL.TrimEnd('/') } else { 'https://lennette.itssingularity.com' }
$selfUrl = if ($env:LENNETTE_INSTALL_URL_PS1) { $env:LENNETTE_INSTALL_URL_PS1 } else { "$site/install.ps1" }
$dataDir = if ($env:LENNETTE_HOME) { $env:LENNETTE_HOME } else { Join-Path $env:LOCALAPPDATA 'lennette' }
$binDir = Join-Path $dataDir 'bin'
$temp = $null

function Get-Download([string]$Uri, [string]$Destination) {
    if ([IO.Path]::IsPathRooted($Uri)) { Copy-Item -LiteralPath $Uri -Destination $Destination -Force }
    elseif ($Uri.StartsWith('https://')) { Invoke-WebRequest -UseBasicParsing -TimeoutSec 300 -Uri $Uri -OutFile $Destination }
    else { throw 'Downloads must use HTTPS or an absolute local mirror path.' }
}

try {
    if ($Uninstall) {
        $manager = Join-Path $binDir 'lenny-launch.ps1'
        if (Test-Path -LiteralPath $manager) { & $manager uninstall }
        else { Write-Host 'Lennette is not installed.' }
        return
    }
    if ($env:OS -ne 'Windows_NT') { throw 'Use the Bash setup command on Linux. macOS is not supported yet.' }
    $arch = if ($env:PROCESSOR_ARCHITEW6432) { $env:PROCESSOR_ARCHITEW6432 } else { $env:PROCESSOR_ARCHITECTURE }
    if ($arch -ne 'AMD64') { throw "Only Windows x86_64 releases are available (this machine: $arch)." }
    if ($Version -eq 'latest') {
        if ([IO.Path]::IsPathRooted($releases)) {
            $latest = Get-ChildItem -LiteralPath (Join-Path $releases 'download') -Directory |
                Sort-Object { [version]($_.Name -replace '^v([0-9]+\.[0-9]+\.[0-9]+).*$', '$1') }, Name -Descending |
                Select-Object -First 1
            $Version = $latest.Name
        } else {
            $response = Invoke-WebRequest -UseBasicParsing -Method Head -TimeoutSec 30 -Uri "$releases/latest"
            $url = if ($response.BaseResponse.ResponseUri) { $response.BaseResponse.ResponseUri.AbsoluteUri } else { $response.BaseResponse.RequestMessage.RequestUri.AbsoluteUri }
            if (-not $url.StartsWith("$releases/tag/")) { throw 'No published release was found.' }
            $Version = $url.Substring($url.LastIndexOf('/') + 1)
        }
    }
    if ($Version -cnotmatch '^v[0-9A-Za-z][0-9A-Za-z._-]*$') { throw 'Expected a published release tag beginning with v.' }
    $payloadDir = Join-Path $dataDir "versions\$Version\windows-x86_64"
    $exe = Join-Path $payloadDir 'lennette.exe'
    $ready = $false
    if (Test-Path -LiteralPath $exe) {
        & $exe --version *> $null
        $ready = $LASTEXITCODE -eq 0
    }
    if ($ready) { Write-Host "Already installed: $Version" }
    else {
        $temp = Join-Path ([IO.Path]::GetTempPath()) ("lennette-" + [guid]::NewGuid().ToString('N'))
        New-Item -ItemType Directory -Path $temp | Out-Null
        Write-Host "Downloading Lennette $Version for Windows x86_64…"
        $download = Join-Path $temp 'lennette.exe'
        $manifest = Join-Path $temp 'SHA256SUMS-windows-x86_64.txt'
        Get-Download "$releases/download/$Version/SHA256SUMS-windows-x86_64.txt" $manifest
        Get-Download "$releases/download/$Version/lennette-windows-x86_64.exe" $download
        $match = [regex]::Match([IO.File]::ReadAllText($manifest), '(?m)^([0-9a-fA-F]{64})\s+\*?lennette-windows-x86_64\.exe\s*$')
        if (-not $match.Success) { throw 'The executable is missing from the checksum manifest.' }
        $hash = (Get-FileHash -LiteralPath $download -Algorithm SHA256).Hash
        if ($hash -ne $match.Groups[1].Value) { throw 'checksum mismatch; the existing installation was left alone.' }
        & $download --version
        if ($LASTEXITCODE -ne 0) { throw 'This build could not start. The existing installation was left alone.' }
        New-Item -ItemType Directory -Force -Path $payloadDir | Out-Null
        Move-Item -LiteralPath $download -Destination $exe -Force
    }

    # Only lenny.cmd is exposed as a command. A sibling lenny.ps1 would shadow
    # it in PowerShell and break on machines with restricted execution policy.
    New-Item -ItemType Directory -Force -Path $binDir | Out-Null
    $manager = @'
$ErrorActionPreference = 'Stop'
$data = '@DATA@'
$bin = Join-Path $data 'bin'
$native = '@EXE@'
$installer = '@INSTALLER@'
$releases = '@RELEASES@'
$arguments = @($args)
function Invoke-Setup([string[]]$InstallArgs) {
    $env:LENNETTE_HOME = $data
    $env:LENNETTE_RELEASES_URL = $releases
    $env:LENNETTE_INSTALL_URL_PS1 = $installer
    $source = if ([IO.Path]::IsPathRooted($installer)) { [IO.File]::ReadAllText($installer) } else {
        [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
        (Invoke-WebRequest -UseBasicParsing -TimeoutSec 60 -Uri $installer).Content
    }
    $setup = [scriptblock]::Create($source)
    # Translate the same public flags on both platforms to native parameters.
    $options = @{ NoLaunch = $true }
    for ($i = 0; $i -lt $InstallArgs.Count; $i++) {
        switch ($InstallArgs[$i]) {
            '--version' {
                if (++$i -ge $InstallArgs.Count) { throw '--version needs a release tag' }
                $options.Version = $InstallArgs[$i]
            }
            '--no-modify-path' { $options.NoModifyPath = $true }
            default { throw "Unknown update option: $($InstallArgs[$i])" }
        }
    }
    & $setup @options
}
try {
    $command = if ($arguments.Count) { $arguments[0] } else { '' }
    switch ($command) {
        'update' {
            $remaining = @($arguments | Select-Object -Skip 1)
            Invoke-Setup $remaining
            exit 0
        }
        'doctor' {
            Write-Host "Platform: Windows $env:PROCESSOR_ARCHITECTURE`nExecutable: $native"
            if (-not (Test-Path -LiteralPath $native)) { throw 'App missing. Run lenny update to repair it.' }
            & $native --version
            if ($LASTEXITCODE -ne 0) { throw 'App failed to start. Close Lennette and run lenny update.' }
            exit 0
        }
        'uninstall' {
            if ($arguments.Count -ne 1) { throw 'Usage: lenny uninstall' }
            # Remove payloads first: Windows locks a running EXE. If it fails,
            # retain the command so the user can close the app and retry.
            $versions = Join-Path $data 'versions'
            if (Test-Path -LiteralPath $versions) { Remove-Item -LiteralPath $versions -Recurse -Force }
            foreach ($name in 'lenny.cmd', 'lenny-launch.ps1') {
                Remove-Item -LiteralPath (Join-Path $bin $name) -Force -ErrorAction SilentlyContinue
            }
            $path = [Environment]::GetEnvironmentVariable('Path', 'User')
            $entries = @($path -split ';' | Where-Object { $_.TrimEnd('\') -ine $bin.TrimEnd('\') })
            $updated = $entries -join ';'
            if ($updated -cne $path) { [Environment]::SetEnvironmentVariable('Path', $updated, 'User') }
            Write-Host 'Lennette removed. Your settings, history, and API keys were kept.'
            exit 0
        }
        { $_ -in 'help', '-h', '--help' } {
            Write-Host "lenny - launch Lennette`n  lenny update [--version <tag>]`n  lenny doctor`n  lenny uninstall`nApp options such as --version and --toggle-dashboard are forwarded."
            exit 0
        }
    }
    if (-not (Test-Path -LiteralPath $native)) {
        Write-Host 'lenny: repairing the missing app…'
        Invoke-Setup @()
        # The new launcher can point to a different release.
        & (Join-Path $bin 'lenny-launch.ps1') @arguments
        exit $LASTEXITCODE
    }
    if ($arguments.Count) { & $native @arguments; exit $LASTEXITCODE }
    Start-Process -FilePath $native | Out-Null
} catch {
    [Console]::Error.WriteLine("lenny: $($_.Exception.Message)")
    exit 1
}
'@
    $manager = $manager.Replace('@DATA@', $dataDir.Replace("'", "''")).Replace('@EXE@', $exe.Replace("'", "''")).Replace('@INSTALLER@', $selfUrl.Replace("'", "''")).Replace('@RELEASES@', $releases.Replace("'", "''"))
    # UTF-8 BOM lets Windows PowerShell 5.1 read non-ASCII user paths correctly.
    [IO.File]::WriteAllText((Join-Path $binDir 'lenny-launch.ps1'), $manager, (New-Object Text.UTF8Encoding($true)))
    $cmd = "@echo off`r`npowershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File `"%~dp0lenny-launch.ps1`" %*`r`nexit /b %errorlevel%`r`n"
    [IO.File]::WriteAllText((Join-Path $binDir 'lenny.cmd'), $cmd, [Text.Encoding]::ASCII)
    # Remove obsolete wrappers from the previous installer, including the .ps1
    # that PowerShell would otherwise choose ahead of our single .cmd command.
    foreach ($name in 'lenny', 'lenny.ps1', 'lennette.exe') {
        Remove-Item -LiteralPath (Join-Path $binDir $name) -Force -ErrorAction SilentlyContinue
    }
    if (-not $NoModifyPath) {
        $current = [Environment]::GetEnvironmentVariable('Path', 'User')
        if (-not (@($current -split ';') | Where-Object { $_.TrimEnd('\') -ieq $binDir.TrimEnd('\') })) {
            [Environment]::SetEnvironmentVariable('Path', "$binDir;$current", 'User')
        }
        if (-not (($env:Path -split ';') -contains $binDir)) { $env:Path = "$binDir;$env:Path" }
    }
    Write-Host 'Ready. From now on, run: lenny'
    if (-not $NoLaunch) { Start-Process -FilePath $exe | Out-Null }
} catch {
    [Console]::Error.WriteLine("lenny: $($_.Exception.Message)")
    # Do not pause: the caller may be an automated update or piped setup.
    exit 1
} finally {
    if ($temp) { Remove-Item -LiteralPath $temp -Recurse -Force -ErrorAction SilentlyContinue }
}
