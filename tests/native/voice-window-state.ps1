$ErrorActionPreference = 'Stop'
Add-Type @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class VoiceWindowProbe {
    public delegate bool Callback(IntPtr window, IntPtr arg);
    [DllImport("user32.dll")] public static extern bool EnumWindows(Callback callback, IntPtr arg);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr window);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr window, out uint pid);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowText(IntPtr window, StringBuilder text, int max);
}
'@
$voiceIds = @(Get-Process VoiceStudio -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Id)
$windows = [System.Collections.Generic.List[object]]::new()
[VoiceWindowProbe]::EnumWindows({
    param($window, $argument)
    [uint32]$windowProcess = 0
    [void][VoiceWindowProbe]::GetWindowThreadProcessId($window, [ref]$windowProcess)
    if ($voiceIds -contains $windowProcess -and [VoiceWindowProbe]::IsWindowVisible($window)) {
        $title = [System.Text.StringBuilder]::new(256)
        [void][VoiceWindowProbe]::GetWindowText($window, $title, 256)
        if ($title.Length -gt 0) {
            $windows.Add(@{pid = $windowProcess; title = $title.ToString(); minimized = [VoiceWindowProbe]::IsIconic($window)})
        }
    }
    return $true
}, [IntPtr]::Zero) | Out-Null
ConvertTo-Json -InputObject @($windows.ToArray()) -Compress
