# Instalação, perfis e BIOS — 2026-11-06

- Instalações com seleção local e download estável/nightly; histórico recente e busca de tag exata no repositório oficial PCSX2/pcsx2. “Escolher versão” significa release oficial escolhido pelo usuário, não instalação de forks arbitrários.
- Removidos catálogo de fallback com URLs fixas e ativação aproximada por canal; ativação de release exige executável associado à versão.
- Perfis com formulário sob demanda e caminhos recolhidos; diagnóstico em lista com verificação manual.
- Capas integradas à tela de instalações, sem aba dedicada.
- BIOS com aviso curto dispensável durante a sessão, ativa primeiro e sem botão de aplicar; hash fora da interface principal. Tutorial resumido e link para documentação oficial do PCSX2.
- Cache de leituras PCSX2 em memória durante a sessão do aplicativo, deduplicando requisições simultâneas; alterações bem-sucedidas invalidam o cache. Atualizar permite reler arquivos modificados externamente. Falhas não ficam armazenadas.
- Instalação falha explicitamente quando download não produz pacote, em vez de acionar fallback simulado. Pacotes devem vir de releases oficiais no GitHub e identificadores de versão não podem conter caminhos.

Validação: testes de cache com fetch simulado, verificações frontend existentes atualizadas para a nova apresentação e build TypeScript/Vite. Consultados guia oficial de BIOS e nomes dos assets via GitHub. Instalação real de pacote e inspeção visual da interface não executadas nesta alteração; não modificar instalações pessoais para testar.

Resultado final: 21 testes frontend e 21 testes focados de gerenciamento PCSX2 passaram; build TypeScript/Vite e cargo check Tauri passaram. A suíte Rust completa não foi concluída: houve conflito de linker com uma execução anterior ainda ativa; a execução ampla foi interrompida e os testes pertinentes foram executados isoladamente com sucesso. O diff global aponta uma linha em branco preexistente em types.ts, arquivo não alterado nesta tarefa.
