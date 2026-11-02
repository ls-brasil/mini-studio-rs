# Histórico de versões

## Em desenvolvimento

- Mover, centralizar, recortar e redimensionar proporcionalmente uma fonte preserva agora seu modo de proporção, inclusive quando a janela capturada muda de tamanho. Esticamento continua disponível com Shift ao redimensionar e é preservado nos usos já configurados assim.
- Validação da correção de proporção: 56 testes passaram, sete integrações externas permaneceram ignoradas; formatação, Clippy e compilação debug concluídos sem erros. A cena relatada não foi validada visualmente com o VS Code, pois esse uso foi removido durante a conferência.
- Configuração da biblioteca com prévia visual à direita para mover, redimensionar, recortar com Alt, espelhar, ajustar opacidade e restaurar o enquadramento. As edições aplicam-se a todos os usos da fonte nas cenas, inclusive ocultos ou bloqueados; novos usos recebem o enquadramento salvo na biblioteca.
- Validação: 55 testes passaram, sete testes dependentes de dispositivos/componentes externos permaneceram ignorados, Clippy sem avisos e compilação debug concluída. Testes de interação em 800×600 e 1440×900 cobrem propagação, desfazer/refazer e novos usos; persistência de fonte sem uso e conferência visual da tela no aplicativo também verificadas.
- Prévia disponível também para fontes ainda sem uso, com reaproveitamento da captura existente quando disponível. Desfazer/refazer da biblioteca restaura os enquadramentos anteriores por uso durante a sessão de edição; a edição direta nas cenas continua individual. README e guia atualizados.

- Validação da recuperação: 53 testes passaram e Clippy passou sem avisos. Inclui reabertura de arquivo que reaparece e regras de identidade para título/identificador de janela alterados; sete testes dependentes de dispositivos ou componentes externos permanecem ignorados.
- Recuperação ativa das fontes: após uma falha, novas buscas e aberturas ocorrem em segundo plano, com intervalos de 2 a 5 segundos, sem depender de uma notificação da sessão antiga. Usos compartilhados mantêm a mesma sessão de recuperação; botões de reconexão manual removidos.
- Captura de janela acompanha mudanças de título pelo identificador e procura uma janela recriada pelo título/aplicativo. A busca pelo aplicativo exige uma candidata única e não usa processos hospedeiros genéricos; ambiguidades não selecionam outra janela arbitrariamente.

- Removidos os templates de cena e o fluxo de seleção guiada. Criar cena agora solicita apenas o nome e gera uma composição vazia para receber fontes da biblioteca; cenas já salvas mantêm suas fontes e enquadramentos. Ajuda e guias ajustados.
- Removidos os cinco testes exclusivos dos templates: os 51 testes restantes passaram, com sete testes de integração ignorados e Clippy sem avisos. A atualização da lista de origens preserva o destino da configuração na biblioteca.

- Fontes da cena ativa iniciam automaticamente ao reabrir o aplicativo, sem aguardar Ativar. A troca de cena mantém a abertura automática; falhas agora disparam a recuperação automática em intervalos controlados, e o encerramento explícito das capturas é respeitado. Ajuda e guias atualizados para esse comportamento.

- Conferência visual da biblioteca compacta e do editor amplo em uma cópia de verificação, sem gravar nas cenas do usuário.
- Biblioteca compacta com um único botão + para escolher o tipo e linhas com ícone, nome truncado e engrenagem. A edição ocupa a área principal, fora do rodapé e sem modal, com nome, origem, captura do cursor, indicação de uso, cópia e exclusão; Voltar à prévia retorna à composição.

- Fontes podem ser arrastadas da biblioteca no rodapé sobre qualquer cena; o destino recebe destaque durante o arraste, e cada uso mantém sua posição e tamanho. Configurações da biblioteca ficam no editor separado para liberar a lista de fontes.

- Layout dividido em biblioteca no lado esquerdo do rodapé e cenas à direita, mantendo a prévia grande acima. Adicionar uma camada passa a selecionar fontes cadastradas; a prévia vazia orienta o reaproveitamento, e imagens arrastadas são cadastradas na biblioteca.

- Versão de distribuição elevada para 2.0.0.
- Biblioteca de fontes permite cadastrar e configurar capturas e imagens sem vinculá-las a uma cena, reutilizá-las em várias cenas e criar cópias independentes.
- Nome e configuração da captura compartilhados; posição, tamanho, recorte, espelhamento, opacidade, visibilidade, bloqueio e ordem independentes por uso. Duplicar cenas ou camadas mantém a referência à fonte.
- Remover usos ou cenas preserva as fontes cadastradas; exclusão da biblioteca disponível somente para fontes sem uso, com indicação das cenas que as utilizam.
- Migração das cenas antigas para o formato 3 preserva cada configuração e composição, sem unir fontes automaticamente. O formato migrado não é compatível com versões anteriores.
- Guia e README atualizados para o fluxo da biblioteca.
- Validação automatizada: 56 testes passaram (incluindo arrastar uma fonte para duas cenas com enquadramentos independentes e editor da biblioteca em janelas de 800×600 e 1440×900, carregamento automático de uma imagem de cena salva e ausência de repetição de abertura após falha) e Clippy passou sem avisos; sete testes que dependem de dispositivos, desktop interativo ou componentes instalados não foram executados.

