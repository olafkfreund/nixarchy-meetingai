param(
    [Parameter(Mandatory=$true)][string]$Installer,
    [Parameter(Mandatory=$true)][string]$Binary
)
$ErrorActionPreference = 'Stop'
$Installer = (Resolve-Path -LiteralPath $Installer).Path
$Binary = (Resolve-Path -LiteralPath $Binary).Path
$registration = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\{72AF4F45-C286-4FB8-BB11-AD2C8C83985C}_is1'
if (Test-Path -LiteralPath $registration) { throw 'Refusing to replace an existing Lennette setup installation. Run this check in a clean Windows account.' }
$sandbox = Join-Path ([IO.Path]::GetTempPath()) ('lennette-setup-test-' + [guid]::NewGuid().ToString('N'))
$installDir = Join-Path $sandbox 'Installed App'
$group = 'Lennette setup test ' + [guid]::NewGuid().ToString('N')
$shortcut = Join-Path ([Environment]::GetFolderPath('StartMenu')) "Programs\$group\Lennette.lnk"
New-Item -ItemType Directory -Path $sandbox | Out-Null
$passed = $false
function Invoke-Checked([string]$File, [string[]]$Arguments) {
    $process = Start-Process -FilePath $File -ArgumentList $Arguments -WindowStyle Hidden -PassThru
    if (-not $process.WaitForExit(120000)) { $process.Kill(); throw "Timed out: $File" }
    if ($process.ExitCode -ne 0) { throw "Process failed ($($process.ExitCode)): $File" }
}
try {
    $arguments = @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/SP-', "/DIR=`"$installDir`"", "/GROUP=`"$group`"", '/TASKS=')
    Invoke-Checked $Installer ($arguments + "/LOG=`"$sandbox\install.log`"")
    $installed = Join-Path $installDir 'lennette.exe'
    if ((Get-FileHash -LiteralPath $installed).Hash -ne (Get-FileHash -LiteralPath $Binary).Hash) { throw 'Installed binary differs from the packaged binary' }
    Invoke-Checked $installed @('--version')
    if (-not (Test-Path -LiteralPath $shortcut)) { throw 'Start menu shortcut was not installed' }
    $shell = New-Object -ComObject WScript.Shell
    if ($shell.CreateShortcut($shortcut).TargetPath -ne $installed) { throw 'Start menu shortcut points at the wrong binary' }
    if (-not (Test-Path -LiteralPath $registration)) { throw 'Per-user uninstall registration is missing' }
    if ((Get-ItemProperty -LiteralPath $registration).InstallLocation.TrimEnd('\') -ne $installDir) { throw 'Installation was not isolated to the test directory' }
    # Repeat setup in place, preserving unrelated user files.
    $sentinel = Join-Path $installDir 'keep-user-data.txt'
    Set-Content -LiteralPath $sentinel -Value 'preserve'
    Invoke-Checked $Installer ($arguments + "/LOG=`"$sandbox\upgrade.log`"")
    if ((Get-Content -LiteralPath $sentinel) -ne 'preserve') { throw 'Upgrade removed user files' }
    if ((Get-FileHash -LiteralPath $installed).Hash -ne (Get-FileHash -LiteralPath $Binary).Hash) { throw 'Upgrade changed the installed payload' }
    Invoke-Checked (Join-Path $installDir 'unins000.exe') @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', "/LOG=`"$sandbox\uninstall.log`"")
    $deadline = [DateTime]::UtcNow.AddSeconds(20)
    while ((Test-Path -LiteralPath $installed) -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 100 }
    if (Test-Path -LiteralPath $installed) { throw 'Uninstall left the app binary behind' }
    if (Test-Path -LiteralPath $shortcut) { throw 'Uninstall left the shortcut behind' }
    if (Test-Path -LiteralPath $registration) { throw 'Uninstall left its registration behind' }
    if ((Get-Content -LiteralPath $sentinel) -ne 'preserve') { throw 'Uninstall removed user files' }
    $passed = $true
} finally {
    $uninstaller = Join-Path $installDir 'unins000.exe'
    if (-not $passed -and (Test-Path -LiteralPath $uninstaller)) {
        Invoke-Checked $uninstaller @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART')
    }
    # Only remove the unique directory created by this test, never the app's data.
    $tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\') + '\'
    $resolved = [IO.Path]::GetFullPath($sandbox)
    if (-not $resolved.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase) -or (Split-Path $resolved -Leaf) -notlike 'lennette-setup-test-*') { throw 'Unsafe test cleanup path' }
    if ($passed) {
        $deadline = [DateTime]::UtcNow.AddSeconds(20)
        # Inno's detached cleanup process can briefly retain the uninstall log
        # after the payload and registration have disappeared.
        while (Test-Path -LiteralPath $resolved) {
            try { Remove-Item -LiteralPath $resolved -Recurse -Force; break }
            catch {
                if ([DateTime]::UtcNow -ge $deadline) { throw }
                Start-Sleep -Milliseconds 100
            }
        }
    }
    else { Write-Warning "Installer diagnostics preserved in $resolved" }
}
Write-Output 'PASS: per-user setup installs the exact binary, creates a working shortcut, upgrades in place and uninstalls while preserving user files.'
