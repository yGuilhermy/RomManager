# RomManager

<p align="center">
  <img src="apps/desktop/src/assets/logo.png" alt="Logo do RomManager" width="120" />
</p>

<p align="center">
  <strong>Sua biblioteca de PlayStation 2, organizada no PC e pronta para abrir no PCSX2.</strong>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Windows-0078D6?style=for-the-badge" alt="Windows" />
  <img src="https://img.shields.io/badge/Rust-000000?style=for-the-badge&logo=rust&logoColor=white" alt="Rust" />
  <img src="https://img.shields.io/badge/Tauri-FFC107?style=for-the-badge&logo=tauri&logoColor=black" alt="Tauri" />
  <img src="https://img.shields.io/badge/React-20232A?style=for-the-badge&logo=react&logoColor=61DAFB" alt="React" />
  <img src="https://img.shields.io/badge/TypeScript-007ACC?style=for-the-badge&logo=typescript&logoColor=white" alt="TypeScript" />
  <img src="https://img.shields.io/badge/SQLite-07405E?style=for-the-badge&logo=sqlite&logoColor=white" alt="SQLite" />
</p>

## O projeto

O RomManager reúne a biblioteca local de jogos de PlayStation 2, um catálogo de títulos e a integração com o PCSX2. Você pode sincronizar a pasta de ROMs, encontrar capas, consultar descrições e abrir um jogo no emulador pela própria interface.


## Como instalar e usar

