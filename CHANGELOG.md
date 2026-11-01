# Histórico de versões

## Em desenvolvimento

- Opção de capturar ou ocultar o ponteiro do mouse nas propriedades de cada fonte de tela ou janela, salva com a cena e aplicada ao alterar.
- Projetor de saída limpa em janela própria ou tela cheia no monitor escolhido, acompanhando a cena ativa sem guias ou controles.
- Projeção com proporção preservada, atalhos F11/Esc e funcionamento independente da câmera virtual.

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
- Guias de composição, busca de fontes, atalhos, ajuda integrada e escala de interface.
- Persistência dos ajustes por usuário e tratamento de erros sem perder a imagem anterior.
- Acessibilidade dos controles por teclado e integração AccessKit/UI Automation.
- Captura de tela/janela, câmera física, PNG/JPEG e exportação PNG em 1280 × 720.
- Câmera virtual própria com publicação a 30 FPS, controle independente e descoberta pelo registro COM.
- Instalador MSI x64 em português, pacote portátil, ícone e versão nos binários, guia e avisos de terceiros.
- Testes de interação da interface, consumo real por Media Foundation e FFmpeg e estabilidade da saída virtual.
