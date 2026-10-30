# Decisões do MVP

## Escopo

Validar uma versão leve do OBS Studio com captura de uma tela ou janela e uma câmera. A prioridade é funcionamento, fluidez e consumo de recursos. A composição permite câmera sozinha ou sobreposta, com ajustes de canto, tamanho e espelhamento, e salvamento PNG em 1280 × 720.

Áudio, gravação de vídeo, transmissão, câmera virtual e múltiplas cenas não fazem parte desta etapa.

## Expansão definida após o MVP

Em 30/10/2026, foram adicionados ao escopo futuro **imagens como fonte** e **saída por câmera virtual**. Eles ainda não estão implementados e não alteram a descrição das funcionalidades atuais acima. O plano está em [Imagens e câmera virtual](imagens-e-camera-virtual.md), com requisitos propostos e critérios para validação pelo usuário.

Áudio, gravação, transmissão e múltiplas cenas continuam sem compromisso de implementação nesta expansão.

## Arquitetura atual: aplicativo nativo

- **Interface:** Rust, egui/eframe, com renderização por wgpu/Direct3D 12. O aplicativo não inicia WebView ou runtime JavaScript.
- **Tela/janela:** Windows Graphics Capture mantém uma sessão por fonte. Somente o quadro mais recente é disponibilizado ao consumidor, evitando fila de quadros antigos.
- **Preparação:** redução para caber em 1280 × 720, preservando proporção, com redimensionamento SIMD via `fast_image_resize`.
- **Câmera:** captura nativa pelo Windows Media Foundation, sem `getUserMedia` ou permissão do navegador.
- **Prévia:** texturas e composição na GPU. O aplicativo solicita atualização quando há quadros ou interação; o alvo da captura é 30 FPS.
- **PNG:** composição em CPU apenas ao salvar, usando as mesmas funções de geometria da prévia. O arquivo tem fundo opaco e resolução 1280 × 720; o seletor de destino é nativo.
- **Build:** `Cargo.toml` na raiz; `cargo run` e `cargo build --release`. O `package.json` oferece somente atalhos opcionais.

O objetivo de pelo menos 30 FPS com baixo consumo de CPU depende de validação na máquina de uso. Uma fonte estática pode gerar menos quadros. Configurar a cadência para 30 FPS não comprova a taxa efetivamente exibida.

## Limitações e validação

- O alvo é Windows 10 versão 1903/build 18362 ou superior, com Direct3D 12 e drivers adequados. O mínimo das APIs de captura está documentado para [janela](https://learn.microsoft.com/en-us/windows/win32/api/windows.graphics.capture.interop/nf-windows-graphics-capture-interop-igraphicscaptureiteminterop-createforwindow) e [monitor](https://learn.microsoft.com/en-us/windows/win32/api/windows.graphics.capture.interop/nf-windows-graphics-capture-interop-igraphicscaptureiteminterop-createformonitor).
- Janelas minimizadas não aparecem no seletor. Uma fonte já conectada pode pausar ao minimizar e retomar ao restaurar.
- Conteúdo protegido pode produzir imagem preta ou impedir a captura.
- Capturar a tela que contém o Mini Studio causa repetição da própria prévia.
- A câmera depende das permissões do Windows, dos formatos aceitos e da disponibilidade do dispositivo.

A validação funcional desta versão fica com o usuário. Roteiro:

- [ ] Capturar cada monitor e observar a prévia com conteúdo em movimento.
- [ ] Capturar, redimensionar, minimizar, restaurar e fechar uma janela.
- [ ] Testar câmera sozinha e sobreposta, canto, tamanho e espelhamento.
- [ ] Testar câmera indisponível e desconexão do dispositivo.
- [ ] Trocar, remover e encerrar fontes, confirmando liberação da câmera.
- [ ] Salvar e abrir o PNG, conferir composição e cancelar o diálogo de salvamento.
- [ ] Observar FPS, CPU e memória durante alguns minutos; registrar resolução e fonte utilizadas.

## Histórico da migração em 30/10/2026

A primeira versão usou Tauri/WebView2, captura individual via xcap/GDI e JPEG, com limite de 12 FPS. Uma medição pontual daquela versão release indicou 11–12 FPS. Em seguida, o backend passou a WGC persistente e transporte de quadros RGBA para o frontend web.

O usuário pediu a remoção do WebView. A implementação atual substitui a interface por egui/eframe e a câmera por Media Foundation. Resultados e testes das versões Tauri anteriores não comprovam funcionamento ou desempenho desta migração nativa. O usuário solicitou compilar e receber instruções para realizar a validação pessoalmente.
