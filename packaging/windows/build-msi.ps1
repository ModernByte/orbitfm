<#
.SYNOPSIS
    Собирает MSI из подготовленного каталога dist/OrbitFM через WiX v7.

.DESCRIPTION
    AppDir должен содержать orbitfm.exe, libmpv-2.dll и остальные runtime DLL.
    WiX устанавливается отдельно: dotnet tool install --global wix --version 7.0.0.
    Элемент Files для автосбора каталога есть только начиная с WiX 7.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string] $AppDir,
    [Parameter(Mandatory)][string] $Version,
    [Parameter(Mandatory)][string] $Output
)

$ErrorActionPreference = 'Stop'

$resolvedApp = (Resolve-Path -LiteralPath $AppDir).Path
$template = Join-Path $PSScriptRoot 'orbitfm.wxs'

$wix = Join-Path $env:USERPROFILE '.dotnet\tools\wix.exe'
if (-not (Test-Path -LiteralPath $wix)) {
    $command = Get-Command wix.exe -ErrorAction SilentlyContinue
    if (-not $command) {
        throw 'wix.exe не найден; выполните dotnet tool install --global wix --version 7.0.0'
    }
    $wix = $command.Source
}

New-Item -ItemType Directory -Force -Path (Split-Path -Parent $Output) | Out-Null

& $wix build $template -arch x64 -d "AppDir=$resolvedApp" -d "Version=$Version" -out $Output
if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath $Output)) {
    throw "WiX не собрал MSI: $Output"
}

Write-Host "MSI: $Output"
