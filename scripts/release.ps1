[CmdletBinding()]
param([switch]$SkipBuild)
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
Push-Location $repo
try {
    $version = (Get-Content (Join-Path $repo 'package.json') -Raw | ConvertFrom-Json).version
    if ($version -cnotmatch '^[0-9]+\.[0-9]+\.[0-9]+$') { throw 'version inválido no package.json: use major.minor.patch.' }
    $parts = $version.Split('.')
    if ([decimal]$parts[0] -gt 255 -or [decimal]$parts[1] -gt 255 -or [decimal]$parts[2] -gt 65535) { throw 'version excede os limites do MSI (255.255.65535).' }
    if (-not $SkipBuild) {
        cargo fmt -- --check
        if ($LASTEXITCODE -ne 0) { throw 'Formatação inválida.' }
        cargo test --locked --release
        if ($LASTEXITCODE -ne 0) { throw 'Os testes Rust falharam.' }
        cargo clippy --locked --release --all-targets -- -D warnings
        if ($LASTEXITCODE -ne 0) { throw 'Clippy encontrou problemas.' }
        cargo build --locked --release
        if ($LASTEXITCODE -ne 0) { throw 'Falha na compilação do aplicativo.' }
        & (Join-Path $PSScriptRoot 'build-vcam.ps1')
    }
    foreach ($binary in @('target/release/mini-studio-rs.exe', 'target/vcam/Release/MiniStudioRSVCam.dll')) {
        $path = Join-Path $repo $binary
        if (-not (Test-Path -LiteralPath $path)) { throw "Compile primeiro: $binary" }
        if ((Get-Item -LiteralPath $path).VersionInfo.ProductVersion -ne $version) { throw "Versão desatualizada em $binary. Recompile antes de empacotar." }
    }
    python (Join-Path $PSScriptRoot 'third-party-notices.py')
    if ($LASTEXITCODE -ne 0) { throw 'Falha ao reunir avisos de terceiros.' }
    $wix = Join-Path $repo 'target/tools/wix.exe'
    if (-not (Test-Path -LiteralPath $wix)) {
        dotnet tool install wix --tool-path (Join-Path $repo 'target/tools') --version 5.0.2
        if ($LASTEXITCODE -ne 0) { throw 'Instale o .NET SDK para preparar a ferramenta de empacotamento WiX.' }
    }
    & $wix extension add -g WixToolset.UI.wixext/5.0.2
    if ($LASTEXITCODE -ne 0) { throw 'Falha ao preparar os diálogos do instalador.' }
    $out = Join-Path $repo "dist/MiniStudio-$version-win-x64"
    New-Item -ItemType Directory -Force -Path $out | Out-Null
    Copy-Item -LiteralPath (Join-Path $repo 'target/release/mini-studio-rs.exe') -Destination $out -Force
    Copy-Item -LiteralPath (Join-Path $repo 'docs/Guia-do-usuario.txt') -Destination $out -Force
    Copy-Item -LiteralPath (Join-Path $repo 'target/Avisos-de-terceiros.txt') -Destination $out -Force
    $msi = Join-Path $repo "dist/MiniStudio-$version-Setup.msi"
    & $wix build (Join-Path $repo 'installer/MiniStudio.wxs') -arch x64 -culture pt-BR -ext WixToolset.UI.wixext -d "Version=$version" -d "Repo=$repo" -o $msi
    if ($LASTEXITCODE -ne 0) { throw 'Falha ao criar o instalador MSI.' }
    & $wix msi validate $msi
    if ($LASTEXITCODE -ne 0) { throw 'O instalador não passou nas verificações ICE.' }
    $zip = Join-Path $repo "dist/MiniStudio-$version-portable.zip"
    Compress-Archive -LiteralPath (Join-Path $out 'mini-studio-rs.exe'), (Join-Path $out 'Guia-do-usuario.txt'), (Join-Path $out 'Avisos-de-terceiros.txt') -DestinationPath $zip -Force
    Get-FileHash -LiteralPath $msi,$zip -Algorithm SHA256 | ForEach-Object { "$($_.Hash.ToLower())  $([IO.Path]::GetFileName($_.Path))" } | Set-Content (Join-Path $repo "dist/SHA256SUMS-$version.txt") -Encoding ASCII
    Write-Host "Pronto: $msi"
    Write-Host "Portátil: $zip (a câmera virtual exige instalação pelo MSI)."
}
finally { Pop-Location }
