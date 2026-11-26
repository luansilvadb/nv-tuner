#Requires -Version 5.1
<#
.SYNOPSIS
  Gera o BUILD DE TESTE local do mantenedor (docs/spec/interface-e-saude.md D1, D1a, §8.4).

.DESCRIPTION
  NÃO É PARA USUÁRIOS. Compila em release nvt-service, nvt-agente e nvt-ui com a feature
  'medicao-partidas' (que liga 'build-de-teste' de nvt-manutencao) e gera um MSI separado
  em target\dist-teste\nv-tuner-X.Y.Z-teste-x64.msi. Os binários se identificam como
  versão 'X.Y.Z+teste' e mostram "Build de teste".

  NVT_BUILD_DE_TESTE=1 é definida só neste processo (exigida pelos build.rs). Recusa rodar
  se CI ou GITHUB_ACTIONS estiverem definidos: o CI nunca gera este build.

.PARAMETER SemValidar
  Repassado a instalador\build-msi.ps1 (pula a validação ICE).

.PARAMETER Dotnet
  Repassado a instalador\build-msi.ps1.

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File scripts\build-teste.ps1
#>
[CmdletBinding()]
param(
    [switch]$SemValidar,
    [string]$Dotnet
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Write-Aviso([string]$Texto) {
    Write-Host ''
    Write-Host '==================================================================' -ForegroundColor Yellow
    Write-Host "  $Texto" -ForegroundColor Yellow
    Write-Host '==================================================================' -ForegroundColor Yellow
    Write-Host ''
}

if ($env:CI -or $env:GITHUB_ACTIONS) {
    throw 'Build de teste recusado: CI ou GITHUB_ACTIONS definido. Este build é só local do mantenedor.'
}

$Raiz = Split-Path -Parent $PSScriptRoot
$Crates = Join-Path $Raiz 'crates'

if (-not (Test-Path (Join-Path $Crates 'nvt-medicao\Cargo.toml'))) {
    throw 'crates\nvt-medicao não existe: a gravação de partidas (feature medicao-partidas) ainda não foi implementada; não há build de teste para gerar.'
}

# Feature de teste (D9/Q11): 'medicao-partidas' nos três binários; ela liga também o
# dry-run da limpeza (build-de-teste) no serviço e no agente.
function Get-FeatureTeste([string]$Pacote) {
    $toml = Get-Content -Raw -Encoding UTF8 (Join-Path $Crates "$Pacote\Cargo.toml")
    $secao = [regex]::Match($toml, '(?ms)^\[features\]\s*$(.*?)(?=^\[|\z)')
    if ($secao.Success -and $secao.Groups[1].Value -match '(?m)^\s*medicao-partidas\s*=') {
        return 'medicao-partidas'
    }
    throw "crates\$Pacote\Cargo.toml não declara a feature 'medicao-partidas'."
}

$Pacotes = 'nvt-service', 'nvt-agente', 'nvt-ui'
$Features = @(foreach ($p in $Pacotes) { "$p/$(Get-FeatureTeste $p)" })

$toml = Get-Content -Raw -Encoding UTF8 (Join-Path $Raiz 'Cargo.toml')
$m = [regex]::Match($toml, '(?ms)^\[workspace\.package\].*?^version\s*=\s*"([^"]+)"')
if (-not $m.Success) { throw 'Versão não encontrada em [workspace.package] do Cargo.toml.' }
$Versao = $m.Groups[1].Value

Write-Aviso "BUILD DE TESTE $Versao+teste - NÃO É PARA USUÁRIOS, NÃO PUBLIQUE"

# Destino próprio (target\teste): nunca mistura binários de teste com target\release.
$anterior = $env:NVT_BUILD_DE_TESTE
$anteriorTarget = $env:CARGO_TARGET_DIR
$env:NVT_BUILD_DE_TESTE = '1'
$env:CARGO_TARGET_DIR = Join-Path $Raiz 'target\teste'
try {
    $argsCargo = @('build', '--release', '--locked') + @(foreach ($p in $Pacotes) { '-p'; $p }) + @('--features', ($Features -join ','))
    Write-Host "==> cargo $($argsCargo -join ' ')"
    Push-Location $Raiz
    try {
        & cargo @argsCargo
        if ($LASTEXITCODE -ne 0) { throw "cargo build falhou ($LASTEXITCODE)." }
    }
    finally { Pop-Location }

    $argsMsi = @{ Versao = $Versao; SemCompilar = $true; SufixoTeste = 'teste'; OrigemBin = 'target\teste\release' }
    if ($SemValidar) { $argsMsi.SemValidar = $true }
    if ($Dotnet) { $argsMsi.Dotnet = $Dotnet }
    & (Join-Path $Raiz 'instalador\build-msi.ps1') @argsMsi
}
finally {
    $env:NVT_BUILD_DE_TESTE = $anterior
    $env:CARGO_TARGET_DIR = $anteriorTarget
}

Write-Aviso 'MSI de BUILD DE TESTE (target\dist-teste\). NÃO É PARA USUÁRIOS; só VM do mantenedor.'
Write-Host 'Binários de teste em target\teste\release; target\release não foi tocado.' -ForegroundColor Yellow
