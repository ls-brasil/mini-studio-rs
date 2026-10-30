# Decisões do MVP

## Objetivo e escopo

Validar a ideia de uma versão leve do OBS Studio com captura de tela, janela e câmera. A prioridade é ter um fluxo funcional e simples, com interface em português, antes de investir em acabamento visual ou ampliar o produto.

A composição aceita uma tela **ou** janela como fonte principal e uma câmera opcional. A câmera ocupa a prévia quando está sozinha e pode aparecer sobreposta à fonte principal, com ajuste de canto, tamanho e espelhamento. O resultado pode ser salvo como imagem PNG.

Áudio, gravação de vídeo, transmissão, câmera virtual e múltiplas cenas não fazem parte desta etapa.

## Arquitetura

- **Aplicativo:** Tauri 2 com backend Rust; arquivos HTML, CSS e JavaScript em `src/`, sem bundler.
- **Tela/janela:** `xcap` enumera fontes e captura quadros no backend. O frontend solicita quadros sequencialmente, com alvo de 12 FPS, evitando acumular solicitações.
- **Transporte:** quadros JPEG pelo IPC do Tauri, limitados a 1280 × 720 e com proporção preservada.
- **Câmera:** `getUserMedia` no WebView2, solicitando somente vídeo e usando a permissão do usuário.
- **Composição:** canvas de 1280 × 720 (16:9) combina a fonte principal e a câmera. O PNG exportado corresponde à composição, não à resolução original do monitor.
- **Salvamento:** diálogo nativo para escolher o destino do PNG.

A abordagem de quadros individuais reduz o esforço inicial de integração. A taxa real depende da resolução, do computador e da fonte. Este MVP não promete captura de jogos ou vídeo de alta fluidez; uma continuidade pode exigir outro pipeline de captura e composição.

## Limitações conhecidas

- Mantenha a janela capturada aberta e restaurada. Janelas minimizadas ou fechadas deixam de ser fontes válidas.
- Conteúdo protegido e alguns aplicativos com aceleração gráfica podem falhar ou produzir imagem preta. Experimente capturar a tela nesses casos.
- Capturar a tela que contém o Mini Studio gera repetição da própria prévia. Outra tela ou uma janela específica evita esse efeito.
- A câmera depende das permissões do WebView2 e do Windows e pode estar indisponível quando outro aplicativo a utiliza.
- O alvo inicial é Windows. A disponibilidade multiplataforma das dependências não significa validação do aplicativo em macOS ou Linux.

## Checklist de validação manual

Os itens abaixo são um roteiro de validação, não um registro de testes já executados.

- [ ] Selecionar cada monitor disponível e confirmar que a prévia acompanha mudanças na tela.
- [ ] Selecionar uma janela, movimentá-la, redimensioná-la e confirmar a captura.
- [ ] Fechar ou minimizar a janela capturada e verificar a mensagem de erro e a possibilidade de selecionar outra fonte.
- [ ] Abrir a câmera sozinha e depois junto de uma tela/janela; verificar canto, tamanho e espelho.
- [ ] Negar a permissão ou desconectar a câmera e verificar se é possível tentar novamente.
- [ ] Trocar fontes, cancelar os seletores e remover fontes sem deixar capturas anteriores ativas.
- [ ] Salvar um PNG, abri-lo e conferir a composição e as dimensões; cancelar o diálogo de salvamento.
- [ ] Encerrar todas as capturas e confirmar que a câmera foi liberada.

Os comandos de build e testes automatizados documentados no README não substituem esta validação com dispositivos reais.

## Verificação realizada em 30/10/2026

- `cargo check`, `cargo fmt --check` e Clippy com `-D warnings`: aprovados.
- Rust: três testes unitários de JPEG, proporção, limites e validação PNG aprovados; dois testes opcionais de captura real (monitor e janela) aprovados nesta sessão Windows.
- Interface: seis testes aprovados, incluindo composição, payload PNG, troca de fontes, erro de captura, permissão negada, cancelamento durante conexão e encerramento dos tracks. O dispositivo de câmera é simulado pelo Edge e a ponte Tauri é substituída nesses testes.
- Aplicativo Tauri/WebView2: abertura da interface, captura real de monitor, troca para janela e composição com câmera simulada verificadas, sem erros JavaScript observados.
- Build de produção gerado com `cargo tauri build --no-bundle`. No executável release, a captura de janela mostrou 11–12 FPS em quatro amostras de um segundo, sem erros JavaScript. Essa medição é pontual nesta máquina, não um benchmark geral.
- Ainda pendentes: câmera física, múltiplos dispositivos, desconexão física e interação manual com o diálogo de salvamento.

Em debug, o teste nativo levou cerca de 430 ms por captura/codificação/decodificação; a prévia integrada ficou próxima de 2 FPS durante a compilação em paralelo. O limite de 12 FPS não é uma garantia. Use o build de produção para avaliar desempenho.

Referências técnicas: [XCap](https://github.com/nashaofu/xcap), [resposta binária do Tauri](https://docs.rs/tauri/latest/tauri/ipc/struct.Response.html) e [captura de câmera com getUserMedia](https://developer.mozilla.org/en-US/docs/Web/API/MediaDevices/getUserMedia).
