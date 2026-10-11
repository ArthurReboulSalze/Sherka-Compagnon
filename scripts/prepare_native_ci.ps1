# Cargo supplies dependency search paths to `cargo test`; a direct EXE launch
# needs them explicitly on Windows Server. This does not alter release packages.
$ErrorActionPreference = 'Stop'
& (Join-Path $PSScriptRoot 'ensure_webview2.ps1')
$searchPaths = @()
Get-ChildItem -Path 'target/debug/build/ort-sys-*/output' -File | ForEach-Object {
    foreach ($line in Get-Content -LiteralPath $_.FullName) {
        if ($line -match '^cargo:rustc-link-search=(?:native=)?(.+)$' -and (Test-Path -LiteralPath $Matches[1] -PathType Container)) {
            $searchPaths += $Matches[1]
        }
    }
}
$searchPaths = @($searchPaths | Select-Object -Unique)
foreach ($path in $searchPaths) {
    if ($env:GITHUB_PATH) { $path | Out-File -LiteralPath $env:GITHUB_PATH -Encoding utf8 -Append }
}
$env:PATH = (($searchPaths + @($env:PATH)) -join ';')
foreach ($library in @('DirectML.dll', 'dxcore.dll', 'msvcp140.dll', 'msvcp140_1.dll')) {
    $locations = @([Environment]::SystemDirectory) + $searchPaths
    $location = $locations | ForEach-Object { Join-Path $_ $library } | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
    Write-Output "Native dependency $library available: $([bool]$location)"
}
