# Validação de desempenho nativo

O objetivo é pelo menos 30 FPS com baixo consumo de CPU. A versão atual usa interface Rust/egui, GPU via wgpu/Direct3D 12, Windows Graphics Capture e câmera Media Foundation. A avaliação completa de CPU/GPU, captura real e uso prolongado será realizada pelo usuário. A verificação curta da saída virtual está registrada abaixo.

## Validação simples

Abra `target/release/mini-studio-rs.exe`, selecione uma tela ou janela com conteúdo em movimento e observe o indicador de FPS. No Gerenciador de Tarefas, confira CPU e memória do processo. Repita com câmera e com a composição das duas fontes.

Mantenha fonte, resolução, duração e tamanho da prévia iguais ao comparar versões. Não compile em paralelo durante a medição. Conteúdo parado pode gerar poucos quadros; isso não prova falta de capacidade de processamento.

## Relatório opcional do aplicativo

Com uma janela de teste aberta e animação contínua:

```powershell
cargo run --release -- --benchmark-window "Mini Studio Benchmark" --seconds 15 --report target/benchmark-native.json
```

Ou use o executável já compilado:

```powershell
.\target\release\mini-studio-rs.exe --benchmark-window "Mini Studio Benchmark" --seconds 15 --report target/benchmark-native.json
```

O título pode ser um trecho que identifique a janela. O diretório do relatório deve existir. O arquivo JSON contém adaptador gráfico, duração, quadros apresentados, FPS, quadros pulados e percentis dos intervalos entre quadros. Esse relatório não mede CPU; use o Gerenciador de Tarefas ou outra ferramenta do sistema para essa parte.

O contador apresentado mede processamento de quadros novos pelo aplicativo, não a comprovação física de cada atualização no monitor. A câmera e a composição simultânea precisam de validação própria.

## Teste opcional do backend

```powershell
cargo test --release benchmark_real_monitor_stream -- --ignored --nocapture
```

Exige uma sessão Windows interativa. A medição do backend não inclui composição da interface e não substitui a avaliação do executável final.

## Histórico

O roteiro anterior utilizava Playwright/CDP para medir o frontend Tauri e somava o processo do aplicativo à árvore WebView2. Esse procedimento e suas métricas JavaScript não se aplicam ao aplicativo nativo atual. Os antigos números de 11–12 FPS pertencem ao pipeline inicial com JPEG e não são resultados desta versão.

## Verificação automatizada da expansão — 31/10/2026

Ambiente: Windows build 26300.9818, MSVC 19.51, Windows SDK 10.0.26100.0, build C++ Release x64.

- `cargo test`: 11 testes passaram; 3 testes de dispositivos/captura real permaneceram ignorados.
- `cargo clippy --all-targets -- -D warnings`: sem avisos.
- `cargo build --release`: concluído. Aplicativo aberto com PNG transparente via `--image`; prévia e controles conferidos visualmente na captura `target/image-source-ui.png`. O processo de teste foi fechado normalmente.
- `vcam_transport_test`: leitura/escrita concorrentes sem mistura de pixels, exclusão de segundo produtor, invalidação de quadro após 500 ms e reinício passaram.
- `vcam_media_test`: 31 amostras NV12 de 1280 × 720 consumidas por `IMFSourceReader`, cerca de **29,62 FPS** na execução registrada. Conferiu pixels e mudança de conteúdo, tamanho de buffer, timestamps, duração e liberação COM. A janela é curta, inclui inicialização e não mede CPU/GPU nem estabilidade prolongada.

O teste nativo instancia a media source no próprio processo e usa arquivo temporário. Não valida a enumeração do dispositivo instalado pelo Frame Server, aplicativos de reunião nem a composição Rust completa a 30 FPS. Nenhuma instalação elevada foi executada.

Para validar a entrega, compare imagem estática, tela/janela em movimento e combinação com câmera. Registre FPS publicados (indicador próprio da saída), FPS recebidos no consumidor, CPU/GPU/memória por 10 minutos e nomes/versões de dois consumidores. O FPS da prévia mede quadros novos; ele pode ficar em zero com uma imagem estática enquanto a saída virtual repete quadros regularmente.
