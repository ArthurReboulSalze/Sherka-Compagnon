param([int]$TargetProcessId)
$ErrorActionPreference = 'Stop'
Add-Type @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class SherkaWindowProbe {
    public delegate bool Callback(IntPtr window, IntPtr arg);
    [DllImport("user32.dll")] public static extern bool EnumWindows(Callback callback, IntPtr arg);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr window, out uint pid);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowText(IntPtr window, StringBuilder text, int max);
}
'@
$result = @{widget = $false; panel = $false}
[SherkaWindowProbe]::EnumWindows({
    param($window, $argument)
    [uint32]$windowProcess = 0
    [void][SherkaWindowProbe]::GetWindowThreadProcessId($window, [ref]$windowProcess)
    if ($windowProcess -eq $TargetProcessId) {
        $title = New-Object System.Text.StringBuilder 256
        [void][SherkaWindowProbe]::GetWindowText($window, $title, 256)
        if ($title.ToString().EndsWith('Compagnon')) { $result.widget = [SherkaWindowProbe]::IsWindowVisible($window) }
        if ($title.ToString().EndsWith('Poste de bord')) { $result.panel = [SherkaWindowProbe]::IsWindowVisible($window) }
    }
    return $true
}, [IntPtr]::Zero) | Out-Null
$result | ConvertTo-Json -Compress
