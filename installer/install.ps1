# Installs / upgrades / removes the Weather Clock screensaver (system-wide, needs admin).
param([switch]$Uninstall, [switch]$Quiet)

$ErrorActionPreference = 'Stop'
$Name    = 'Weather Clock'
$ScrName = 'Weather Clock.scr'
$Target  = Join-Path $env:windir "System32\$ScrName"
$UninstKey = 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\WeatherClockScr'
$Keep    = Join-Path $env:ProgramFiles 'Weather Clock'   # holds the uninstaller

# --- elevate ---------------------------------------------------------------
$admin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
         ).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $admin) {
    $argList = @('-NoProfile','-WindowStyle','Hidden','-ExecutionPolicy','Bypass','-File',"`"$PSCommandPath`"")
    if ($Uninstall) { $argList += '-Uninstall' }
    if ($Quiet)     { $argList += '-Quiet' }
    $p = Start-Process powershell.exe -Verb RunAs -WindowStyle Hidden -ArgumentList $argList -Wait -PassThru
    exit $p.ExitCode
}

function Say($m) { if (-not $Quiet) { Write-Host $m } }
function Done($m) {
    if (-not $Quiet) {
        Add-Type -AssemblyName System.Windows.Forms
        [void][Windows.Forms.MessageBox]::Show($m, "$Name screensaver")
    }
}

function Stop-Running {
    # a running .scr locks the file; stop any wxscr / Weather Clock instance
    Get-Process | Where-Object { $_.ProcessName -in 'wxscr','Weather Clock','wxscr-handoff' } |
        Stop-Process -Force -ErrorAction SilentlyContinue
    Start-Sleep -Milliseconds 500
}

function Remove-OldCopies {
    # every place earlier versions may have been put
    $candidates = @(
        $Target,
        (Join-Path $env:windir 'System32\wxscr.scr'),
        (Join-Path $env:windir 'SysWOW64\Weather Clock.scr'),
        (Join-Path $env:windir 'SysWOW64\wxscr.scr')
    )
    foreach ($f in $candidates) {
        if (Test-Path -LiteralPath $f) { Say "Removing old version: $f"; Remove-Item -LiteralPath $f -Force }
    }
}

# The saver hands over to a helper copy on the user's real desktop (see README, "When Windows starts the screensaver itself").
# The task that starts it is registered here and nowhere else: it has no trigger and only runs when the saver asks.
$TaskName = 'WeatherClockScreensaverHandoff'

function Remove-HandoffTask {
    schtasks /delete /f /tn $TaskName 2>&1 | Out-Null
    # earlier versions kept the helper in the user's profile and registered the task from the saver itself
    Remove-Item "$env:APPDATA\wxscr\wxscr-handoff.exe" -Force -ErrorAction SilentlyContinue
}

function Add-HandoffTask {
    try {
        # the Task Scheduler will not start a .scr, so the helper is a copy of the saver with an .exe name
        $action    = New-ScheduledTaskAction -Execute (Join-Path $Keep 'wxscr-handoff.exe') -Argument '/s handoff'
        $principal = New-ScheduledTaskPrincipal -UserId "$env:USERDOMAIN\$env:USERNAME" -LogonType Interactive -RunLevel Limited
        $settings  = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -ExecutionTimeLimit ([TimeSpan]::Zero)
        Register-ScheduledTask -TaskName $TaskName -Action $action -Principal $principal -Settings $settings -Force | Out-Null
    } catch {
        Say "Could not register the hand-off task ($($_.Exception.Message)). The screensaver still works; it just fades out differently when Windows starts it."
    }
}

function Clear-ActiveIfOurs {
    foreach ($hive in 'HKCU:') {
        $k = "$hive\Control Panel\Desktop"
        $cur = (Get-ItemProperty $k -ErrorAction SilentlyContinue).'SCRNSAVE.EXE'
        if ($cur -and ($cur -match 'Weather Clock\.scr$' -or $cur -match 'wxscr\.scr$')) {
            Remove-ItemProperty $k 'SCRNSAVE.EXE' -ErrorAction SilentlyContinue
        }
    }
}

# --- uninstall -------------------------------------------------------------
if ($Uninstall) {
    Stop-Running
    Remove-OldCopies
    Clear-ActiveIfOurs
    Remove-HandoffTask
    Remove-Item $UninstKey -Recurse -Force -ErrorAction SilentlyContinue
    if (Test-Path $Keep) {
        # the uninstaller is this script; delete the folder after we exit
        Start-Process cmd.exe -WindowStyle Hidden -ArgumentList '/c', "ping -n 3 127.0.0.1 >nul & rmdir /s /q `"$Keep`""
    }
    Done "$Name screensaver was removed.`n(Your settings in %APPDATA%\wxscr were left in place.)"
    exit 0
}

# --- install / upgrade -----------------------------------------------------
$src = Join-Path $PSScriptRoot $ScrName
if (-not (Test-Path -LiteralPath $src)) { throw "Missing $ScrName next to the installer." }
$ver = if (Test-Path "$PSScriptRoot\version.txt") { (Get-Content "$PSScriptRoot\version.txt" -Raw).Trim() } else { '0' }

$old = (Get-ItemProperty $UninstKey -ErrorAction SilentlyContinue).DisplayVersion
if ($old) { Say "Upgrading $Name $old -> $ver" } else { Say "Installing $Name $ver" }

Stop-Running
Remove-OldCopies
Copy-Item -LiteralPath $src -Destination $Target -Force

# keep a copy of this script as the uninstaller
New-Item -ItemType Directory -Force $Keep | Out-Null
Copy-Item -LiteralPath $PSCommandPath -Destination (Join-Path $Keep 'install.ps1') -Force

# the hand-off helper (a copy of the saver with an .exe name) and the on-demand task that starts it
Remove-HandoffTask
Copy-Item -LiteralPath $src -Destination (Join-Path $Keep 'wxscr-handoff.exe') -Force
Add-HandoffTask

# Add/Remove Programs entry (overwritten on every upgrade)
New-Item $UninstKey -Force | Out-Null
$props = @{
    DisplayName     = "$Name screensaver"
    DisplayVersion  = $ver
    Publisher       = 'wxscr'
    DisplayIcon     = $Target
    InstallLocation = $Keep
    UninstallString = "powershell.exe -NoProfile -ExecutionPolicy Bypass -File `"$Keep\install.ps1`" -Uninstall"
    NoModify = 1; NoRepair = 1
}
foreach ($k in $props.Keys) {
    $t = if ($props[$k] -is [int]) { 'DWord' } else { 'String' }
    New-ItemProperty $UninstKey -Name $k -Value $props[$k] -PropertyType $t -Force | Out-Null
}

# make it the active screensaver for the user who ran the installer
$desk = 'HKCU:\Control Panel\Desktop'
Set-ItemProperty $desk 'SCRNSAVE.EXE' $Target
Set-ItemProperty $desk 'ScreenSaveActive' '1'
if (-not (Get-ItemProperty $desk).ScreenSaveTimeOut) { Set-ItemProperty $desk 'ScreenSaveTimeOut' '300' }

# no "installed" pop-up: Windows' own Screen Saver Settings (the Screen saver tab of the old Display control panel) opens
# with the new screensaver selected and previewing, which says it already
if (-not $Quiet) { Start-Process -FilePath "$env:windir\System32\rundll32.exe" -ArgumentList 'shell32.dll,Control_RunDLL desk.cpl,,1' }
exit 0
