# Mini Studio

MVP de um estúdio de captura leve, inspirado no OBS Studio, feito com Rust, Tauri 2 e JavaScript. O objetivo desta versão é validar a captura e a composição de fontes no Windows.

## Recursos

- Captura de uma tela ou de uma janela, com seleção da fonte.
- Captura de uma câmera, sozinha ou sobreposta à tela/janela.
- Ajustes de canto, tamanho e espelhamento da câmera.
- Prévia da composição em 1280 × 720 (16:9).
- Salvamento da composição em PNG, com diálogo para escolher o arquivo.
- Remoção individual de fontes e encerramento de todas as capturas.

Há no máximo uma tela/janela e uma câmera ativas por vez. Áudio, gravação de vídeo, transmissão, câmera virtual e gerenciamento de cenas ficam fora deste MVP.

## Executar

Pré-requisitos no Windows:

- Rust com toolchain MSVC.
- Visual Studio Build Tools com ferramentas de C++ e Windows SDK.
- Microsoft Edge WebView2 Runtime.
- Tauri CLI 2, instalável com `cargo install tauri-cli --version "^2" --locked`.

Na raiz do projeto:

```powershell
cargo tauri dev
```

### Atalhos do package.json

Com Node.js 20 ou superior, use `npm install` (ou `pnpm install`) para instalar as ferramentas de teste. O aplicativo continua sem depender de um build JavaScript.

| Comando | Ação |
| --- | --- |
| `npm run dev` | Inicia o aplicativo com recarga durante o desenvolvimento |
| `npm run build` | Gera o executável de produção, sem instalador |
| `npm run build:debug` | Compila o executável de desenvolvimento |
| `npm run check` | Verifica a sintaxe JavaScript e compila a checagem Rust |
| `npm test` | Executa os testes da interface e os testes unitários Rust |
| `npm run test:ui` | Testa composição e ciclo de vida das fontes no Edge headless |
| `npm run test:rust` | Executa apenas os testes Rust |
| `npm run test:capture` | Testa captura real de monitor e janela na sessão Windows atual |
| `npm run lint` | Executa Clippy sem permitir avisos |
| `npm run fmt` | Formata o código Rust |
| `npm run fmt:check` | Confere a formatação Rust |

Os mesmos scripts funcionam com `pnpm` no lugar de `npm`. Os testes da interface exigem Microsoft Edge instalado; usam câmera simulada e substituem somente a ponte nativa de captura/salvamento. Os testes `test:capture` exigem desktop interativo com monitor e uma janela aberta.

Para compilar o executável de produção sem gerar instalador:

```powershell
cargo tauri build --no-bundle
```

O executável fica em `src-tauri/target/release/mini-studio-rs.exe`. O frontend é servido diretamente de `src/`, sem etapa de build com npm.

## Usar

1. Adicione uma tela ou janela e selecione a fonte desejada.
2. Adicione uma câmera e permita seu uso quando solicitado. Também é possível começar apenas com a câmera.
3. Ajuste a sobreposição da câmera e confira a prévia.
4. Salve uma imagem PNG ou encerre as capturas quando terminar.

A captura da tela que contém o próprio Mini Studio produz o efeito de repetição da prévia. Para evitá-lo, capture outra tela ou uma janela específica.

## Verificação

```powershell
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
```

Consulte [as decisões do MVP, limitações e checklist manual](docs/mvp.md) para validar dispositivos e permissões reais. O alvo inicial de validação é Windows.
