#Requires -Version 5.1
<#
.SYNOPSIS
  Compila o nv-tuner em release e gera o MSI + SHA256 em target\dist\.

.DESCRIPTION
  Requisitos: Rust (cargo) e .NET SDK 8 ou superior (só para rodar o WiX como
  ferramenta local; o MSI gerado não depende de .NET). O WiX 5.0.2 é fixado em
  instalador\.config\dotnet-tools.json e baixado por 'dotnet tool restore'.
  Não exige administrador. NÃO executa o MSI.

.PARAMETER Versao
  Versão MSI (major.minor.build). Padrão: [workspace.package].version do Cargo.toml.

.PARAMETER SemCompilar
  Não roda o cargo; usa os .exe já presentes em target\release.

.PARAMETER SemValidar
  Pula a validação ICE ('wix msi validate').

.PARAMETER SufixoTeste
  Só para o build de teste (scripts\build-teste.ps1): acrescenta '-<sufixo>' ao nome do
  MSI e grava em target\dist-<sufixo>\, separado dos MSIs normais. Vazio (padrão): MSI
  normal em target\dist\, comportamento inalterado.

.PARAMETER Dotnet
  Caminho do dotnet.exe. Padrão: $env:NVT_DOTNET ou o 'dotnet' do PATH.

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File instalador\build-msi.ps1
#>
[CmdletBinding()]
param(
    [string]$Versao,
    [switch]$SemCompilar,
    [switch]$SemValidar,
    [ValidatePattern('^[a-z0-9]*$')]
    [string]$SufixoTeste = '',
    [string]$Dotnet
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$VersaoWixExt = '5.0.2'   # mesma versão do 'wix' no manifesto de ferramentas
$Extensoes = @('WixToolset.UI.wixext', 'WixToolset.Util.wixext')

function Invoke-Nativo {
    param([string]$Exe, [string[]]$Argumentos)
    & $Exe @Argumentos
    if ($LASTEXITCODE -ne 0) {
        throw "Falhou (código $LASTEXITCODE): $Exe $($Argumentos -join ' ')"
    }
}

$Instalador = $PSScriptRoot
$Raiz = Split-Path -Parent $Instalador
$BinDir = Join-Path $Raiz 'target\release'
$Dist = Join-Path $Raiz 'target\dist'
$Obj = Join-Path $Raiz 'target\wix'
$SufixoNome = ''
if ($SufixoTeste) {
    $Dist = Join-Path $Raiz "target\dist-$SufixoTeste"
    $Obj = Join-Path $Raiz "target\wix-$SufixoTeste"
    $SufixoNome = "-$SufixoTeste"
}

# Versão a partir do Cargo.toml da raiz.
if (-not $Versao) {
    $toml = Get-Content -Raw -Encoding UTF8 (Join-Path $Raiz 'Cargo.toml')
    $m = [regex]::Match($toml, '(?ms)^\[workspace\.package\].*?^version\s*=\s*"([^"]+)"')
    if (-not $m.Success) { throw 'Versão não encontrada em [workspace.package] do Cargo.toml.' }
    $Versao = $m.Groups[1].Value
}
# MSI aceita só números: descarta sufixos de pré-release (ex.: 0.1.0-beta.1 -> 0.1.0).
$VersaoMsi = ($Versao -split '[-+]')[0]
if ($VersaoMsi -notmatch '^\d+\.\d+\.\d+$') { throw "Versão inválida para MSI: '$Versao'." }

# dotnet
if (-not $Dotnet) { $Dotnet = $env:NVT_DOTNET }
if (-not $Dotnet) {
    $cmd = Get-Command dotnet -ErrorAction SilentlyContinue
    if ($cmd) { $Dotnet = $cmd.Source }
}
if (-not $Dotnet -or -not (Test-Path $Dotnet)) {
    throw '.NET SDK não encontrado. Instale o .NET SDK 8+ (ou passe -Dotnet / defina NVT_DOTNET).'
}

# 1. Compilação release.
if (-not $SemCompilar) {
    Write-Host '==> cargo build --release -p nvt-service -p nvt-ui -p nvt-agente'
    Push-Location $Raiz
    try { Invoke-Nativo 'cargo' @('build', '--release', '-p', 'nvt-service', '-p', 'nvt-ui', '-p', 'nvt-agente') }
    finally { Pop-Location }
}
foreach ($exe in 'nvt-service.exe', 'nvt-ui.exe', 'nvt-agente.exe') {
    if (-not (Test-Path (Join-Path $BinDir $exe))) { throw "Binário ausente: $BinDir\$exe" }
}

New-Item -ItemType Directory -Force $Dist, $Obj | Out-Null
$Msi = Join-Path $Dist "nv-tuner-$VersaoMsi$SufixoNome-x64.msi"

# 2. WiX (ferramenta local + extensões UI e Util, cacheadas em instalador\.wix\).
Push-Location $Instalador
try {
    Write-Host '==> dotnet tool restore (WiX)'
    Invoke-Nativo $Dotnet @('tool', 'restore')

    $argsExt = @()
    foreach ($nomeExt in $Extensoes) {
        $ref = "$nomeExt/$VersaoWixExt"
        if (-not (Test-Path (Join-Path $Instalador ".wix\extensions\$nomeExt\$VersaoWixExt"))) {
            Write-Host "==> wix extension add $ref"
            Invoke-Nativo $Dotnet @('wix', 'extension', 'add', $ref)
        }
        $argsExt += @('-ext', $ref)
    }

    Write-Host "==> wix build -> $Msi"
    Invoke-Nativo $Dotnet (@(
        'wix', 'build', 'nv-tuner.wxs',
        '-arch', 'x64',
        '-culture', 'pt-BR') + $argsExt + @(
        '-d', "Versao=$VersaoMsi",
        '-d', "BinDir=$BinDir",
        '-d', "RaizRepo=$Raiz",
        '-intermediatefolder', $Obj,
        '-pdb', (Join-Path $Obj 'nv-tuner.wixpdb'),
        '-o', $Msi
    ))

    # 3. Validação ICE (não instala nada).
    if (-not $SemValidar) {
        Write-Host '==> wix msi validate (ICE)'
        Invoke-Nativo $Dotnet @('wix', 'msi', 'validate', '-pdb', (Join-Path $Obj 'nv-tuner.wixpdb'), $Msi)
    }
}
finally { Pop-Location }

# 4. SHA256 no formato do sha256sum ("<hash>  <arquivo>").
$hash = (Get-FileHash -Algorithm SHA256 $Msi).Hash.ToLowerInvariant()
$nome = Split-Path -Leaf $Msi
$arqHash = "$Msi.sha256"
[System.IO.File]::WriteAllText($arqHash, "$hash  $nome`n", (New-Object System.Text.UTF8Encoding($false)))

Write-Host ''
Write-Host "MSI:    $Msi"
Write-Host "SHA256: $hash"
Write-Host 'O MSI NÃO foi executado. Para testar, use uma VM descartável.'
