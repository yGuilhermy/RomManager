# Tema do RomManager

## Seleção

Abas, navegação e filtros de escolha exclusiva usam o mesmo tratamento: texto azul (`--selection-color`), superfície discreta (`--selection-surface`) e borda inferior azul de 2px com cantos inferiores arredondados. Os valores ficam em `App.css` e derivam do accent atual.

Aplicado à navegação lateral, abas de instalação/perfis, catálogo, detalhes do jogo, perfil, visualização grade/lista, ordenação e filtro alfabético. Estados existentes `.active`, `aria-selected=true` e `aria-pressed=true` continuam controlando a seleção.

Novos controles podem usar `theme-selection` com o estado semântico apropriado. Manter nome acessível, foco visível e comportamento de teclado conforme o tipo de controle. Não usar role tab sem implementar o comportamento de tabs correspondente.

Hover é discreto; seleção persistente tem borda inferior. Foco tem contorno próprio. Transições respeitam prefers-reduced-motion. Botões de ação, badges e cartões de conteúdo não usam esse tratamento por não representarem a mesma interação.
