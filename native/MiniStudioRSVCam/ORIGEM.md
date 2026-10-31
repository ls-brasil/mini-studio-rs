# Origem do componente

Adaptação interna dos fontes de `ls-brasil/mini-studio`, pasta `native/MiniStudioVCam`, commit `2c6f69ce8eabe4332d321bdc7da0d80171379564`. A árvore local consultada não apresenta arquivo LICENSE; preserva-se a origem sem atribuir uma licença pública nova. Não há código nem binário do OBS nesta pasta.

Reaproveitados: estrutura COM, modelo de eventos Media Foundation e integração com o alocador do Frame Server.

Alterados nesta implementação: CLSID/nome/pastas, transporte sincronizado de um quadro, controle de produtor único, ACL de leitura do serviço, exports para Rust, limpeza em erro, comprimento/limites dos buffers, cadência do consumidor, contagem da fábrica COM e testes nativos. Não há dependência de binários ou checkout do projeto irmão para compilar.