- Nova fonte NDI com descoberta de canais locais e da rede, busca por nome e atualização da lista, disponível na prévia vazia e no menu de Fontes.
- Canais NDI salvos nas cenas, compartilhamento de recepção entre fontes iguais e substituição do canal preservando o enquadramento. O vídeo participa da composição, projetor, exportação PNG e câmera virtual.
- Recepção NDI em segundo plano, com transparência e mudanças de resolução; após três segundos sem vídeo, a imagem antiga é removida e a fonte aguarda reconexão automática.
- Carregamento opcional do NDI Runtime x64, com orientação e link de instalação no seletor. Recepção somente de vídeo; áudio, PTZ, tally e envio NDI não estão disponíveis, e NDI HX ainda não foi validado.
- Documentação da integração NDI, avisos de licença dos headers incluídos no empacotamento e testes de persistência, interface, conversão de quadros e recepção/reconexão com runtime real.
- Opção de capturar ou ocultar o ponteiro do mouse nas propriedades de cada fonte de tela ou janela, salva com a cena e aplicada ao alterar.
- Projetor de saída limpa em janela própria ou tela cheia no monitor escolhido, acompanhando a cena ativa sem guias ou controles.
- Projeção com proporção preservada, atalhos F11/Esc e funcionamento independente da câmera virtual.
- Encerramento da projeção com aviso quando o monitor de destino é desconectado ou reconfigurado.
- Skill e guias de engenharia reversa adicionados ao projeto, incluindo observação de execução e registro de evidências.
- Orientação permanente em AGENTS.md para atualizar este histórico junto com cada mudança e conferir commits e alterações pendentes antes de concluir.

## 1.0.3 — 31/10/2026

- Removidos os botões de Propriedades e Ajustar à tela da faixa abaixo da prévia.
- Ajustar à tela agora fica na barra inferior de Fontes, como botão de ícone com dica ao passar o mouse.

## 1.0.2 — 31/10/2026

- Modelos de cena com configuração guiada: tela ou janela com câmera no canto e duas janelas lado a lado, preservando as proporções das fontes.
- Inclusão direta de tela, janela, câmera ou imagem pela prévia vazia, com acesso aos modelos de composição.
- Ações na lista de fontes salvas para ativar, reconectar, localizar imagens e escolher outro dispositivo ou janela.
- Diálogo de propriedades mais compacto e adaptável a janelas pequenas, preservando o enquadramento ao alterar outras opções.
- Ajuda, guia do usuário e captura da interface atualizados para os novos fluxos de composição e atalhos existentes.
- Testes adicionais para modelos, configuração guiada, propriedades e interação com fontes salvas.
- Geração de checksums SHA256 dos pacotes pelo .NET, sem depender do cmdlet Get-FileHash.

## 1.0.1 — 31/10/2026

- Visualização múltipla em janela separada, com prévias das cenas e troca de cena por clique.
- Fontes mantidas ativas durante a visualização múltipla e capturas extras liberadas ao fechar a janela.
- Botão de Visualização múltipla no painel Controles, substituindo o botão de salvar cena; as cenas continuam sendo salvas automaticamente.
- Versão da aplicação, câmera virtual e pacotes centralizada no package.json, com validação do formato e dos limites de versão.

## 1.0.0 — 31/10/2026

- Nova interface nativa com tema escuro, identidade visual, estados vazios orientados e layout adaptável.
- Fontes em camadas, visibilidade individual, ajustes contextuais e posicionamento pela prévia.
- Múltiplas fontes do mesmo tipo por cena, duplicação com captura compartilhada, ordem das camadas, bloqueio e migração das cenas anteriores.
- Redimensionamento por alças, recorte com Alt, proporção livre com Shift, encaixe nas bordas e centros, espelhamento e desfazer/refazer do enquadramento.
- Guias de composição, busca de fontes, atalhos, ajuda integrada e escala de interface.
- Persistência dos ajustes por usuário e tratamento de erros sem perder a imagem anterior.
- Acessibilidade dos controles por teclado e integração AccessKit/UI Automation.
- Captura de tela/janela, câmera física, PNG/JPEG e exportação PNG em 1280 × 720.
- Câmera virtual própria com publicação a 30 FPS, controle independente e descoberta pelo registro COM.
- Instalador MSI x64 em português, pacote portátil, ícone e versão nos binários, guia e avisos de terceiros.
- Testes de interação da interface, consumo real por Media Foundation e FFmpeg e estabilidade da saída virtual.
