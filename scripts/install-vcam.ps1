#Requires -RunAsAdministrator
[CmdletBinding()]
param(
    [string]$DllPath = (Join-Path (Split-Path -Parent $PSScriptRoot) 'target/vcam/Release/MiniStudioRSVCam.dll'),
    [string]$ProducerSid = ([System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value)
)
$ErrorActionPreference = 'Stop'
if (-not [Environment]::Is64BitProcess) { throw 'Use PowerShell x64.' }
$build = [int](Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion').CurrentBuildNumber
if ($build -lt 22000) { throw 'A câmera virtual exige Windows 11 build 22000 ou superior.' }
if (-not (Test-Path -LiteralPath $DllPath -PathType Leaf)) { throw 'Compile primeiro com scripts/build-vcam.ps1.' }
$installDir = Join-Path $env:ProgramFiles 'MiniStudioRS'
$dataDir = Join-Path $env:ProgramData 'MiniStudioRS'
New-Item -ItemType Directory -Force -Path $installDir | Out-Null
New-Item -ItemType Directory -Force -Path $dataDir | Out-Null

# Vídeo da composição: somente o usuário produtor escreve; o Frame Server lê.
$acl = New-Object System.Security.AccessControl.DirectorySecurity
$acl.SetAccessRuleProtection($true, $false)
foreach ($entry in @(
    @('S-1-5-18', 'FullControl'),
    @('S-1-5-32-544', 'FullControl'),
    @('S-1-5-19', 'ReadAndExecute'),
    @($ProducerSid, 'Modify')
)) {
    $sid = New-Object System.Security.Principal.SecurityIdentifier($entry[0])
    $rule = New-Object System.Security.AccessControl.FileSystemAccessRule($sid, $entry[1], 'ContainerInherit,ObjectInherit', 'None', 'Allow')
    $acl.AddAccessRule($rule)
}
Set-Acl -LiteralPath $dataDir -AclObject $acl
$target = Join-Path $installDir 'MiniStudioRSVCam.dll'
Copy-Item -LiteralPath $DllPath -Destination $target -Force
$registration = Start-Process -FilePath "$env:SystemRoot\System32\regsvr32.exe" -ArgumentList @('/s', ('"' + $target + '"')) -WindowStyle Hidden -Wait -PassThru
if ($registration.ExitCode -ne 0) { throw "Falha no registro COM: $($registration.ExitCode)." }
$key = 'HKLM:\SOFTWARE\Classes\CLSID\{F0C418C8-9211-479E-BAAE-3CBF90F15D37}\InprocServer32'
if ((Get-Item -LiteralPath $key).GetValue('') -ne $target) { throw 'O registro COM não aponta para a DLL instalada.' }
Write-Host 'Componente instalado. Abra o Mini Studio e clique em Iniciar câmera virtual.'