Para gerar uma versão local, siga as instruções de [compilação](#compilando-o-projeto). O script de release coloca o executável `RomManager_portable.exe` e os instaladores MSI produzidos na pasta `releases/`.

1. Execute a versão portable ou instale o pacote MSI gerado.
2. Abra **Configurações** e informe a pasta onde ficam suas ROMs.
3. Configure o caminho do executável do PCSX2, como `pcsx2-qt.exe`.
4. Se já tiver uma pasta de capas, informe seu caminho na seção de capas.
5. Na **Biblioteca**, clique em **Sincronizar Pasta** para indexar os jogos.
6. Abra os detalhes de um jogo instalado e selecione **Iniciar no PCSX2**.
7. Use **Regenerar capas** na biblioteca ou nas configurações para atualizar as associações. Na aba **Perfil**, registre o progresso e faça backup da coleção.

O PCSX2 precisa estar instalado e configurado separadamente. O RomManager não inclui o emulador nem a BIOS.

## Funcionalidades principais

- **Biblioteca local:** indexação de ROMs nos formatos CHD, ISO, CSO, BIN e GZ, com busca, ordenação, filtros de região e formato e visualização em grade ou lista.
- **Catálogo PS2:** catálogo local com 4.427 entradas, pesquisa por título, índice alfabético, paginação e ordenação por popularidade, nome ou tamanho, conforme a tela.
- **Integração com PCSX2:** inicialização de jogos instalados e acesso à pasta do arquivo pelo aplicativo.
- **Regeneração de capas:** reassociação de imagens locais e nova busca das capas automáticas geradas pelo RomManager, com progresso e preservação de imagens pessoais.
- **Capas e informações:** associação de imagens locais, busca de capas por serial ou título, descrições da Wikipédia e consulta à base de compatibilidade incluída no projeto.
- **Downloads e fila inteligente:** acompanhamento de progresso e velocidade, cancelamento durante a transferência, extração de ZIP, 7z e RAR, suporte a download em lote com modo de seleção múltipla, retenção entre páginas, barra de cálculo de tamanho e limite configurável de 1 a 5 downloads simultâneos na fila.
- **Perfil e coleção:** foto de avatar personalizada persistida localmente, favoritos com busca e remoção, estatísticas em tempo real da biblioteca (espaço total ocupado, média por jogo e regiões ativas) e histórico de ações locais.
- **Backup do perfil:** exportação e importação em JSON com prévia, validação e mesclagem transacional. Os identificadores de jogos podem conter caminhos locais; ROMs e imagens não são incluídas.
- **Pesquisa de traduções:** consulta adicional ao CDRomance, limitada a seis títulos por busca. A disponibilidade depende do serviço externo.

### Cuidados com os arquivos

Os downloads usam diretórios temporários separados e verificam os intervalos e tamanhos recebidos do servidor. A instalação não sobrescreve uma ROM existente. A extração valida os caminhos dos arquivos e limita o conteúdo declarado a 64 GiB; aguarde a extração e a instalação terminarem antes de remover um item.

**Excluir um jogo apaga a ROM do disco, sem passar pela Lixeira.** As capas são preservadas porque podem ser compartilhadas por outros jogos.

## Compilando o projeto

### Pré-requisitos

- Windows com WebView2 Runtime.
- Node.js 22 e npm.
- Rust stable com Cargo e o toolchain MSVC.
- Visual Studio Build Tools com ferramentas de compilação C++ e Windows SDK.
- PCSX2 instalado, para iniciar os jogos pela interface.

### Executar em desenvolvimento

Na pasta do projeto, instale as dependências do desktop:

```powershell
cd apps/desktop
npm.cmd ci
npm.cmd run tauri dev
```

Após instalar as dependências, também é possível iniciar pela raiz:

```powershell
.\dev.bat
```

O comando do Tauri inicia a interface e o core local. Para trabalhar apenas na interface, use `npm.cmd run dev` em `apps/desktop`; os recursos da biblioteca e dos downloads precisam do backend em execução.

### Gerar executável e instaladores

Na raiz do projeto:

```powershell
.\build_releases.bat
```

O script compila o desktop e copia o executável e os pacotes MSI disponíveis para `releases/`. Também é possível executar a compilação diretamente:

```powershell
cd apps/desktop
npm.cmd run tauri build
```

Nesse caso, os artefatos ficam em `apps/desktop/src-tauri/target/release/`, com os instaladores em `bundle/`.

## Dados locais e conexão

O banco SQLite fica em `%APPDATA%\RomManager\local.db`. As ROMs e capas ficam nas pastas configuradas pelo usuário.

A API atende em `127.0.0.1:3000`, com origens restritas ao desktop. A porta 3000 precisa estar livre.

## Créditos e links úteis

| Projeto ou fonte | Uso no RomManager |
| --- | --- |
| [GIMI Mod Manager](https://github.com/yGuilhermy/GIMIModManager) | Projeto original que deu origem ao fork e referência deste README. |
| [Internet Archive](https://archive.org/) | Acervos públicos 1G1R de ROMs de PS2 preservadas em formato .CHD. |
| [PCSX2](https://pcsx2.net/) | Emulador usado para iniciar os jogos. |
| [PCSX2 Wiki](https://wiki.pcsx2.net/) | Referência para informações e compatibilidade dos títulos. |
| [ps2-covers, de xlenore](https://github.com/xlenore/ps2-covers) | Fonte de capas identificadas por serial. |
| [libretro-thumbnails](https://github.com/libretro-thumbnails/Sony_-_PlayStation_2) | Fonte de capas pesquisadas pelo nome do jogo. |
| [Wikipédia](https://www.wikipedia.org/) | Fonte de descrições dos jogos. |
| [CDRomance](https://cdromance.org/) | Fonte da pesquisa adicional de traduções e versões modificadas. |

Use ROMs, BIOS e outros arquivos que você tenha direito de utilizar. Os projetos e serviços citados mantêm suas próprias licenças e condições de uso.


## Licença

O código do RomManager é distribuído sob a [licença MIT](LICENSE), com atribuição a yGuilhermy e aos contribuidores.

### Licenças e componentes de terceiros

- **Tauri &amp; WebView2**: Licença Apache 2.0 / MIT.
- **React &amp; Vite**: Licença MIT.
- **Axum, Tokio &amp; Reqwest**: Licença MIT.
- **SQLite (rusqlite)**: Domínio Público / MIT.
- **PCSX2**: Licença GPL v3 (emulador executado externamente, não empacotado no app).
