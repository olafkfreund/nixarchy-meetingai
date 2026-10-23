param(
    [Parameter(Mandatory=$true)][string]$Binary,
    [ValidateRange(1,10)][int]$Runs = 3,
    [string]$OutputPath
)
$ErrorActionPreference = 'Stop'
$Binary = (Resolve-Path -LiteralPath $Binary).Path
if (Get-Process -Name lennette -ErrorAction SilentlyContinue) { throw 'Close Lennette before benchmarking so its session is not changed.' }
Add-Type @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class LennetteStartup {
    delegate bool Callback(IntPtr hwnd, IntPtr unused);
    [DllImport("user32.dll")] static extern bool EnumWindows(Callback callback, IntPtr unused);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint process);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetWindowText(IntPtr hwnd, StringBuilder text, int size);
    [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr hwnd);
    [DllImport("user32.dll")] static extern bool GetWindowDisplayAffinity(IntPtr hwnd, out uint affinity);
    public static bool Ready(uint process) {
        bool ready = false;
        EnumWindows((window, unused) => {
            uint owner, affinity;
            GetWindowThreadProcessId(window, out owner);
            if (owner != process || !IsWindowVisible(window)) return true;
            var title = new StringBuilder(128); GetWindowText(window, title, 128);
            if (title.ToString() == "lennette" && GetWindowDisplayAffinity(window, out affinity) && affinity == 0x11) ready = true;
            return !ready;
        }, IntPtr.Zero);
        return ready;
    }
}
'@
$samples = @()
foreach ($mode in @('privacy', 'normal')) {
    for ($run = 1; $run -le $Runs; $run++) {
        $app = $null
        try {
            $timer = [Diagnostics.Stopwatch]::StartNew()
            $app = Start-Process -FilePath $Binary -ArgumentList "--$mode" -WindowStyle Hidden -PassThru
            while (-not [LennetteStartup]::Ready($app.Id)) {
                if ($app.HasExited) { throw 'App exited or another instance intercepted the benchmark. Close Lennette first.' }
                if ($timer.Elapsed.TotalSeconds -gt 20) { throw 'Protected overlay did not become visible within 20 seconds.' }
                Start-Sleep -Milliseconds 20
            }
            $visibleMs = $timer.Elapsed.TotalMilliseconds
            Start-Sleep -Milliseconds 1000
            $app.Refresh()
            $cpu = $app.TotalProcessorTime.TotalMilliseconds
            Start-Sleep -Milliseconds 500
            $app.Refresh()
            $samples += [pscustomobject]@{
                mode = $mode
                run = $run
                protected_window_visible_ms = [math]::Round($visibleMs, 1)
                working_set_mib = [math]::Round($app.WorkingSet64 / 1MB, 1)
                idle_cpu_ms_per_500ms = [math]::Round($app.TotalProcessorTime.TotalMilliseconds - $cpu, 1)
                threads = $app.Threads.Count
            }
        } finally {
            if ($app -and -not $app.HasExited) { Stop-Process -Id $app.Id; $app.WaitForExit() }
        }
    }
}
if ($OutputPath) {
    [IO.File]::WriteAllText([IO.Path]::GetFullPath($OutputPath), ($samples | ConvertTo-Json), (New-Object Text.UTF8Encoding($false)))
}
$samples | Format-Table -AutoSize
Write-Output 'Timing includes process launch until an OS-excluded overlay is visible. It does not measure first painted pixels or a cold disk cache. Memory excludes GPU allocations; idle CPU is one 500 ms sample.'
