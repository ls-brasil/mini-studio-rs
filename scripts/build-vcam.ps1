[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
cmake -S (Join-Path $repo 'native/MiniStudioRSVCam') -B (Join-Path $repo 'target/vcam') -A x64
if ($LASTEXITCODE -ne 0) { throw 'Falha na configuração CMake. Instale CMake, MSVC C++ e Windows SDK 11.' }
cmake --build (Join-Path $repo 'target/vcam') --config Release
if ($LASTEXITCODE -ne 0) { throw 'Falha ao compilar a câmera virtual.' }
ctest --test-dir (Join-Path $repo 'target/vcam') -C Release --output-on-failure
if ($LASTEXITCODE -ne 0) { throw 'Falha nos testes nativos.' }
Write-Host 'Componente pronto em target/vcam/Release/MiniStudioRSVCam.dll'
