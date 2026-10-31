# Mini Studio

Estúdio nativo para Windows, inspirado no OBS Studio. Interface em Rust com egui/eframe e renderização pela GPU via wgpu/Direct3D 12. Não usa Tauri, WebView2, HTML ou JavaScript no aplicativo atual.

## Executar a versão compilada

Abra `target/release/mini-studio-rs.exe` ou execute na raiz:

```powershell
.\target\release\mini-studio-rs.exe
```

## Usar o Mini Studio

1. Clique em **Capturar tela**, escolha um monitor e confira a prévia com conteúdo em movimento.
2. Troque para **Capturar janela**, selecione uma janela aberta e experimente mover e redimensionar seu conteúdo. Minimize e restaure para verificar a retomada.
3. Adicione uma câmera. Teste câmera sozinha e sobreposta à tela/janela, incluindo tamanho, canto e espelhamento.
4. Clique em **Adicionar imagem** para abrir PNG/JPEG. Escolha fundo ou sobreposição, ajuste tamanho/posição e use **Mostrar imagem** para ocultar sem remover.
5. Use **Salvar imagem PNG**, abra o arquivo e confira se corresponde à composição.
6. No Windows 11, instale o componente abaixo e clique em **Iniciar câmera virtual**. Selecione **Mini Studio RS Cam** no aplicativo consumidor (o Windows pode acrescentar “Windows Virtual Camera” ao nome).
7. Remova fontes ou clique em **Parar tudo** para encerrar capturas, imagens e saída virtual.

A composição aceita uma tela **ou** janela, uma imagem e uma câmera. PNG e câmera virtual usam a mesma composição de 1280 × 720, sem os controles da interface. Áudio, gravação de vídeo, transmissão e múltiplas cenas continuam fora do escopo.

Imagens: PNG/JPEG de até 32 MiB, 16 milhões de pixels e 8192 pixels por lado. A leitura ocorre em segundo plano, preserva transparência e orientação EXIF, e reduz a imagem uma única vez para caber na saída. Também é possível abrir com `mini-studio-rs.exe --image "C:\imagens\arte.png"`.

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

| Atalho npm/pnpm        | Comando                                          |
| ---------------------- | ------------------------------------------------ |
| `run dev`              | `cargo run`                                      |
| `run build`            | `cargo build --release`                          |
| `run build:debug`      | `cargo build`                                    |
| `run check`            | `cargo check`                                    |
| `test`                 | `cargo test`                                     |
| `run test:capture`     | Teste opcional de captura real na sessão Windows |
| `run lint`             | `cargo clippy --all-targets -- -D warnings`      |
| `run fmt`              | `cargo fmt`                                      |
| `run fmt:check`        | `cargo fmt -- --check`                           |
| `run benchmark -- ...` | `cargo run --release -- ...`                     |

## Câmera virtual — Windows 11 x64

Compile o aplicativo com `cargo build --release` e o componente com:

```powershell
.\scripts\build-vcam.ps1
```

O componente exige CMake, ferramentas C++ do Visual Studio e Windows SDK 11. O script compila a DLL e executa dois testes nativos. Em **PowerShell x64 como administrador**, instale uma vez:

```powershell
.\scripts\install-vcam.ps1
```

Depois, execute o aplicativo normalmente. **Iniciar câmera virtual** publica a composição a 30 FPS, inclusive com imagem estática e sem fontes. **Parar câmera virtual** mantém as fontes abertas. A câmera existe durante a sessão e é removida ao parar/fechar o aplicativo. A própria saída não aparece no seletor de câmera física.

A DLL é instalada em `%ProgramFiles%\MiniStudioRS`, com registro COM próprio. O transporte fica em `%ProgramData%\MiniStudioRS`, com escrita restrita ao usuário configurado na instalação. Caso a elevação use outra conta, passe `-ProducerSid` com o SID do usuário que executará o aplicativo (`whoami /user`). Apenas uma instância pode publicar por vez.

Para remover, feche o Mini Studio e os consumidores e execute como administrador:

```powershell
.\scripts\uninstall-vcam.ps1
```

Se o Frame Server ainda mantiver a DLL em uso, reinicie o Windows e repita a remoção. Instalação, atualização e remoção não encerram serviços do Windows automaticamente.

A câmera virtual depende de Windows 11 build 22000 ou superior, conforme [MFCreateVirtualCamera](https://learn.microsoft.com/en-us/windows/win32/api/mfvirtualcamera/nf-mfvirtualcamera-mfcreatevirtualcamera). O restante do aplicativo mantém o requisito de Windows 10. O carregamento da DLL é opcional; sua ausência gera orientação na interface.

## Validação

`cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt -- --check` e os testes nativos verificam composição, carregamento, conversão, transporte e consumo por Media Foundation. Os testes de câmera física e captura real são opcionais.

A descoberta do dispositivo pelo Frame Server após instalação, compatibilidade com aplicativos de reunião e uso prolongado ainda precisam de validação na máquina de destino. Consulte [imagens e câmera virtual](docs/imagens-e-camera-virtual.md), [as decisões do MVP](docs/mvp.md) e [a medição de desempenho](docs/desempenho.md).
