# Intenção — atualização automática

Data: 2026-11-26

Referência de inspiração: o mecanismo de atualização do projeto público do
mesmo mantenedor, github.com/luansilvadb/orquestrador (`docs/atualizacao.md`
lá).

## Resultado

O nv-tuner se atualiza sozinho para a última release publicada no GitHub, sem
o usuário baixar ou instalar o MSI.

## Usuário

O mantenedor e todos os usuários; todas as máquinas sempre em dia.

## Como

O serviço verifica em segundo plano (ao iniciar e periodicamente), mesmo com a
interface fechada, baixa o MSI, confere a assinatura Minisign (chave privada
fora do repositório, em secret do GitHub) e instala em silêncio; a interface
só informa depois ("atualizado para a vX").

## Sucesso

Publicar uma tag nova e, pouco depois, as máquinas estarem na versão nova sem
ninguém fazer nada.

## Restrições

- Atualização obrigatória: sem opção de desligar.
- Falha ou falta de internet não bloqueia nada: segue na versão atual e tenta
  depois.
- Nunca atualiza com jogo/anticheat aberto (mesmo portão de processos).
- Assinatura inválida ou ausente não instala nada.
- O upgrade preserva os ajustes (não restaura nada).
- Só sobe de versão: defeito se corrige publicando versão nova.

## Fora do escopo

- Bloquear o app sem internet (diferente do orquestrador).
- Canal de pré-release/beta.
- Assinatura Authenticode (continua pendência separada).
- Voltar de versão.
- Atualizar qualquer coisa além do próprio nv-tuner.

## Nota

Exige uma chave Minisign própria do nv-tuner, gerada pelo mantenedor e
guardada com backup; sem ela, nenhuma máquina instalada aceita atualizações.
