# Builds dist\WeatherClock-Setup-<version>.exe (single file, uses Windows' built-in IExpress).
param([switch]$SkipCargo)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
Set-Location $root

$ver = ((Select-String '^version\s*=\s*"(.+)"' Cargo.toml).Matches[0].Groups[1].Value)
if (-not $SkipCargo) { cargo build --release; if ($LASTEXITCODE) { throw 'cargo build failed' } }

$stage = Join-Path $env:TEMP "wxscr-installer-$PID"
New-Item -ItemType Directory -Force $stage | Out-Null
Copy-Item target\release\wxscr.exe (Join-Path $stage 'Weather Clock.scr')
Copy-Item installer\install.ps1 $stage
Set-Content (Join-Path $stage 'version.txt') $ver

New-Item -ItemType Directory -Force dist | Out-Null
$out = Join-Path $root "dist\WeatherClock-Setup-$ver.exe"
Remove-Item $out -ErrorAction SilentlyContinue

$sed = @"
[Version]
Class=IEXPRESS
SEDVersion=3
[Options]
PackagePurpose=InstallApp
ShowInstallProgressWindow=0
HideExtractAnimation=1
UseLongFileName=1
InsideCompressed=0
CAB_FixedSize=0
CAB_ResvCodeSigning=0
RebootMode=N
InstallPrompt=
DisplayLicense=
FinishMessage=
TargetName=$out
FriendlyName=Weather Clock screensaver $ver
AppLaunched=powershell.exe -NoProfile -WindowStyle Hidden -ExecutionPolicy Bypass -File install.ps1
PostInstallCmd=<None>
AdminQuietInstCmd=
UserQuietInstCmd=
SourceFiles=SourceFiles
[Strings]
FILE0="Weather Clock.scr"
FILE1="install.ps1"
FILE2="version.txt"
[SourceFiles]
SourceFiles0=$stage\
[SourceFiles0]
%FILE0%=
%FILE1%=
%FILE2%=
"@
$sedPath = Join-Path $stage 'installer.sed'
Set-Content $sedPath $sed -Encoding ASCII
Push-Location $stage
Start-Process iexpress.exe -ArgumentList '/N',$sedPath -Wait
Pop-Location
for ($i = 0; $i -lt 60 -and -not (Test-Path $out); $i++) { Start-Sleep -Milliseconds 500 }
if (-not (Test-Path $out)) { throw 'IExpress did not produce an installer' }
Remove-Item $stage -Recurse -Force
Write-Host "Built $out"
