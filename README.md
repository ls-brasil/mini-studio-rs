# Mini Studio

MVP nativo para Windows, inspirado no OBS Studio. Interface em Rust com egui/eframe e renderização pela GPU via wgpu/Direct3D 12. Não usa Tauri, WebView2, HTML ou JavaScript no aplicativo atual.

## Executar a versão compilada

Abra `target/release/mini-studio-rs.exe` ou execute na raiz:

```powershell
.\target\release\mini-studio-rs.exe
```

## Validar o MVP

1. Clique em **Capturar tela**, escolha um monitor e confira a prévia com conteúdo em movimento.
2. Troque para **Capturar janela**, selecione uma janela aberta e experimente mover e redimensionar seu conteúdo. Minimize e restaure para verificar a retomada.
3. Adicione uma câmera. Teste câmera sozinha e sobreposta à tela/janela, incluindo tamanho, canto e espelhamento.
4. Use **Salvar imagem PNG**, abra o arquivo e confira se corresponde à composição.
5. Remova fontes ou clique em **Parar tudo** e confira se a câmera foi liberada.

A composição aceita uma tela **ou** janela e uma câmera, com saída PNG de 1280 × 720. Áudio, gravação de vídeo, transmissão e câmera virtual ficam fora deste MVP.

Para avaliar fluidez, mantenha animação ou vídeo em movimento na fonte. O indicador conta quadros novos; uma fonte parada pode enviar menos quadros. Observe também CPU e memória do `mini-studio-rs.exe` no Gerenciador de Tarefas. O objetivo é pelo menos 30 FPS com baixo consumo de CPU; o desempenho desta versão fica para sua validação.

Capturar o monitor que contém o próprio Mini Studio gera repetição da prévia. Outra tela ou uma janela específica evita esse efeito. A câmera precisa estar habilitada nas permissões do Windows e disponível para uso.

## Compilar e desenvolver

Requisitos: Windows 10 versão 1903/build 18362 ou superior, GPU/driver com Direct3D 12, Rust MSVC e Visual Studio Build Tools com C++ e Windows SDK. A captura de janela/monitor usa Windows Graphics Capture; o mínimo dessas APIs consta na [documentação da Microsoft](https://learn.microsoft.com/en-us/windows/win32/api/windows.graphics.capture.interop/nf-windows-graphics-capture-interop-igraphicscaptureiteminterop-createforwindow).

Na raiz do projeto:

```powershell
cargo run
cargo build --release
```

`cargo run` usa o perfil de desenvolvimento otimizado. O executável de produção fica em `target/release/mini-studio-rs.exe`. Node.js, npm e pnpm são opcionais; `package.json` contém apenas atalhos para Cargo, sem dependências JavaScript.

| Atalho npm/pnpm | Comando |
| --- | --- |
| `run dev` | `cargo run` |
| `run build` | `cargo build --release` |
| `run build:debug` | `cargo build` |
| `run check` | `cargo check` |
| `test` | `cargo test` |
| `run test:capture` | Teste opcional de captura real na sessão Windows |
| `run lint` | `cargo clippy --all-targets -- -D warnings` |
| `run fmt` | `cargo fmt` |
| `run fmt:check` | `cargo fmt -- --check` |
| `run benchmark -- ...` | `cargo run --release -- ...` |

Consulte [as decisões do MVP](docs/mvp.md) e [a medição opcional de desempenho](docs/desempenho.md). Os comandos de teste estão disponíveis para uso posterior; a validação funcional desta migração será feita pelo usuário.
