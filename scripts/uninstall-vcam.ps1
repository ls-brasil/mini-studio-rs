#Requires -RunAsAdministrator
[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
if (-not [Environment]::Is64BitProcess) { throw 'Use PowerShell x64.' }
$target = Join-Path $env:ProgramFiles 'MiniStudioRS\MiniStudioRSVCam.dll'
if (Test-Path -LiteralPath $target) {
    $registration = Start-Process -FilePath "$env:SystemRoot\System32\regsvr32.exe" -ArgumentList @('/s', '/u', ('"' + $target + '"')) -WindowStyle Hidden -Wait -PassThru
    if ($registration.ExitCode -ne 0) { throw "Falha ao remover registro COM: $($registration.ExitCode)." }
    Remove-Item -LiteralPath $target -Force
}
$frames = Join-Path $env:ProgramData 'MiniStudioRS\vcam-frames.bin'
if (Test-Path -LiteralPath $frames) { Remove-Item -LiteralPath $frames -Force }
Write-Host 'Componente removido. Se a DLL estiver em uso, feche os consumidores e tente novamente após reiniciar o Windows.'
