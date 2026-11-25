# Segurança

O nv-tuner instala um serviço do Windows que roda como SYSTEM e grava no
perfil do driver NVIDIA, então problemas de segurança (escalada de
privilégio pelo named pipe, pelo diretório `%ProgramData%\nv-tuner` ou pelo
instalador, por exemplo) são levados a sério.

## Como reportar uma vulnerabilidade

**Não abra issue pública.** Reporte de forma privada pelo GitHub Security
Advisories: aba **Security** do repositório → **Report a vulnerability**
([link direto](../../security/advisories/new)).

Inclua, se possível:

- versão do nv-tuner e do Windows;
- descrição do problema e do impacto;
- passos para reproduzir ou prova de conceito mínima.

Não inclua dados pessoais seus nem de terceiros. A resposta inicial deve vir
em alguns dias; a correção e a divulgação são combinadas no próprio advisory.

## Versões suportadas

Pré-alfa: só a versão mais recente publicada em Releases recebe correções.
