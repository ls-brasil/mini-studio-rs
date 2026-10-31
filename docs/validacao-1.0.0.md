# Validação — Mini Studio 1.0.0

Data: 31/10/2026. Ambiente: Windows x64 build 26300.9818, Rust/MSVC, Visual Studio 18 e Windows SDK 10.0.26100.0.

## Verificações automatizadas

- 18 testes Rust passaram no perfil Release; 5 testes de dispositivos são opt-in.
- 2 testes nativos C++ passaram.
- Os 2 testes opt-in da câmera virtual instalada passaram também com o runtime estático da distribuição: FFmpeg e Media Foundation (teste curto de 10 segundos, além do ensaio anterior de 600 segundos).
- Formatação e Clippy com avisos tratados como erros passaram.

## Funcionalidade e interface

A versão 1.0 inclui teste automatizado de interação real do egui: carregamento de imagem, clique no olho para ocultar/mostrar, remoção por Del, navegação Fontes/Ajustes na janela compacta, proteção dos atalhos durante diálogos e preservação da imagem após erro/cancelamento. Os controles são localizados pelos nomes de acessibilidade. Há também testes de composição, transparência, proporção, espelhamento, posição, conversão NV12, limites de imagem e preferências.

A interface foi renderizada pelo executável e inspecionada em 1440 × 900 e 1024 × 720, com estado vazio e PNG transparente. Ajustes permanecem roláveis; o controle da câmera virtual fica sempre disponível em seu painel. Capturas de QA ficam em `target/qa` (não versionadas).

O executável Release também capturou um monitor por 10,03 segundos: **301 quadros, 30,00 FPS**, nenhum quadro pulado, p50 de 33,43 ms e p95 de 45,18 ms. Adaptador: NVIDIA GeForce GTX 1650; relatório em `target/qa/release-screen.json`. É um ensaio curto da sessão atual, não uma certificação de desempenho em qualquer computador.

## Saída virtual instalada

O componente já estava registrado no computador no início desta preparação. A sessão não executou instalação elevada. O teste utiliza somente imagens sintéticas e não ativa câmera física.

- **Media Foundation:** 17.999 quadros recebidos em 600 segundos após aquecimento; **30,00 FPS**. Validou tamanho 1280 × 720, cores, mudança de conteúdo, exclusão da própria câmera do seletor de entrada, desaparecimento após parada e reinício.
- **FFmpeg 9.0.2 / DirectShow:** 150 quadros em 5 segundos, registrados por `framemd5` em processo separado.
- **Transporte nativo:** concorrência de leitor/escritor, rejeição de segundo produtor, expiração por heartbeat e reinício.
- **Media source nativa:** formato, pixels, timestamps, buffers e liberação COM por IMFSourceReader.

O monitoramento parcial do processo de teste teve 94 amostras ao longo de 466 segundos: memória privada entre 5,54 e 16,30 MiB; última amostra 10,57 MiB. Inclui produtor e consumidor de teste; exclui interface gráfica, Frame Server e GPU. Houve outras tarefas e compilação simultânea. Não é uma medição controlada do custo total do produto.

Os arquivos locais `target/qa/vcam-soak.log`, `target/qa/soak-memory.json`, `target/qa/ffmpeg-vcam.log` e `target/qa/ffmpeg-vcam.framemd5` preservam a evidência da execução. A DLL distribuída acrescenta metadados de versão ao mesmo código nativo exercitado pelo componente instalado.

## Distribuição

A versão é centralizada em Cargo.toml: 1.0.0 no aplicativo, DLL e MSI. O runtime C é incorporado aos binários Windows. O MSI inclui atalhos, registro COM condicionado ao Windows 11, permissões da pasta de transporte, guia e avisos de terceiros. O pacote portátil contém aplicativo, guia e avisos. Os hashes SHA-256 permitem verificar os arquivos gerados.

O comando `wix msi validate`, incluído no script de distribuição, aplica as verificações ICE do Windows Installer. A inspeção do pacote confere versão, plataforma x64, arquivos, permissões e registro; os EXEs extraídos de MSI/ZIP são comparados por SHA-256 com o binário compilado. Esses testes de estrutura e extração não equivalem a uma instalação elevada completa.

## Limites verificados e pendências externas

- Instalação, atualização e remoção do **novo MSI** em uma máquina limpa não foram executadas nesta sessão sem elevação.
- Zoom, Teams, Meet e outros aplicativos de reunião específicos não foram exercitados; os dois consumidores técnicos testados são Media Foundation e FFmpeg/DirectShow.
- O teste de 10 minutos usa conteúdo sintético variável. Não certifica o desempenho com captura de tela/janela e câmera física simultâneas em todas as GPUs.
- O pacote não possui assinatura Authenticode: não havia certificado de publicação disponível no projeto.
- A v1.0 compõe vídeo e exporta PNG; áudio, gravação, transmissão e múltiplas cenas não integram seu escopo.

Para repetir os testes, consulte o README. Fontes e configurações pessoais não são incluídas nos pacotes de distribuição.
