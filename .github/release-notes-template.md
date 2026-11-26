<p align="center">
  <img src="https://raw.githubusercontent.com/{{REPO}}/{{TAG}}/assets/png/nv-tuner-128.png" width="96" alt="nv-tuner">
</p>

<h2 align="center">nv-tuner {{TAG}}</h2>
<p align="center"><em>Ajuste automático do perfil do driver NVIDIA, por GPU e por jogo, sem você precisar medir nada.</em></p>

> **Status: pré-alfa.** Funciona, mas foi testado em pouquíssimas máquinas (uma GPU Turing, um driver): espere bugs. Tudo o que o app muda no driver pode ser desfeito com um clique.

## Novidades

{{NOVIDADES}}

## Instalação

1. Baixe **`{{MSI}}`** em *Assets*, logo abaixo.
2. (Opcional) Confira o SHA256, que deve ser `{{SHA256}}` (o mesmo do arquivo `.sha256`):
   ```powershell
   Get-FileHash .\{{MSI}} -Algorithm SHA256
   ```
3. Execute o MSI. O instalador **não é assinado**, então o Windows pode mostrar *"O Windows protegeu o computador"*: clique em **Mais informações → Executar assim mesmo**.
4. Aceite o UAC (só na instalação). Abra o **nv-tuner** pelo Menu Iniciar.

**Atualizando da {{TAG_ANTERIOR}}?** Basta instalar por cima: interface e serviço são atualizados juntos.

## Como desfazer

- **Remover o app:** *Configurações → Aplicativos*. O perfil do driver fica como está.
- **Remover e voltar o driver ao estado original** (PowerShell como administrador):
  ```powershell
  msiexec /x {{MSI}} RESTAURAR_PERFIL=1
  ```
- Ou use **Restaurar original** dentro do app.

## Requisitos

Windows 10/11 x64 · GPU NVIDIA com driver recente · as regras de DLSS só agem em jogos com DLSS.

---

**Changelog completo:** https://github.com/{{REPO}}/compare/{{TAG_ANTERIOR}}...{{TAG}}
