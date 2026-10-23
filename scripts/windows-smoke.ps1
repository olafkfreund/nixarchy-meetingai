param([Parameter(Mandatory=$true)][string]$Binary)
$ErrorActionPreference = 'Stop'
$Binary = (Resolve-Path $Binary).Path
Add-Type @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class LennetteWindows {
    public delegate bool Callback(IntPtr hwnd, IntPtr unused);
    [DllImport("user32.dll")] static extern bool EnumWindows(Callback callback, IntPtr unused);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint process);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetWindowText(IntPtr hwnd, StringBuilder text, int size);
    [DllImport("user32.dll")] public static extern bool GetWindowDisplayAffinity(IntPtr hwnd, out uint affinity);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hwnd);
    [DllImport("user32.dll", EntryPoint="GetWindowLongPtrW")] public static extern IntPtr GetWindowLongPtr(IntPtr hwnd, int index);
    [DllImport("user32.dll", EntryPoint="GetClassLongPtrW")] static extern IntPtr GetClassLongPtr(IntPtr hwnd, int index);
    [DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr hwnd, uint attribute, out uint value, uint size);
    [DllImport("user32.dll")] static extern uint GetDpiForWindow(IntPtr hwnd);
    [DllImport("user32.dll")] static extern bool GetClientRect(IntPtr hwnd, out Rect rect);
    [DllImport("user32.dll")] static extern bool ClientToScreen(IntPtr hwnd, ref Point point);
    [DllImport("user32.dll")] static extern IntPtr SendMessage(IntPtr hwnd, uint message, IntPtr wparam, IntPtr lparam);
    [DllImport("user32.dll")] static extern void keybd_event(byte key, byte scan, uint flags, UIntPtr extra);
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left, Top, Right, Bottom; }
    [StructLayout(LayoutKind.Sequential)] struct Point { public int X, Y; }
    [StructLayout(LayoutKind.Sequential)] struct BitmapInfo {
        public uint Size; public int Width, Height; public ushort Planes, Bits;
        public uint Compression, ImageSize; public int XPels, YPels; public uint Used, Important;
    }
    [DllImport("user32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
    static extern IntPtr CreateWindowEx(uint exStyle, string className, string title, uint style,
        int x, int y, int width, int height, IntPtr parent, IntPtr menu, IntPtr instance, IntPtr unused);
    [DllImport("user32.dll")] public static extern bool DestroyWindow(IntPtr window);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr window, int command);
    [DllImport("user32.dll")] static extern bool UpdateWindow(IntPtr window);
    [DllImport("user32.dll")] static extern bool SetWindowPos(IntPtr window, IntPtr after, int x, int y, int width, int height, uint flags);
    [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr window, out Rect rect);
    [DllImport("user32.dll")] static extern IntPtr WindowFromPoint(Point point);
    [DllImport("user32.dll")] static extern int GetSystemMetrics(int metric);
    [DllImport("user32.dll")] static extern IntPtr GetDC(IntPtr window);
    [DllImport("user32.dll")] static extern int ReleaseDC(IntPtr window, IntPtr dc);
    [DllImport("gdi32.dll")] static extern IntPtr CreateCompatibleDC(IntPtr dc);
    [DllImport("gdi32.dll")] static extern IntPtr CreateCompatibleBitmap(IntPtr dc, int width, int height);
    [DllImport("gdi32.dll")] static extern IntPtr SelectObject(IntPtr dc, IntPtr value);
    [DllImport("gdi32.dll")] static extern bool DeleteDC(IntPtr dc);
    [DllImport("gdi32.dll")] static extern bool DeleteObject(IntPtr value);
    [DllImport("gdi32.dll")] static extern bool BitBlt(IntPtr dest, int x, int y, int width, int height, IntPtr source, int sx, int sy, uint operation);
    [DllImport("gdi32.dll")] static extern int GetDIBits(IntPtr dc, IntPtr bitmap, uint start, uint lines, byte[] pixels, ref BitmapInfo info, uint usage);
    public static Rect Desktop() {
        int x = GetSystemMetrics(76), y = GetSystemMetrics(77);
        return new Rect { Left=x, Top=y, Right=x+GetSystemMetrics(78), Bottom=y+GetSystemMetrics(79) };
    }
    public static IntPtr Fixture(bool white, Rect bounds) {
        // Built-in STATIC controls paint synchronously; no custom message loop is needed.
        var window = CreateWindowEx(0x88, "STATIC", "Lennette capture control", 0x90000000u | (white ? 6u : 4u),
            bounds.Left, bounds.Top, bounds.Right-bounds.Left, bounds.Bottom-bounds.Top,
            IntPtr.Zero, IntPtr.Zero, IntPtr.Zero, IntPtr.Zero);
        if (window == IntPtr.Zero) throw new InvalidOperationException("Cannot create capture control");
        UpdateWindow(window);
        return window;
    }
    public static Rect PlaceAbove(IntPtr window, uint process) {
        Rect desktop = Desktop(), original;
        if (!GetWindowRect(window, out original)) throw new InvalidOperationException("Cannot read app bounds");
        int width = Math.Min(original.Right-original.Left, desktop.Right-desktop.Left-80);
        int height = Math.Min(original.Bottom-original.Top, desktop.Bottom-desktop.Top-100);
        int x = desktop.Left+40, y = desktop.Top+40;
        // Promote the real GPUI window above our white backdrop, not underneath it.
        if (!SetWindowPos(window, new IntPtr(-1), x, y, width, height, 0x40))
            throw new InvalidOperationException("Cannot position app above capture control");
        GetWindowRect(window, out original);
        uint owner; GetWindowThreadProcessId(WindowFromPoint(new Point { X=x+width/2, Y=y+height/2 }), out owner);
        if (owner != process || !IsWindowVisible(window))
            throw new InvalidOperationException("App is not locally visible above the capture control");
        return new Rect { Left=original.Left+8, Top=original.Top+8, Right=original.Right-8, Bottom=original.Bottom-8 };
    }
    public static int NonWhitePixels(Rect area) {
        Rect bounds = Desktop();
        int width=bounds.Right-bounds.Left, height=bounds.Bottom-bounds.Top;
        IntPtr screen=GetDC(IntPtr.Zero), memory=IntPtr.Zero, bitmap=IntPtr.Zero, previous=IntPtr.Zero;
        try {
            memory=CreateCompatibleDC(screen); bitmap=CreateCompatibleBitmap(screen, width, height);
            if (screen == IntPtr.Zero || memory == IntPtr.Zero || bitmap == IntPtr.Zero)
                throw new InvalidOperationException("Cannot allocate desktop capture");
            previous=SelectObject(memory, bitmap);
            if (!BitBlt(memory, 0, 0, width, height, screen, bounds.Left, bounds.Top, 0x40cc0020))
                throw new InvalidOperationException("Full-desktop BitBlt failed");
            SelectObject(memory, previous); previous=IntPtr.Zero;
            var info=new BitmapInfo { Size=40, Width=width, Height=-height, Planes=1, Bits=32 };
            var pixels=new byte[checked(width*height*4)];
            if (GetDIBits(memory, bitmap, 0, (uint)height, pixels, ref info, 0) != height)
                throw new InvalidOperationException("Cannot read desktop pixels");
            int different=0;
            for (int y=area.Top; y<area.Bottom; y++) for (int x=area.Left; x<area.Right; x++) {
                int offset=((y-bounds.Top)*width+x-bounds.Left)*4;
                if (pixels[offset]!=255 || pixels[offset+1]!=255 || pixels[offset+2]!=255) different++;
            }
            return different;
        } finally {
            if (previous != IntPtr.Zero) SelectObject(memory, previous);
            if (bitmap != IntPtr.Zero) DeleteObject(bitmap);
            if (memory != IntPtr.Zero) DeleteDC(memory);
            if (screen != IntPtr.Zero) ReleaseDC(IntPtr.Zero, screen);
        }
    }
    public static IntPtr Find(uint process, string title) {
        IntPtr result = IntPtr.Zero;
        EnumWindows((window, unused) => {
            uint owner; GetWindowThreadProcessId(window, out owner);
            var text = new StringBuilder(256); GetWindowText(window, text, 256);
            if (owner == process && text.ToString() == title) result = window;
            return true;
        }, IntPtr.Zero);
        return result;
    }
    public static int HitTest(IntPtr window, int x, int y) {
        // Feed the app's mouse position before querying GPUI's rendered native
        // control hitboxes. This sends no real mouse input to the user's desktop.
        SendMessage(window, 0x200, IntPtr.Zero, new IntPtr((y << 16) | (x & 0xffff)));
        var point = new Point { X=x, Y=y };
        if (!ClientToScreen(window, ref point)) throw new InvalidOperationException("Cannot map pill coordinates");
        return SendMessage(window, 0x84, IntPtr.Zero, new IntPtr((point.Y << 16) | (point.X & 0xffff))).ToInt32();
    }
    public static void AssertPillDrag(IntPtr window) {
        Rect client;
        if (!GetClientRect(window, out client)) throw new InvalidOperationException("Cannot read client bounds");
        double scale = GetDpiForWindow(window)/96.0;
        int y=(int)(27*scale), first=-1;
        for (int x=(int)(20*scale); x<client.Right-(int)(20*scale); x++) {
            if (HitTest(window, x, y) == 2) { first=x; break; } // HTCAPTION
        }
        if (first < 0) throw new InvalidOperationException("Top pill has no native window drag region");
        // Pill padding + 18px grip + gap, then the 34px new-chat button.
        if (HitTest(window, first+(int)(49*scale), y) != 1)
            throw new InvalidOperationException("New-chat button was swallowed by the pill drag region");
        if (HitTest(window, first+(int)(90*scale), y) != 1)
            throw new InvalidOperationException("Model selector was swallowed by the pill drag region");
        if (HitTest(window, client.Right/2, (int)(120*scale)) != 1)
            throw new InvalidOperationException("Chat content became a window drag region");
    }
    public static void AssertDashboardDrag(IntPtr window) {
        Rect client;
        if (!GetClientRect(window, out client)) throw new InvalidOperationException("Cannot read dashboard bounds");
        double scale=GetDpiForWindow(window)/96.0;
        int y=(int)(27*scale);
        if (HitTest(window, (int)(35*scale), y) != 2 || HitTest(window, client.Right-(int)(72*scale), y) != 2)
            throw new InvalidOperationException("Dashboard logo/empty header space is not draggable");
        if (HitTest(window, client.Right-(int)(35*scale), y) != 1)
            throw new InvalidOperationException("Dashboard close button became a drag region");
    }
    public static void AssertIcon(IntPtr window) {
        if (GetClassLongPtr(window, -14) == IntPtr.Zero)
            throw new InvalidOperationException("The application class has no branded icon resource");
    }
    public static void ToggleHotkey() {
        keybd_event(0x11, 0, 0, UIntPtr.Zero); // Ctrl
        keybd_event(0x10, 0, 0, UIntPtr.Zero); // Shift
        keybd_event(0xdc, 0, 0, UIntPtr.Zero); // OEM backslash
        keybd_event(0xdc, 0, 2, UIntPtr.Zero);
        keybd_event(0x10, 0, 2, UIntPtr.Zero);
        keybd_event(0x11, 0, 2, UIntPtr.Zero);
    }
}
'@
function Wait-For([scriptblock]$Condition, [string]$Description) {
    $deadline = [DateTime]::UtcNow.AddSeconds(20)
    do {
        if ($app.HasExited) { throw "App exited during $Description (exit $($app.ExitCode))" }
        if (& $Condition) { return }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "Timed out: $Description"
}
function Send-Command([string]$Flag) {
    $command = Start-Process -FilePath $Binary -ArgumentList $Flag -Wait -PassThru
    if ($command.ExitCode -ne 0) { throw "Command failed: $Flag" }
    Start-Sleep -Milliseconds 350
}
function Assert-Protection([IntPtr]$Window) {
    [uint32]$affinity = 0
    if (-not [LennetteWindows]::GetWindowDisplayAffinity($Window, [ref]$affinity) -or $affinity -ne 0x11) {
        throw "The actual GPUI window is not excluded from capture (stage=$script:stage handle=$Window affinity=$affinity)"
    }
    $style = [LennetteWindows]::GetWindowLongPtr($Window, -20).ToInt64()
    if (($style -band 0x80) -eq 0 -or ($style -band 0x40000) -ne 0) { throw 'Taskbar/Alt+Tab styling is missing' }
    [uint32]$border = 0
    # Windows 10 does not implement these Windows 11 attributes.
    if ([LennetteWindows]::DwmGetWindowAttribute($Window, 34, [ref]$border, 4) -eq 0 -and $border -ne 0xfffffffeL) { throw 'Native square window border is still enabled' }
    [uint32]$corners = 0
    if ([LennetteWindows]::DwmGetWindowAttribute($Window, 33, [ref]$corners, 4) -eq 0 -and $corners -ne 1) { throw 'Windows is clipping the app with a competing corner radius' }
    [uint32]$nativeFrame = 0
    if ([LennetteWindows]::DwmGetWindowAttribute($Window, 1, [ref]$nativeFrame, 4) -ne 0 -or $nativeFrame -ne 0) { throw 'Native frame material is interfering with app transparency' }
    [LennetteWindows]::AssertIcon($Window)
}
$previousPrivate = $env:LENNETTE_PRIVATE_OVERLAY
try {
    $script:stage = 'startup'
    $diagnostics = Join-Path ([IO.Path]::GetTempPath()) ('lennette-native-' + [guid]::NewGuid().ToString('N'))
    # Explicit privacy launch must omit shell-owned icons.
    $env:LENNETTE_PRIVATE_OVERLAY = $null
    $app = Start-Process -FilePath $Binary -ArgumentList '--privacy' -WindowStyle Hidden -RedirectStandardOutput "$diagnostics.out" -RedirectStandardError "$diagnostics.err" -PassThru
    Wait-For { $script:overlay = [LennetteWindows]::Find($app.Id, 'lennette'); $overlay -ne [IntPtr]::Zero -and [LennetteWindows]::IsWindowVisible($overlay) } 'protected overlay startup'
    Assert-Protection $overlay
    Start-Sleep -Milliseconds 500
    [LennetteWindows]::AssertPillDrag($overlay)
    if ([LennetteWindows]::Find($app.Id, 'lennette tray') -ne [IntPtr]::Zero) { throw 'Privacy launch created an Explorer tray icon' }
    # Exercise RegisterHotKey, not just the one-shot control flags.
    [LennetteWindows]::ToggleHotkey()
    Wait-For { -not [LennetteWindows]::IsWindowVisible($overlay) } 'hotkey hide'
    Send-Command '--focus-input'
    Send-Command '--toggle-dashboard'
    if ([LennetteWindows]::IsWindowVisible($overlay)) { throw 'Hidden focus shortcut restored the app' }
    if ([LennetteWindows]::Find($app.Id, 'lennette dashboard') -ne [IntPtr]::Zero) { throw 'Hidden dashboard shortcut ran' }
    [LennetteWindows]::ToggleHotkey()
    Wait-For { [LennetteWindows]::IsWindowVisible($overlay) } 'hotkey restore'
    $script:stage = 'hotkey restore'
    Assert-Protection $overlay
    Send-Command '--toggle-dashboard'
    $script:stage = 'dashboard startup'
    Wait-For { $script:dashboard = [LennetteWindows]::Find($app.Id, 'lennette dashboard'); $dashboard -ne [IntPtr]::Zero -and [LennetteWindows]::IsWindowVisible($dashboard) } 'dashboard after restore'
    Assert-Protection $dashboard
    Start-Sleep -Milliseconds 300
    [LennetteWindows]::AssertDashboardDrag($dashboard)
    # Capture the whole desktop with a visible, unprotected black positive control.
    # A white backdrop makes even a black-rectangle fallback fail this check.
    [LennetteWindows]::ShowWindow($overlay, 0) | Out-Null
    [LennetteWindows]::ShowWindow($dashboard, 0) | Out-Null
    $desktop = [LennetteWindows]::Desktop()
    $backdrop = [LennetteWindows]::Fixture($true, $desktop)
    $controlBounds = [LennetteWindows+Rect]@{ Left=$desktop.Left+20; Top=$desktop.Top+20; Right=$desktop.Left+120; Bottom=$desktop.Top+120 }
    $control = [LennetteWindows]::Fixture($false, $controlBounds)
    Start-Sleep -Milliseconds 500
    if ([LennetteWindows]::NonWhitePixels($controlBounds) -lt 9000) { throw 'Desktop capture missed the visible positive control' }
    [LennetteWindows]::DestroyWindow($control) | Out-Null
    $control = [IntPtr]::Zero
    Start-Sleep -Milliseconds 300
    if ([LennetteWindows]::NonWhitePixels($controlBounds) -ne 0) { throw 'Capture backdrop is not stable' }
    foreach ($window in @($overlay, $dashboard)) {
        $script:stage = 'desktop capture'
        $area = [LennetteWindows]::PlaceAbove($window, $app.Id)
        Assert-Protection $window
        Start-Sleep -Milliseconds 700
        if ([LennetteWindows]::NonWhitePixels($area) -ne 0) { throw 'Protected app pixels or a black fallback leaked into the full-desktop capture' }
        [LennetteWindows]::ShowWindow($window, 0) | Out-Null
    }
    Send-Command '--normal'
    $script:stage = 'normal/privacy switching'
    Wait-For { [LennetteWindows]::Find($app.Id, 'lennette tray') -ne [IntPtr]::Zero } 'normal-mode tray'
    Wait-For { ([LennetteWindows]::GetWindowLongPtr($overlay, -20).ToInt64() -band 0x80) -eq 0 } 'normal-mode taskbar style'
    Send-Command '--privacy'
    Wait-For { [LennetteWindows]::Find($app.Id, 'lennette tray') -eq [IntPtr]::Zero } 'privacy-mode tray removal'
    Wait-For { ([LennetteWindows]::GetWindowLongPtr($overlay, -20).ToInt64() -band 0x80) -ne 0 } 'privacy-mode taskbar style'
    $script:stage = 'switched overlay'
    Assert-Protection $overlay
    $script:stage = 'switched dashboard'
    Send-Command '--toggle-dashboard'
    Wait-For { [LennetteWindows]::IsWindowVisible($dashboard) } 'protected dashboard restore after mode switch'
    Assert-Protection $dashboard
    Write-Output 'PASS: shared rounded frames, pill/dashboard drag regions and controls; desktop capture excludes overlay/dashboard; hotkeys and restore protection; Normal/Privacy switches tray and taskbar visibility.'
} finally {
    if ($diagnostics -and (Test-Path "$diagnostics.err") -and (Get-Item "$diagnostics.err").Length -gt 0) { Get-Content "$diagnostics.err" | Select-Object -Last 30 | Write-Output }
    foreach ($fixture in @($control, $backdrop)) {
        if ($fixture -and $fixture -ne [IntPtr]::Zero) { [LennetteWindows]::DestroyWindow($fixture) | Out-Null }
    }
    if ($app -and -not $app.HasExited) { Stop-Process -Id $app.Id -Force }
    $env:LENNETTE_PRIVATE_OVERLAY = $previousPrivate
}
