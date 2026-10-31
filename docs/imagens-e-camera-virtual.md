# Imagens e câmera virtual

## Status e objetivo

Em 30/10/2026, o usuário definiu dois novos recursos para o Mini Studio: **imagens** e **câmera virtual**. A implementação foi adicionada em 31/10/2026; a validação em aplicativos consumidores permanece pendente.

A direção permanece um aplicativo nativo para Windows, leve e com alvo de 30 FPS. A versão atual oferece uma tela ou janela, uma imagem, uma câmera física, exportação PNG e saída virtual opcional. A validação prática continua com o usuário.

Os requisitos abaixo orientam a entrega implementada. As listas de aceite representam validação funcional pelo usuário; testes automatizados não marcam esses itens como concluídos.

## Imagens como fonte

Permitir usar um arquivo local como fundo ou sobreposição, por exemplo uma arte de apresentação ou um logotipo. Importar uma imagem é uma nova fonte; o botão existente de salvar PNG continua sendo uma exportação da composição.

### Entrega implementada

- Adicionar uma imagem por seletor de arquivo nativo, com suporte inicial a PNG e JPEG.
- Usar a imagem sozinha, como fundo ou como sobreposição à tela/janela; manter a câmera física opcional.
- Preservar proporção e transparência de PNG. Áreas transparentes revelam as fontes abaixo; sem outra fonte, revelam o fundo da composição.
- Permitir mostrar/ocultar, substituir, remover, ajustar tamanho e posição da imagem.
- Definir a ordem inicial: fundo da composição → imagem de fundo → tela/janela → imagem de sobreposição → câmera. Uma imagem ocupa apenas um desses modos por vez.
- Exibir erro compreensível para arquivo inválido, formato não aceito ou imagem que exceda o limite de memória definido na implementação, preservando a composição anterior.

Animações, GIF, SVG, sequência de imagens, múltiplas imagens simultâneas e restauração de arquivos entre sessões ficam para avaliação posterior.

### Orientação técnica

Decodificar fora da thread da interface e enviar a textura à GPU somente ao carregar ou substituir a imagem. Reutilizar a textura durante a composição; uma imagem estática não deve provocar decodificação ou upload a cada quadro.

Compartilhar geometria e ordem das fontes entre prévia, PNG e câmera virtual. A exportação PNG deve preservar a aparência da composição, com fundo final opaco como no MVP original. Dimensionar os limites de arquivo e de pixels antes de alocar a imagem decodificada.

### Aceite para validação

- [ ] Abrir JPEG e PNG com transparência, inclusive imagens em retrato e maiores que a saída.
- [ ] Conferir imagem sozinha, fundo, sobreposição e combinação com câmera física.
- [ ] Ajustar posição/tamanho, ocultar, substituir e remover sem interromper a captura.
- [ ] Conferir proporção, transparência e ordem das fontes na prévia e no PNG salvo.
- [ ] Cancelar o seletor e tentar um arquivo inválido, mantendo a composição anterior.
- [ ] Comparar fluidez e consumo com e sem imagem; não introduzir engasgos recorrentes com conteúdo em movimento.

## Câmera virtual

Disponibilizar a composição final do Mini Studio como dispositivo de vídeo selecionável em outros aplicativos, como ferramentas de reunião. O aplicativo consumidor recebe a composição, sem os controles ou bordas da interface do Mini Studio.

### Entrega implementada

- Oferecer controles explícitos **Iniciar câmera virtual** e **Parar câmera virtual**, com estados de inicialização, ativa, parada e erro.
- Publicar inicialmente vídeo de 1280 × 720 a 30 FPS, sem áudio.
- Incluir todas as fontes visíveis e seus ajustes: tela/janela, imagem e câmera física, incluindo posição e espelhamento.
- Permitir trocar fontes com a saída ativa, sem exigir reinício do dispositivo no aplicativo consumidor.
- Manter cadência de saída de 30 FPS também com imagem estática ou fonte parada, repetindo o último quadro composto quando necessário.
- Quando uma fonte for removida, ela deve deixar de aparecer na saída. Sem fontes, publicar o fundo da composição; ao parar/fechar o Mini Studio, encerrar a publicação e liberar os recursos.
- Evitar que a própria câmera virtual seja selecionada como câmera física no Mini Studio, prevenindo realimentação.
- Orientar instalação, ativação e remoção do componente, conforme a tecnologia escolhida, e explicar erros de indisponibilidade na interface.

Compatibilidade com aplicativos específicos será registrada após validação. Não há promessa atual de suporte universal, múltiplos consumidores simultâneos ou funcionamento com o Mini Studio fechado.

### Decisão técnica — 31/10/2026

