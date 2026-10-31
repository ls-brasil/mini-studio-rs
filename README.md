# Mini Studio 1.0.0

Seu espaço para criar e apresentar. Um estúdio nativo para Windows que combina tela, janela, câmera e imagem, com prévia ao vivo, exportação PNG e câmera virtual.

![Interface do Mini Studio 1.0.0](docs/screenshots/studio.png)

## Instalar e começar

Abra **`dist/MiniStudio-1.0.0-Setup.msi`**. O instalador em português cria os atalhos, instala o aplicativo e configura a câmera virtual no Windows 11. A instalação pede administrador; o aplicativo roda normalmente, sem elevação.

Também há **`dist/MiniStudio-1.0.0-portable.zip`**: extraia e abra `mini-studio-rs.exe`. A câmera virtual exige o componente instalado pelo MSI. Os hashes dos pacotes ficam em `dist/SHA256SUMS-1.0.0.txt`.

1. No rodapé, use **+** em **Cenas** para criar uma composição vazia e dar um nome.
2. Use **+** em **Fontes** para adicionar telas, janelas, câmeras e imagens. Combine várias fontes, inclusive do mesmo tipo.
3. Arraste qualquer fonte na prévia para posicioná-la; arraste seu canto inferior direito para redimensionar. A lista mostra as fontes da frente para trás; use **↑ / ↓** para mudar a ordem.
4. O olho mostra ou oculta e o cadeado bloqueia o movimento na prévia. **Propriedades** (ou dois cliques na fonte) abre nome, posição, tamanho, espelhamento e opacidade.
5. Em **Controles**, salve a cena, exporte uma imagem PNG ou inicie a câmera virtual. No aplicativo consumidor, escolha **Mini Studio RS Cam**.

O [guia do usuário](docs/Guia-do-usuario.txt) acompanha os pacotes da versão 1.0; o fluxo de cenas descrito abaixo corresponde ao código atual do projeto. **Ajuda** ou **F1** abre orientações e opções de tamanho da interface.

## Cenas e fontes

A prévia ocupa a parte superior. **Cenas**, **Fontes** e **Controles** ficam em painéis no rodapé, cuja altura pode ser ajustada arrastando a divisória superior. As propriedades abrem em um diálogo, sem ocupar as laterais.

Cada cena guarda uma lista ordenada de fontes, com nomes, visibilidade, bloqueio e transformação independentes. Não existe mais a restrição de uma fonte por tipo: uma cena pode misturar várias imagens, telas, janelas e câmeras.

- Clique em uma cena para alternar. Suas fontes são abertas e as da cena anterior são liberadas. A câmera virtual continua ligada, se estiver ativa.
- **Salvar cena** grava imediatamente. As alterações também são salvas automaticamente e ao fechar o aplicativo. **Salvar imagem** exporta PNG.
- No menu **•••** de Cenas, renomeie ou duplique. A lixeira exclui a cena selecionada; a última cena é mantida.
- Para repetir uma fonte na composição, selecione-a e use **+ → Duplicar fonte selecionada** ou **Ctrl+D**. As duas camadas têm ajustes independentes e compartilham a captura do dispositivo.
- Ao reabrir o aplicativo, escolha **Ativar fontes**. Fontes indisponíveis permanecem na configuração e mostram o erro ao passar o mouse. Em Propriedades, reconecte ou escolha outro arquivo/dispositivo.

As cenas ficam em `%LOCALAPPDATA%\MiniStudioRS\scenes.json`. A estrutura anterior é migrada para a lista de fontes, preservando os nomes, referências e visibilidade. Imagens são referenciadas pelo caminho original: mantenha os arquivos nesse local. Telas e janelas são localizadas novamente pelo nome; se o título mudar ou houver mais de uma correspondência, selecione a fonte outra vez.

## Recursos

- Interface escura com tipografia do Windows, ícones vetoriais, controles acessíveis por teclado e painéis que se adaptam a janelas menores.
- Composição de múltiplas telas, janelas, câmeras e imagens em 1280 × 720.
- Ordem livre das camadas, transparência PNG, opacidade, posição, tamanho, espelhamento e bloqueio por fonte.
- Prévia, PNG e câmera virtual compartilham a mesma ordem e geometria das fontes. Guias e seleção aparecem somente no editor.
- Saída virtual NV12 a 30 FPS, com início e parada independentes das fontes e indicação de atividade.
- PNG/JPEG por seletor, arrastar e soltar ou `--image "C:\imagens\arte.png"`; leitura em segundo plano e preservação da imagem anterior em caso de erro.
- Busca no seletor de dispositivos/janelas, atualização da lista, estados de carregamento e mensagens de erro.
- Cenas e preferências salvas por usuário. As fontes da cena são ativadas por você ao reabrir o aplicativo.

