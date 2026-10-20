param([int]$TargetProcessId, [int]$MoveWidgetX = 0, [int]$MoveWidgetY = 0)
$ErrorActionPreference = 'Stop'
Add-Type @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class ImageViewerProbe {
    public delegate bool Callback(IntPtr window, IntPtr arg);
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int left, top, right, bottom; }
    [DllImport("user32.dll")] public static extern bool EnumWindows(Callback callback, IntPtr arg);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr window, out uint pid);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr window, StringBuilder text, int max);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr window, out Rect rect);
    [DllImport("user32.dll", EntryPoint="GetWindowLongPtrW")] public static extern IntPtr GetWindowLongPtr(IntPtr window, int index);
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr window, IntPtr after, int x, int y, int w, int h, uint flags);
}
'@
$result = @{}
$rank = [System.Collections.Generic.List[int]]::new()
[ImageViewerProbe]::EnumWindows({
    param($window, $argument)
    [uint32]$windowProcess = 0
    [void][ImageViewerProbe]::GetWindowThreadProcessId($window, [ref]$windowProcess)
    if ($windowProcess -eq $TargetProcessId) {
        $title = New-Object System.Text.StringBuilder 256
        [void][ImageViewerProbe]::GetWindowText($window, $title, 256)
        $key = if ($title.ToString().EndsWith('Compagnon')) {'widget'} elseif ($title.ToString().EndsWith('Illustration')) {'image'}
        if ($key) {
            $rect = New-Object ImageViewerProbe+Rect
            [void][ImageViewerProbe]::GetWindowRect($window, [ref]$rect)
            if ($key -eq 'widget' -and ($MoveWidgetX -ne 0 -or $MoveWidgetY -ne 0)) {
                [void][ImageViewerProbe]::SetWindowPos($window, [IntPtr]::Zero, $rect.left + $MoveWidgetX, $rect.top + $MoveWidgetY, 0, 0, 0x0015)
                [void][ImageViewerProbe]::GetWindowRect($window, [ref]$rect)
            }
            $result[$key] = @{
                visible = [ImageViewerProbe]::IsWindowVisible($window)
                topmost = ([ImageViewerProbe]::GetWindowLongPtr($window, -20).ToInt64() -band 8) -ne 0
                x = $rect.left; y = $rect.top; width = $rect.right - $rect.left; height = $rect.bottom - $rect.top
                rank = $rank.Count
            }
        }
    }
    $rank.Add(0)
    return $true
}, [IntPtr]::Zero) | Out-Null
$result | ConvertTo-Json -Depth 3 -Compress
