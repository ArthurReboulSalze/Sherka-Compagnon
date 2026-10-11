param([int]$TargetProcessId)
$ErrorActionPreference = 'Stop'
Add-Type @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class SherkaStartupProbe {
    public delegate bool Callback(IntPtr window, IntPtr arg);
    [DllImport("user32.dll")] public static extern bool EnumWindows(Callback callback, IntPtr arg);
    [DllImport("user32.dll")] public static extern bool EnumChildWindows(IntPtr window, Callback callback, IntPtr arg);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr window, out uint pid);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr SendMessageTimeout(IntPtr window, uint msg, IntPtr length, StringBuilder text, uint flags, uint timeout, out IntPtr result);
    public static string Text(IntPtr window) {
        var text = new StringBuilder(1024);
        IntPtr result;
        SendMessageTimeout(window, 13, new IntPtr(1024), text, 2, 200, out result);
        return text.ToString();
    }
}
'@
$windows = [Collections.Generic.List[string]]::new()
[void][SherkaStartupProbe]::EnumWindows({
    param($window, $argument)
    [uint32]$windowProcess = 0
    [void][SherkaStartupProbe]::GetWindowThreadProcessId($window, [ref]$windowProcess)
    if ($windowProcess -eq $TargetProcessId) {
        $windows.Add([SherkaStartupProbe]::Text($window))
        [void][SherkaStartupProbe]::EnumChildWindows($window, {
            param($childWindow, $argument)
            $windows.Add([SherkaStartupProbe]::Text($childWindow))
            return $true
        }, [IntPtr]::Zero)
    }
    return $true
}, [IntPtr]::Zero)
$root = Get-Process -Id $TargetProcessId -ErrorAction SilentlyContinue
$children = @(Get-CimInstance Win32_Process | Where-Object ParentProcessId -eq $TargetProcessId | Select-Object Name,ProcessId,SessionId)
@{running=[bool]$root;session=$root.SessionId;windows=@($windows);children=$children} | ConvertTo-Json -Depth 4 -Compress
