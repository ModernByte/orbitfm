<#
.SYNOPSIS
    Готовит mpv.lib для MSVC из libmpv-2.dll (MSYS2 UCRT64).

.DESCRIPTION
    libmpv2-sys ожидает библиотеку `mpv`, но MSYS2 поставляет только GNU-импорт
    `libmpv.dll.a`, который link.exe не читает. Скрипт извлекает экспорты через
    dumpbin и собирает COFF-импорт библиотеку через lib.exe.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string] $MpvDll,
    [Parameter(Mandatory)][string] $OutDir
)

$ErrorActionPreference = 'Stop'

if (-not (Test-Path -LiteralPath $MpvDll)) {
    throw "Не найден libmpv: $MpvDll"
}
foreach ($tool in 'dumpbin.exe', 'lib.exe') {
    if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) {
        throw "$tool не найден в PATH; нужен шаг ilammy/msvc-dev-cmd"
    }
}

New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$dllName = Split-Path -Leaf $MpvDll
$defPath = Join-Path $OutDir 'libmpv-2.def'
$libPath = Join-Path $OutDir 'mpv.lib'

$exports = & dumpbin.exe /nologo /exports $MpvDll
if ($LASTEXITCODE -ne 0) {
    throw "dumpbin завершился с кодом $LASTEXITCODE"
}

$names = [System.Collections.Generic.List[string]]::new()
foreach ($line in $exports) {
    if ($line -match '^\s*\d+\s+[0-9A-Fa-f]+\s+[0-9A-Fa-f]+\s+(\S+)\s*$') {
        $name = $Matches[1]
        if ($name -notmatch '^[A-Za-z_][A-Za-z0-9_]*$') {
            throw "Неожиданное имя экспорта libmpv: $name"
        }
        $names.Add($name)
    }
}
if ($names.Count -eq 0) {
    throw 'В libmpv-2.dll не найдено ни одного экспорта'
}

$content = "LIBRARY $dllName`r`nEXPORTS`r`n" + ($names -join "`r`n") + "`r`n"
Set-Content -LiteralPath $defPath -Value $content -Encoding ascii

& lib.exe /nologo "/def:$defPath" /machine:x64 "/out:$libPath"
if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath $libPath)) {
    throw "lib.exe не собрал $libPath"
}

Write-Host "mpv.lib: $libPath ($($names.Count) экспортов)"
