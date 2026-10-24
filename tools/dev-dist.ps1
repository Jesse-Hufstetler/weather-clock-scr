# Dev build: compiles the release exe and refreshes the loose files in dist\ that are run for quick testing.
#   dist\Weather Clock.scr, dist\Weather.Clock.scr  - the fresh build
#   dist\Settings.bat                               - opens the config screen (Windows 11 has no Configure on right click)
# (dist\ is gitignored, so Settings.bat is regenerated here instead of being checked in.)
param([switch]$SkipCargo)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
Set-Location $root

if (-not $SkipCargo) { cargo build --release; if ($LASTEXITCODE) { throw 'cargo build failed' } }

New-Item -ItemType Directory -Force dist | Out-Null
Copy-Item target\release\wxscr.exe 'dist\Weather Clock.scr' -Force
Copy-Item target\release\wxscr.exe 'dist\Weather.Clock.scr' -Force
Set-Content dist\Settings.bat "@echo off`r`nstart `"`" `"%~dp0Weather Clock.scr`" /c`r`n" -Encoding ASCII
Write-Host 'dist\ refreshed: Weather Clock.scr, Weather.Clock.scr, Settings.bat'
