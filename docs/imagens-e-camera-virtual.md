# Imagens e câmera virtual

## Status e objetivo

Em 30/10/2026, o usuário definiu dois novos recursos para o Mini Studio: **imagens** e **câmera virtual**. Este documento prepara a implementação; nenhum dos dois está disponível na versão atual.

A direção permanece um aplicativo nativo para Windows, leve e com alvo de 30 FPS. O MVP atual oferece uma tela ou janela, uma câmera física e exportação PNG. A validação prática continua com o usuário.

Os dois recursos estão aprovados como escopo futuro. Formatos, controles e escolhas técnicas abaixo são a proposta inicial para executá-los, sujeita a ajustes durante o desenvolvimento.

## Imagens como fonte

Permitir usar um arquivo local como fundo ou sobreposição, por exemplo uma arte de apresentação ou um logotipo. Importar uma imagem é uma nova fonte; o botão existente de salvar PNG continua sendo uma exportação da composição.

### Primeira entrega proposta

- Adicionar uma imagem por seletor de arquivo nativo, com suporte inicial a PNG e JPEG.
- Usar a imagem sozinha, como fundo ou como sobreposição à tela/janela; manter a câmera física opcional.
- Preservar proporção e transparência de PNG. Áreas transparentes revelam as fontes abaixo; sem outra fonte, revelam o fundo da composição.
- Permitir mostrar/ocultar, substituir, remover, ajustar tamanho e posição da imagem.
- Definir a ordem inicial: fundo da composição → imagem de fundo → tela/janela → imagem de sobreposição → câmera. Uma imagem ocupa apenas um desses modos por vez.
- Exibir erro compreensível para arquivo inválido, formato não aceito ou imagem que exceda o limite de memória definido na implementação, preservando a composição anterior.

Animações, GIF, SVG, sequência de imagens, múltiplas imagens simultâneas e restauração de arquivos entre sessões ficam para avaliação posterior.

### Orientação técnica

Decodificar fora da thread da interface e enviar a textura à GPU somente ao carregar ou substituir a imagem. Reutilizar a textura durante a composição; uma imagem estática não deve provocar decodificação ou upload a cada quadro.

Compartilhar geometria e ordem das fontes entre prévia, PNG e futura câmera virtual. A exportação PNG deve preservar a aparência da composição, com fundo final opaco como no MVP atual. Dimensionar os limites de arquivo e de pixels antes de alocar a imagem decodificada.

### Aceite para validação

- [ ] Abrir JPEG e PNG com transparência, inclusive imagens em retrato e maiores que a saída.
- [ ] Conferir imagem sozinha, fundo, sobreposição e combinação com câmera física.
- [ ] Ajustar posição/tamanho, ocultar, substituir e remover sem interromper a captura.
- [ ] Conferir proporção, transparência e ordem das fontes na prévia e no PNG salvo.
- [ ] Cancelar o seletor e tentar um arquivo inválido, mantendo a composição anterior.
- [ ] Comparar fluidez e consumo com e sem imagem; não introduzir engasgos recorrentes com conteúdo em movimento.

## Câmera virtual

Disponibilizar a composição final do Mini Studio como dispositivo de vídeo selecionável em outros aplicativos, como ferramentas de reunião. O aplicativo consumidor recebe a composição, sem os controles ou bordas da interface do Mini Studio.

### Primeira entrega proposta

- Oferecer controles explícitos **Iniciar câmera virtual** e **Parar câmera virtual**, com estados de inicialização, ativa, parada e erro.
- Publicar inicialmente vídeo de 1280 × 720 a 30 FPS, sem áudio.
- Incluir todas as fontes visíveis e seus ajustes: tela/janela, imagem e câmera física, incluindo posição e espelhamento.
- Permitir trocar fontes com a saída ativa, sem exigir reinício do dispositivo no aplicativo consumidor.
- Manter cadência de saída de 30 FPS também com imagem estática ou fonte parada, repetindo o último quadro composto quando necessário.
- Quando uma fonte for removida, ela deve deixar de aparecer na saída. Sem fontes, publicar o fundo da composição; ao parar/fechar o Mini Studio, encerrar a publicação e liberar os recursos.
- Evitar que a própria câmera virtual seja selecionada como câmera física no Mini Studio, prevenindo realimentação.
- Orientar instalação, ativação e remoção do componente, conforme a tecnologia escolhida, e explicar erros de indisponibilidade na interface.

Compatibilidade com aplicativos específicos será registrada após validação. Não há promessa atual de suporte universal, múltiplos consumidores simultâneos ou funcionamento com o Mini Studio fechado.

### Decisões técnicas antes da implementação

A tecnologia da câmera virtual ainda não está escolhida. Fazer uma prova pequena de publicação e consumo de vídeo antes de integrar todos os controles. Essa prova deve esclarecer:

1. Versões do Windows atendidas e eventual diferença em relação ao requisito atual do MVP.
2. Mecanismo de dispositivo virtual, necessidade de componente nativo separado, registro, instalação e permissões.
3. Formatos de pixel e negociação aceitos pelos aplicativos consumidores escolhidos para validação.
4. Transporte de quadros entre composição e dispositivo, custo de cópias, uso de GPU e encerramento seguro.
5. Comportamento ao desconectar o consumidor, parar a saída ou fechar o processo produtor.

Consultar a documentação oficial vigente das APIs escolhidas e registrar a decisão antes de assumir requisitos de distribuição. A implementação dos projetos irmãos pode servir como referência, após avaliar compatibilidade e licença; este plano não pressupõe reutilização de binários ou dependência do OBS instalado.

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

## Ordem de implementação proposta

1. Adicionar imagens e consolidar ordem, geometria e visibilidade das fontes na prévia e no PNG.
2. Fazer a prova de câmera virtual com quadro de teste; documentar tecnologia, requisitos e consumidores testados.
3. Conectar a composição real à câmera virtual com cadência de 30 FPS.
4. Adicionar controles, tratamento de erros e instalação/remoção conforme a tecnologia escolhida.
5. Compilar e entregar roteiro atualizado para validação pelo usuário, registrando resultados em [desempenho](desempenho.md).

Áudio, gravação, transmissão, múltiplas cenas e edição avançada continuam fora desta expansão. Nenhuma etapa deste documento implica que esses recursos já estejam implementados ou validados.
