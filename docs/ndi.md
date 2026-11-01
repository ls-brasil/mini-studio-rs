# Recepção NDI

## Referências e decisões

Investigação de código-fonte em 31/10/2026. O repositório OBS já está disponível em `../obs-studio`; não foi necessário criar outro submódulo nem analisar binários com REA. O suporte NDI do OBS é fornecido pelo plugin DistroAV. O Mini Studio usa uma implementação própria sobre a ABI pública do SDK, sem copiar código do plugin.

Fontes consultadas:

- [DistroAV, ndi-source.cpp](https://github.com/DistroAV/DistroAV/blob/68f74d91fd5e782162599ba613ebbe60815df204/src/ndi-source.cpp): receptor em thread, captura com timeout e liberação dos quadros.
- [DistroAV, ndi-finder.cpp](https://github.com/DistroAV/DistroAV/blob/68f74d91fd5e782162599ba613ebbe60815df204/src/ndi-finder.cpp): descoberta de emissores.
- [DistroAV, plugin-main.cpp](https://github.com/DistroAV/DistroAV/blob/68f74d91fd5e782162599ba613ebbe60815df204/src/plugin-main.cpp): carregamento opcional do runtime.
- [NDI-RECV](https://docs.ndi.video/all/developing-with-ndi/sdk/ndi-recv), [NDI-FIND](https://docs.ndi.video/all/developing-with-ndi/sdk/ndi-find) e [Frame Types](https://docs.ndi.video/all/developing-with-ndi/sdk/frame-types).
- Headers do NDI 6 SDK instalado: `Processing.NDI.Find.h`, `Processing.NDI.Recv.h`, `Processing.NDI.Send.h` e `Processing.NDI.structs.h`. Declarações adaptadas em `src/ndi.rs`; aviso MIT em `docs/NDI-headers-LICENSE.txt`, também incluído pelo gerador dos avisos de terceiros.

Observações das fontes: o nome completo identifica o canal; URL nula permite ao SDK encontrar o endereço atual. O receptor pode permanecer aberto durante perda de conexão. Os buffers de descoberta pertencem ao finder e os buffers de vídeo precisam ser liberados pelo receiver.

Decisões locais: RGBX/RGBA, qualidade máxima, campos desentrelaçados pelo SDK, descoberta durante dois segundos por atualização e somente um quadro pendente por receptor. O compositor já trabalha em RGBA. Três segundos sem vídeo removem a imagem anterior e exibem espera; o mesmo receptor continua tentando receber. Esse limite é uma escolha do Mini Studio, não uma garantia do protocolo.

## Integração

`NdiSource` persiste somente o nome do canal em `SourceInput::Ndi`. Cenas e fontes duplicadas compartilham o runtime quando a entrada é igual. A descoberta e a recepção não bloqueiam a UI. Fontes NDI de cenas inativas são liberadas, salvo quando a visualização múltipla precisa delas.

O módulo carrega `Processing.NDI.Lib.x64.dll` por caminhos absolutos: variáveis `NDI_RUNTIME_DIR_V6/V5`, pasta do executável, instalação padrão em Program Files ou `NDI_SDK_DIR/Bin/x64`. Não exige SDK para compilar nem cria dependência de link estático. A inicialização e a DLL são mantidas pelo processo; finder, receiver e buffers têm liberação própria. Falhas de carga não ficam cacheadas.

`RuntimeSource` mantém um receptor aguardando mesmo sem primeiro quadro. Ao perder sinal, remove pixels/textura, de modo que a composição, o projetor e a câmera virtual deixem de apresentar vídeo antigo. Um novo quadro recupera a fonte e atualiza as dimensões. Remover/parar a fonte cancela o worker; a destruição nativa ocorre fora da thread da interface.

## Verificação

- Testes unitários: ABI x64, cópia de linhas com padding/stride negativo/stride zero, RGBX opaco, RGBA com alpha, rejeição de formatos/dimensões/ponteiros inválidos antes de leitura; espera e recuperação sem recriar receiver.
- Persistência e UI: canal salvo junto com fontes anteriores; seleção, busca, duplicação/compartilhamento, substituição preservando geometria, cancelamento e botões em 800×600 e 1440×900.
- Teste real opt-in `installed_ndi_video_round_trip`: passou com NDI Runtime **6.3.2.0** instalado no Windows x64. Emissor sintético foi anunciado e descoberto pelo finder; vídeo de 320×180 e 640×360 foi recebido, verificando cor e alpha com tolerância de compressão. Destruir o emissor apagou a imagem; recriar o mesmo nome reconectou automaticamente. O worker receptor encerrou em cerca de 1,4 ms nessa execução.
- Suíte padrão: 49 testes passaram; 7 testes de dispositivos/rede permaneceram opt-in. Clippy com `-D warnings`, formatação e compilações de desenvolvimento/release passaram. Captura da interface em 800×600 inspecionada visualmente: o botão NDI e as ações da prévia permanecem visíveis.

```powershell
cargo test --locked
cargo test --locked installed_ndi_video_round_trip -- --ignored --nocapture
cargo clippy --locked --all-targets -- -D warnings
```

O teste opt-in anuncia somente vídeo sintético na rede local. Não captura câmeras, telas ou canais de terceiros. Não roda na suíte padrão porque depende do runtime e da rede.

## Limites ainda não verificados

O loopback prova a integração com o runtime instalado, não a interoperabilidade com todo equipamento NDI. Emissores físicos, outro computador, NDI HX, redes com descoberta restrita e sessões longas ainda precisam de validação específica. Não há áudio, PTZ, tally nem transmissão NDI. Pixels anamórficos não são corrigidos automaticamente; ajuste o enquadramento manualmente. O limite de entrada é 16.777.216 pixels e 8192 por lado, e a composição continua em 1280×720.

O runtime não é redistribuído no repositório ou instalador. Use o [redistribuível oficial](https://ndi.link/NDIRedistV6). Os pacotes existentes em `dist` precisam de uma nova versão/empacotamento para incluir a funcionalidade.
