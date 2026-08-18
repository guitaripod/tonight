$ErrorActionPreference = 'Stop'
$Repo = 'guitaripod/tonight'
$Version = if ($env:TONIGHT_VERSION) { $env:TONIGHT_VERSION } else { 'latest' }

$Dir = Join-Path $env:LOCALAPPDATA 'tonight'
New-Item -ItemType Directory -Force -Path $Dir | Out-Null
$Exe = Join-Path $Dir 'tonight.exe'

Write-Host 'Downloading tonight...'
Invoke-WebRequest "https://github.com/$Repo/releases/$Version/download/tonight-windows-x86_64.exe" -OutFile $Exe

$RunKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
Set-ItemProperty -Path $RunKey -Name 'tonight' -Value "`"$Exe`" --open"

Start-Process $Exe -ArgumentList '--open'
Start-Sleep -Seconds 2
Start-Process 'http://localhost:7777'

$Lan = Get-NetIPAddress -AddressFamily IPv4 -ErrorAction SilentlyContinue |
    Where-Object { $_.IPAddress -notlike '169.254.*' -and $_.IPAddress -ne '127.0.0.1' } |
    Select-Object -First 1

Write-Host ''
Write-Host '  tonight is up.   http://localhost:7777'
if ($Lan) { Write-Host "  From your phone: http://$($Lan.IPAddress):7777" }
Write-Host '  It comes back every time you log in.'
Write-Host '  Stop browsing. Deal a hand.'