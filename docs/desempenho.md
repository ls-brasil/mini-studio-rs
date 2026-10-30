# Validação de desempenho nativo

O objetivo é pelo menos 30 FPS com baixo consumo de CPU. A versão atual usa interface Rust/egui, GPU via wgpu/Direct3D 12, Windows Graphics Capture e câmera Media Foundation. Não há resultados de benchmark registrados para esta migração. A validação será realizada pelo usuário.

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