- **Plataforma:** Windows 11 x64, build 22000+, pela API [MFCreateVirtualCamera](https://learn.microsoft.com/en-us/windows/win32/api/mfvirtualcamera/nf-mfvirtualcamera-mfcreatevirtualcamera). Imagens e capturas continuam no Windows 10.
- **Dispositivo:** DLL COM `MiniStudioRSVCam.dll` com Media Foundation, acesso CurrentUser e duração Session. Registro em HKLM e instalação em Program Files exigem administrador uma vez; iniciar/parar o aplicativo não exige elevação. A DLL é carregada dinamicamente para não impedir o aplicativo de abrir no Windows 10.
- **Identidade própria:** CLSID `{F0C418C8-9211-479E-BAAE-3CBF90F15D37}`, nome “Mini Studio RS Cam”, pasta `MiniStudioRS`. Não usa nem substitui a câmera do projeto irmão.
- **Formato:** NV12, 1280 × 720, 30 FPS, matriz BT.709, faixa limitada. A conversão RGBA → NV12 faz média de croma 2 × 2. NV12 implica subamostragem de cor; não é uma cópia sem perdas do PNG.
- **Composição:** prévia na GPU; PNG e saída virtual em CPU. Geometria, visibilidade e ordem vêm de `Scene::layers()`. Redimensionamento SIMD, buffers de origem compartilhados por `Arc`, sem cópia dos pixels a cada repintura da interface. A saída mantém apenas a cena mais recente e reutiliza o NV12 enquanto ela não muda. Não se trata de composição GPU ponta a ponta; custo com fontes em movimento precisa ser medido.
- **Transporte:** arquivo mapeado em `%ProgramData%\MiniStudioRS\vcam-frames.bin`, um quadro NV12 e cabeçalho de 64 bytes. Leitor e escritor sincronizam explicitamente com `LockFileEx`; outro intervalo reserva o produtor. O instalador concede escrita ao SID configurado e leitura ao Local Service, sem acesso geral de escrita. Um heartbeat invalida quadros após 500 ms; parar normalmente zera os pixels e o heartbeat. Uma interrupção abrupta pode deixar bytes no arquivo até o próximo início/remoção, embora o consumidor deixe de exibi-los pelo heartbeat.
- **Ciclo de vida:** COM, criação, publicação e remoção ocorrem na mesma thread MTA. A parada é assíncrona na interface; fechar o processo aguarda a liberação. O consumidor recebe um quadro neutro quando o produtor não está disponível. Nenhuma fila cresce com consumidor lento.
- **Referência:** o componente C++ foi adaptado dos fontes locais LS Brasil em `mini-studio/native/MiniStudioVCam`, commit `2c6f69ce8eabe4332d321bdc7da0d80171379564`, com novo transporte, identidade e correções de ciclo de vida/buffers. Não inclui código nem binário do OBS. A árvore de origem não contém arquivo LICENSE; esta adaptação interna não atribui uma licença pública nova ao código. Histórico em `native/MiniStudioRSVCam/ORIGEM.md`.

A [documentação de fontes do Frame Server](https://learn.microsoft.com/en-us/windows-hardware/drivers/stream/frame-server-custom-media-source) orienta o uso de buffers Media Foundation e o acesso pelo serviço Local Service. A [amostra oficial de câmera virtual](https://github.com/microsoft/Windows-Camera/tree/master/Samples/VirtualCamera) documenta a separação entre aplicativo, registro e media source.

### Prova automatizada e limites

`vcam_transport_test` exercita escritor/leitor concorrentes, exclusão de segundo produtor, heartbeat e reinício. `vcam_media_test` publica quadros NV12 em arquivo temporário e os consome através de `IMFSourceReader` e da media source real, verificando pixels, mudança do conteúdo, formato, timestamps, tamanho de buffer, cadência e liberação COM.

Esses testes não registram o dispositivo nem executam o serviço Frame Server. A descoberta em dois aplicativos reais, instalação/remoção elevada e teste contínuo de 10 minutos continuam pendentes. Não foi instalado componente no sistema durante a implementação. Consulte os comandos no [README](../README.md).

### Orientação de arquitetura

A composição deve representar a saída independentemente da janela da interface. O pipeline desejado é: fontes → composição em 1280 × 720 → prévia e saída virtual. A exportação PNG deve seguir a mesma ordem e geometria.

A câmera virtual precisa de um relógio de saída próprio; eventos de quadros novos de Windows Graphics Capture não garantem cadência para conteúdo estático. Usar armazenamento limitado ao quadro mais recente, sem acumular atraso quando o consumidor estiver lento. Separar o ciclo de vida da saída virtual do ciclo de vida das fontes e manter a interface responsiva durante início e parada.

O caminho final de composição e transporte depende da prova técnica. Priorizar reuso de recursos e evitar conversões ou transferências CPU/GPU desnecessárias. Não transportar vídeo por WebView ou serialização JSON.

### Aceite para validação

- [ ] Instalar/ativar e localizar o dispositivo em pelo menos dois aplicativos consumidores, registrando nome e versão de cada um e a versão do Windows.
- [ ] Conferir tela/janela, câmera física e imagem, isoladas e combinadas, sem controles da interface na saída.
- [ ] Conferir proporção, cores, orientação, transparência composta e espelhamento no consumidor.
- [ ] Trocar, ocultar e remover fontes com a saída ativa; verificar também composição sem fontes.
- [ ] Conferir conteúdo em movimento a 30 FPS e saída estática com cadência regular, medindo a saída recebida além do indicador da prévia.
- [ ] Iniciar/parar repetidamente, fechar/reabrir o consumidor e fechar o Mini Studio com saída ativa; confirmar liberação dos recursos e possibilidade de iniciar novamente.
- [ ] Usar por pelo menos 10 minutos, observando latência, CPU, GPU e memória, sem crescimento contínuo de memória ou atraso acumulado.
- [ ] Validar as instruções de instalação e remoção na configuração de Windows escolhida para suporte.

## Estado da entrega

Implementados: imagens PNG/JPEG em segundo plano, controles e ordem compartilhada das fontes, composição opaca, saída virtual em thread própria, componente nativo, scripts de build/instalação/remoção e testes de transporte/Media Foundation.

Limites de imagem: arquivo até 32 MiB, até 16 milhões de pixels e 8192 pixels por lado, limite de alocação do decodificador de 128 MiB. A imagem é normalizada para RGBA e reduzida antes do upload; esses limites não representam um teto para a memória total do processo.

Pendente de validação pelo usuário: os critérios de aceite acima e as medições em [desempenho](desempenho.md). Áudio, gravação, transmissão, múltiplas cenas e edição avançada continuam fora desta expansão.
