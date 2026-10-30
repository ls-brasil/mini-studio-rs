export function fitRect(width, height, area) {
  const scale = Math.min(area.width / width, area.height / height);
  const fittedWidth = width * scale;
  const fittedHeight = height * scale;
  return { x: area.x + (area.width - fittedWidth) / 2, y: area.y + (area.height - fittedHeight) / 2, width: fittedWidth, height: fittedHeight };
}

export function cameraRect(percent, position, area) {
  const width = area.width * Math.max(15, Math.min(40, percent)) / 100;
  const height = width * 9 / 16;
  const padding = 24;
  return {
    x: position.endsWith('left') ? area.x + padding : area.x + area.width - width - padding,
    y: position.startsWith('top') ? area.y + padding : area.y + area.height - height - padding,
    width, height,
  };
}

export function mediaError(error) {
  const errors = {
    NotAllowedError: 'Permissão de câmera negada. Permita o acesso no aplicativo e em Configurações do Windows → Privacidade e segurança → Câmera.',
    NotFoundError: 'Nenhuma câmera disponível. Conecte uma câmera e tente novamente.',
    NotReadableError: 'Não foi possível abrir a câmera. Feche outros aplicativos que estejam usando o dispositivo e tente novamente.',
    OverconstrainedError: 'A câmera selecionada não está mais disponível ou não aceita esta configuração. Atualize a lista e tente novamente.',
    AbortError: 'A conexão com a câmera foi interrompida. Tente novamente.',
    SecurityError: 'O acesso à câmera foi bloqueado pelo sistema.',
  };
  return errors[error?.name] || error?.message || String(error);
}
