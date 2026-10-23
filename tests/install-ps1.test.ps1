# Run with Windows PowerShell 5.1 to exercise .cmd dispatch and a real test EXE.
# PowerShell on Linux can also exercise the setup/manager using a shell fixture.
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$installer = Join-Path $root 'web/public/install.ps1'
$nativeWindows = $env:OS -eq 'Windows_NT'
$shell = (Get-Process -Id $PID).Path
$work = Join-Path ([IO.Path]::GetTempPath()) ('lennette-test-' + [guid]::NewGuid().ToString('N'))
$testHome = Join-Path $work "User's home"
$mirror = Join-Path $work 'releases'
$bin = Join-Path $testHome 'bin'
$manager = Join-Path $bin 'lenny-launch.ps1'
$utf8 = New-Object Text.UTF8Encoding($false)
New-Item -ItemType Directory -Force -Path $work | Out-Null
$original = @{}
foreach ($name in 'LENNETTE_HOME', 'LENNETTE_RELEASES_URL', 'LENNETTE_INSTALL_URL_PS1', 'LENNETTE_TEST_LAUNCH_LOG', 'OS', 'PROCESSOR_ARCHITECTURE', 'PROCESSOR_ARCHITEW6432') {
    $original[$name] = [Environment]::GetEnvironmentVariable($name)
}
function Check([string]$What, [bool]$Passed) {
    if (-not $Passed) { throw "FAIL: $What" }
    Write-Host "ok: $What"
}
function Make-Release([string]$Tag) {
    $directory = Join-Path $mirror "download/$Tag"
    New-Item -ItemType Directory -Force -Path $directory | Out-Null
    $exe = Join-Path $directory 'lennette-windows-x86_64.exe'
    Copy-Item -LiteralPath (Join-Path $work 'fixture.exe') -Destination $exe -Force
    $hash = (Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash.ToLowerInvariant()
    [IO.File]::WriteAllText((Join-Path $directory 'SHA256SUMS-windows-x86_64.txt'), "$hash  lennette-windows-x86_64.exe`n", $utf8)
}
function Run-Setup([string[]]$InstallArgs = @()) {
    $ErrorActionPreference = 'Continue'
    $output = & $shell -NoProfile -NonInteractive -ExecutionPolicy Bypass -File $installer @InstallArgs 2>&1
    $script:code = $LASTEXITCODE
    return ($output -join "`n")
}
function Run-Lenny([string[]]$CommandArgs = @()) {
    $ErrorActionPreference = 'Continue'
    if ($nativeWindows) { $output = & (Join-Path $bin 'lenny.cmd') @CommandArgs 2>&1 }
    else { $output = & $shell -NoProfile -NonInteractive -File $manager @CommandArgs 2>&1 }
    $script:code = $LASTEXITCODE
    return ($output -join "`n")
}
try {
    $env:LENNETTE_HOME = $testHome
    $env:LENNETTE_RELEASES_URL = $mirror
    $env:LENNETTE_INSTALL_URL_PS1 = $installer
    $env:LENNETTE_TEST_LAUNCH_LOG = Join-Path $work 'launched'
    $env:OS = 'Windows_NT'
    $env:PROCESSOR_ARCHITECTURE = 'AMD64'
    $env:PROCESSOR_ARCHITEW6432 = ''
    if ($nativeWindows) {
        Add-Type -OutputAssembly (Join-Path $work 'fixture.exe') -OutputType ConsoleApplication -TypeDefinition @'
using System;
using System.IO;
public class Fixture {
    public static int Main(string[] args) {
        if (args.Length > 0 && args[0] == "--version") { Console.WriteLine("lennette test"); return 0; }
        File.WriteAllLines(Environment.GetEnvironmentVariable("LENNETTE_TEST_LAUNCH_LOG"), args);
        return 0;
    }
}
'@
    } else {
        [IO.File]::WriteAllText((Join-Path $work 'fixture.exe'), @'
#!/bin/sh
if [ "${1:-}" = --version ]; then echo 'lennette test'; exit 0; fi
printf '%s\n' "$@" >"$LENNETTE_TEST_LAUNCH_LOG"
'@ + "`n", $utf8)
        & chmod +x (Join-Path $work 'fixture.exe')
    }
    Make-Release 'v0.0.0-test'
    $manifest = Join-Path $mirror 'download/v0.0.0-test/SHA256SUMS-windows-x86_64.txt'
    [IO.File]::WriteAllText($manifest, (('0' * 64) + "  lennette-windows-x86_64.exe`n"), $utf8)
    $output = Run-Setup @('-NoLaunch', '-NoModifyPath')
    Check 'tamper is rejected without a launcher' ($code -ne 0 -and $output.Contains('checksum mismatch') -and -not (Test-Path $manager))

    Make-Release 'v0.0.0-test'
    $output = Run-Setup @('-NoLaunch', '-NoModifyPath')
    Check 'native Windows setup succeeds' ($code -eq 0 -and (Test-Path (Join-Path $bin 'lenny.cmd')))
    Check 'there is no conflicting lenny.ps1 command' (-not (Test-Path (Join-Path $bin 'lenny.ps1')))
    Check '-NoLaunch does not open the app' (-not (Test-Path $env:LENNETTE_TEST_LAUNCH_LOG))
    $output = Run-Lenny @('--version')
    Check 'app flags are forwarded through the command' ($code -eq 0 -and $output.Contains('lennette test'))
    $output = Run-Lenny @('argument with spaces', "apostrophe's argument")
    Check 'arguments keep their spaces and apostrophes' ($code -eq 0 -and ([IO.File]::ReadAllLines($env:LENNETTE_TEST_LAUNCH_LOG) -join '|') -eq "argument with spaces|apostrophe's argument")
    Remove-Item $env:LENNETTE_TEST_LAUNCH_LOG
    $output = Run-Setup @('-NoModifyPath')
    for ($attempt = 0; $attempt -lt 30 -and -not (Test-Path $env:LENNETTE_TEST_LAUNCH_LOG); $attempt++) { Start-Sleep -Milliseconds 100 }
    Check 'setup opens the app immediately by default' ($code -eq 0 -and (Test-Path $env:LENNETTE_TEST_LAUNCH_LOG))
    $output = Run-Setup @('-NoLaunch', '-NoModifyPath')
    Check 'repeated setup reuses the cached release' ($code -eq 0 -and $output.Contains('Already installed'))

    Move-Item -LiteralPath $mirror -Destination (Join-Path $work 'offline')
    $output = Run-Lenny @('doctor')
    Check 'doctor works offline' ($code -eq 0 -and $output.Contains('lennette test'))
    $output = Run-Lenny @('--version')
    Check 'normal commands work offline' ($code -eq 0 -and $output.Contains('lennette test'))
    $output = Run-Lenny @('help')
    Check 'help works offline' ($code -eq 0 -and $output.Contains('lenny update'))
    Move-Item -LiteralPath (Join-Path $work 'offline') -Destination $mirror

    Make-Release 'v0.0.1-test'
    $manifest = Join-Path $mirror 'download/v0.0.1-test/SHA256SUMS-windows-x86_64.txt'
    [IO.File]::WriteAllText($manifest, (('0' * 64) + "  lennette-windows-x86_64.exe`n"), $utf8)
    $output = Run-Lenny @('update', '--no-modify-path')
    Check 'failed upgrade preserves the active release' ($code -ne 0 -and ([IO.File]::ReadAllText($manager)).Contains('v0.0.0-test'))
    Make-Release 'v0.0.1-test'
    Remove-Item $env:LENNETTE_TEST_LAUNCH_LOG
    $output = Run-Lenny @('update', '--no-modify-path')
    Check 'lenny update switches to the newest release' ($code -eq 0 -and ([IO.File]::ReadAllText($manager)).Contains('v0.0.1-test'))
    Check 'update does not open the app' (-not (Test-Path $env:LENNETTE_TEST_LAUNCH_LOG))
    $output = Run-Lenny @('update', '--version', 'v0.0.0-test', '--no-modify-path')
    Check 'the same version flag works on Windows' ($code -eq 0 -and ([IO.File]::ReadAllText($manager)).Contains('v0.0.0-test'))

    Remove-Item -LiteralPath (Join-Path $testHome 'versions') -Recurse -Force
    # Repair calls setup, which normally adds to user PATH. Preserve it on the
    # actual Windows runner; this test must never leave a temporary PATH entry.
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    try {
        $output = Run-Lenny @('--version')
        Check 'missing payload is repaired before forwarding the app flag' ($code -eq 0 -and $output.Contains('lennette test') -and ([IO.File]::ReadAllText($manager)).Contains('v0.0.1-test'))
    } finally {
        if ($nativeWindows) { [Environment]::SetEnvironmentVariable('Path', $userPath, 'User') }
    }
    $output = Run-Setup @('-Version', '../../escape', '-NoLaunch', '-NoModifyPath')
    Check 'unsafe release tags are rejected' ($code -ne 0)
    [IO.File]::WriteAllText((Join-Path $testHome 'settings-marker'), 'keep me')
    $output = Run-Lenny @('uninstall')
    Check 'uninstall removes the command and payloads' ($code -eq 0 -and -not (Test-Path $manager) -and -not (Test-Path (Join-Path $testHome 'versions')))
    Check 'uninstall keeps user data' (Test-Path (Join-Path $testHome 'settings-marker'))
    Write-Host 'Windows installer: all checks passed'
} finally {
    foreach ($name in $original.Keys) { [Environment]::SetEnvironmentVariable($name, $original[$name]) }
    Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
}