| Atalho | Ação |
| --- | --- |
| Ctrl+O | Abrir imagem |
| Ctrl+S | Salvar cena |
| Ctrl+Shift+S | Exportar composição PNG |
| Ctrl+D | Duplicar fonte selecionada |
| Del | Remover fonte selecionada |
| F1 | Ajuda e escala da interface |
| Esc | Fechar diálogo ou desmarcar fonte |
| Tab / Enter / Espaço | Navegar e ativar controles |

Áudio, gravação de vídeo e transmissão pela internet não estão disponíveis. Imagens aceitam até 32 MiB, 16 milhões de pixels e 8192 pixels por lado. A saída é fixa em 720p.

## Requisitos e funcionamento

**Aplicativo:** Windows 10 1903/build 18362 ou superior, x64, com GPU/driver Direct3D 12. **Câmera virtual:** Windows 11 build 22000 ou superior ([requisito da Microsoft](https://learn.microsoft.com/en-us/windows/win32/api/mfvirtualcamera/nf-mfvirtualcamera-mfcreatevirtualcamera)).

A captura utiliza Windows Graphics Capture; a câmera utiliza Media Foundation. Interface Rust/egui/eframe, GPU via wgpu/Direct3D 12. Não requer Node.js, WebView2 ou .NET no computador do usuário.

Prévia parada pode indicar conteúdo estático: o contador mede quadros **novos**. A saída virtual possui relógio próprio e continua enviando a composição. Capture outra tela ou uma janela específica para evitar a repetição da própria prévia. Janelas minimizadas e conteúdo protegido podem interromper a captura.

A câmera virtual usa registro COM próprio e `%ProgramData%\MiniStudioRS\vcam-frames.bin`. Apenas um produtor publica por vez. No MSI, o usuário que cria o arquivo, administradores e o serviço de câmera recebem acesso; em computadores compartilhados, consulte o guia para trocar o produtor. Preferências ficam em `%LOCALAPPDATA%\MiniStudioRS\preferences.json`.

Para atualizar ou desinstalar, feche o aplicativo e seus consumidores. Use o MSI da nova versão ou **Configurações → Aplicativos → Mini Studio**. Se o Windows mantiver o componente em uso, reinicie antes de tentar novamente.

## Desenvolvimento e distribuição

Requisitos de desenvolvimento: Rust MSVC, Visual Studio Build Tools C++, Windows SDK, CMake. O empacotamento também usa Python 3 e .NET SDK para executar WiX 5.0.2; essas ferramentas não são dependências do aplicativo distribuído.

```powershell
cargo run
cargo fmt -- --check
cargo test
cargo clippy --all-targets -- -D warnings
cargo build --release
.\scripts\build-vcam.ps1
.\scripts\release.ps1
```

`release.ps1` verifica formatação, testes e Clippy, compila o aplicativo e o componente nativo, reúne os avisos de terceiros e gera MSI, ZIP e SHA-256 em `dist`. A versão vem de `Cargo.toml` e é incorporada ao EXE, à DLL e ao instalador. `-SkipBuild` reutiliza binários já compilados, conferindo suas versões. `package.json` contém atalhos opcionais para esses comandos.

Para desenvolvimento do componente, `scripts/install-vcam.ps1` e `scripts/uninstall-vcam.ps1` continuam disponíveis e exigem PowerShell como administrador. Para usuários finais, use o MSI. Não misture os dois mecanismos de instalação/remoção.

## Validação

Há testes da composição, transparência, imagens inválidas, conversão NV12, preferências, interação da interface e transporte concorrente. Testes de dispositivos reais são opt-in:

```powershell
cargo test installed_virtual_camera_round_trip -- --ignored --nocapture
cargo test installed_virtual_camera_ffmpeg -- --ignored --nocapture
cargo test benchmark_real_monitor_stream -- --ignored --nocapture
```

O primeiro exige o componente instalado; o segundo também exige FFmpeg no PATH. Para o teste contínuo, defina `$env:MINISTUDIO_VCAM_TEST_SECONDS='600'` antes de executar o primeiro comando.

Resultados e limites: [validação da 1.0](docs/validacao-1.0.0.md), [desempenho](docs/desempenho.md) e [arquitetura de imagens/câmera virtual](docs/imagens-e-camera-virtual.md). As [decisões do MVP](docs/mvp.md) são somente histórico.
