# CI prerequisite, following Microsoft's Evergreen deployment instructions:
# https://learn.microsoft.com/microsoft-edge/webview2/concepts/distribution
$ErrorActionPreference = 'Stop'

function Get-WebViewRuntimeVersion {
    $runtimeId = '{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}'
    $roots = @(
        "HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\$runtimeId",
        "HKLM:\SOFTWARE\Microsoft\EdgeUpdate\Clients\$runtimeId",
        "HKCU:\Software\Microsoft\EdgeUpdate\Clients\$runtimeId"
    )
    foreach ($root in $roots) {
        $version = (Get-ItemProperty -LiteralPath $root -ErrorAction SilentlyContinue).pv
        if ($version -and $version -ne '0.0.0.0') { return $version }
    }
    return $null
}

$runtimeVersion = Get-WebViewRuntimeVersion
if (!$runtimeVersion) {
    $runtimeTemp = if ($env:RUNNER_TEMP) { $env:RUNNER_TEMP } else { [IO.Path]::GetTempPath() }
    $bootstrap = Join-Path $runtimeTemp 'MicrosoftEdgeWebview2Setup.exe'
    Invoke-WebRequest -Uri 'https://go.microsoft.com/fwlink/p/?LinkId=2124703' -OutFile $bootstrap
    $signature = Get-AuthenticodeSignature -LiteralPath $bootstrap
    if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Subject -notmatch 'Microsoft Corporation') {
        throw 'WebView2 bootstrapper must have a valid Microsoft signature.'
    }
    $installer = Start-Process -FilePath $bootstrap -ArgumentList @('/silent', '/install') -WindowStyle Hidden -Wait -PassThru
    $runtimeVersion = Get-WebViewRuntimeVersion
    if (!$runtimeVersion) { throw "WebView2 installation failed: $($installer.ExitCode)" }
}
Write-Output "WebView2 Runtime: $runtimeVersion"
Write-Output "Native test session: $((Get-Process -Id $PID).SessionId)"
