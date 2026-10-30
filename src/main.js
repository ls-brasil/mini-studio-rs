import { fitRect, cameraRect, mediaError } from './studio.js';

const $ = (id) => document.getElementById(id);
const invoke = window.__TAURI__?.core.invoke;
const canvas = $('preview');
const ctx = canvas.getContext('2d', { alpha: false });
const video = document.createElement('video');
video.muted = true;
video.autoplay = true;
video.playsInline = true;

let desktop = null;
let camera = null;
let connectingCamera = null;
let desktopVersion = 0;
let cameraVersion = 0;
let dialogVersion = 0;
let dialogKind = null;
let dialogBusy = false;
let snapshotBusy = false;
let frameTimer;
let frameRequest = Promise.resolve();
let frames = 0;
let cameraFrames = 0;
let lastVideoTime = -1;
let lastPaint = 0;

function notify(message, error = false) {
  $('notice').textContent = message;
  $('notice').classList.toggle('error', error);
  $('notice').hidden = !message;
}

function sourceCard(source, type, remove) {
  const card = document.createElement('div');
  card.className = 'source-card';
  const info = document.createElement('div');
  info.className = 'source-info';
  const name = document.createElement('strong');
  name.className = 'source-name';
  name.textContent = source.name;
  name.title = source.name;
  const detail = document.createElement('span');
  detail.className = 'source-detail';
  detail.textContent = type;
  const button = document.createElement('button');
  button.className = 'source-remove';
  button.textContent = 'Remover';
  button.setAttribute('aria-label', `Remover ${source.name}`);
  button.addEventListener('click', remove);
  info.append(name, detail);
  card.append(info, button);
  return card;
}

function updateUI() {
  const active = Boolean(desktop || camera);
  $('empty-state').hidden = active;
  $('status').textContent = active ? 'Capturando' : 'Pronto para capturar';
  $('status').classList.toggle('active', active);
  $('save-snapshot').disabled = !active || snapshotBusy;
  $('stop-all').disabled = !active && !dialogBusy;
  $('camera-controls').hidden = !camera;
  $('source-list').replaceChildren();
  if (desktop) $('source-list').append(sourceCard(desktop, desktop.kind === 'screen' ? 'Tela · ao vivo' : 'Janela · ao vivo', stopDesktop));
  if (camera) $('source-list').append(sourceCard(camera, 'Câmera · ao vivo', stopCamera));
  if (!active) {
    const empty = document.createElement('p');
    empty.className = 'source-detail';
    empty.textContent = 'Nenhuma fonte adicionada.';
    $('source-list').append(empty);
  }
  $('resolution').textContent = '1280 × 720';
  paint();
}

function stopDesktop() {
  desktopVersion++;
  clearTimeout(frameTimer);
  desktop?.frame?.close();
  desktop = null;
  frames = 0;
  updateUI();
}

function stopCamera() {
  cameraVersion++;
  const previous = camera;
  camera = null;
  connectingCamera = null;
  if (previous) previous.stream.getTracks().forEach((track) => track.stop());
  video.pause();
  video.srcObject = null;
  cameraFrames = 0;
  updateUI();
}

function closeDialog() {
  dialogVersion++;
  cameraVersion++;
  dialogBusy = false;
  if (connectingCamera && camera === connectingCamera) stopCamera();
  $('source-dialog').close();
  updateUI();
}

function stopAll() {
  closeDialog();
  stopDesktop();
  stopCamera();
  notify('Todas as capturas foram encerradas.');
}

// Uma única captura nativa por vez, mesmo durante a troca de fonte.
function captureFrame(source) {
  const next = frameRequest.catch(() => {}).then(async () => {
    const bytes = await invoke('capture_frame', { kind: source.kind, id: source.id });
    return createImageBitmap(new Blob([bytes], { type: 'image/jpeg' }));
  });
  frameRequest = next.then(() => {}, () => {});
  return next;
}

async function pollDesktop(version) {
  if (!desktop || version !== desktopVersion) return;
  const started = performance.now();
  try {
    const frame = await captureFrame(desktop);
    if (!desktop || version !== desktopVersion) {
      frame.close();
      return;
    }
    desktop.frame.close();
    desktop.frame = frame;
    frames++;
  } catch (error) {
    if (version !== desktopVersion) return;
    stopDesktop();
    notify(`A captura foi encerrada. ${String(error)}`, true);
    return;
  }
  frameTimer = setTimeout(() => pollDesktop(version), Math.max(0, 1000 / 12 - (performance.now() - started)));
}

async function startDesktop(source, version) {
  const frame = await captureFrame(source);
  if (version !== dialogVersion || !$('source-dialog').open) {
    frame.close();
    return;
  }
  stopDesktop();
  desktop = { ...source, frame };
  frames = 0;
  updateUI();
  closeDialog();
  notify(source.kind === 'screen' ? 'Tela conectada. Se o Mini Studio estiver nela, a prévia também aparecerá na captura.' : 'Janela conectada. Mantenha a janela restaurada para continuar capturando.');
  pollDesktop(desktopVersion);
}

async function startCamera(source, version) {
  if (!navigator.mediaDevices?.getUserMedia) throw new Error('A câmera não está disponível neste ambiente. Abra o aplicativo instalado.');
  if (camera && source.id && source.id === camera.id) {
    closeDialog();
    return;
  }
  const request = ++cameraVersion;
  const stream = await navigator.mediaDevices.getUserMedia({
    audio: false,
    video: { ...(source.id ? { deviceId: { exact: source.id } } : {}), width: { ideal: 1280 }, height: { ideal: 720 }, frameRate: { ideal: 30, max: 30 } },
  });
  if (request !== cameraVersion || version !== dialogVersion || !$('source-dialog').open) {
    stream.getTracks().forEach((track) => track.stop());
    return;
  }
  stopCamera();
  const track = stream.getVideoTracks()[0];
  const selected = { stream, id: track.getSettings().deviceId, name: track.label || source.name };
  camera = selected;
  connectingCamera = selected;
  video.srcObject = stream;
  try {
    await video.play();
  } catch (error) {
    if (camera === selected) stopCamera();
    throw error;
  }
  if (camera !== selected || version !== dialogVersion) {
    stream.getTracks().forEach((item) => item.stop());
    if (camera === selected) stopCamera();
    return;
  }
  track.addEventListener('ended', () => {
    if (camera !== selected) return;
    stopCamera();
    notify('A câmera foi desconectada ou o acesso foi interrompido. Adicione-a novamente para tentar reconectar.', true);
  });
  track.addEventListener('mute', () => {
    if (camera === selected) notify('A câmera interrompeu o envio de imagens. Verifique a conexão e a permissão.', true);
  });
  track.addEventListener('unmute', () => {
    if (camera === selected) notify('A câmera voltou a enviar imagens.');
  });
  connectingCamera = null;
  updateUI();
  closeDialog();
  notify('Câmera conectada. Use os controles para ajustar a composição.');
}

async function chooseSource(source, version) {
  if (dialogBusy || version !== dialogVersion) return;
  dialogBusy = true;
  $('dialog-message').textContent = 'Conectando à fonte…';
  $('refresh-sources').disabled = true;
  $('source-options').querySelectorAll('button').forEach((button) => { button.disabled = true; });
  updateUI();
  try {
    if (dialogKind === 'camera') await startCamera(source, version);
    else await startDesktop(source, version);
  } catch (error) {
    if (version !== dialogVersion) return;
    $('dialog-message').textContent = mediaError(error);
  } finally {
    if (version === dialogVersion) {
      dialogBusy = false;
      $('refresh-sources').disabled = false;
      $('source-options').querySelectorAll('button').forEach((button) => { button.disabled = false; });
      updateUI();
    }
  }
}

async function loadSources() {
  const version = ++dialogVersion;
  const kind = dialogKind;
  $('source-options').replaceChildren();
  $('dialog-message').textContent = 'Procurando fontes disponíveis…';
  $('refresh-sources').disabled = true;
  try {
    let sources;
    if (kind === 'camera') {
      if (!navigator.mediaDevices?.enumerateDevices) throw new Error('A câmera não está disponível neste ambiente.');
      const devices = await navigator.mediaDevices.enumerateDevices();
      sources = devices.filter((device) => device.kind === 'videoinput').map((device, index) => ({
        id: device.deviceId,
        name: device.label || `Câmera ${index + 1}`,
        detail: device.label ? 'Dispositivo de vídeo' : 'O sistema pedirá permissão ao conectar',
      }));
      if (!sources.length) sources = [{ id: '', name: 'Câmera padrão', detail: 'Conectar e verificar a permissão da câmera' }];
    } else {
      if (!invoke) throw new Error('Abra o Mini Studio com cargo tauri dev para capturar telas e janelas.');
      sources = await invoke('list_capture_sources', { kind });
    }
    if (version !== dialogVersion || !$('source-dialog').open) return;
    for (const source of sources) {
      const button = document.createElement('button');
      button.className = 'source-option';
      const title = document.createElement('strong');
      title.textContent = source.name;
      const detail = document.createElement('span');
      detail.textContent = source.detail;
      button.append(title, detail);
      button.addEventListener('click', () => chooseSource({ ...source, kind }, version));
      $('source-options').append(button);
    }
    $('dialog-message').textContent = sources.length ? 'Selecione uma fonte para iniciar a prévia.' : 'Nenhuma fonte encontrada. Abra ou restaure uma janela e clique em Atualizar.';
  } catch (error) {
    if (version === dialogVersion) $('dialog-message').textContent = mediaError(error);
  } finally {
    if (version === dialogVersion) $('refresh-sources').disabled = false;
  }
}

function openSources(kind) {
  dialogKind = kind;
  dialogBusy = false;
  $('dialog-title').textContent = { screen: 'Capturar tela', window: 'Capturar janela', camera: 'Capturar câmera' }[kind];
  $('dialog-description').textContent = kind === 'camera'
    ? 'Escolha uma câmera. Com uma tela ou janela ativa, ela aparece sobreposta.'
    : 'Escolha a fonte principal. A nova seleção substitui a tela ou janela atual.';
  $('source-dialog').showModal();
  loadSources();
}

function drawContained(image, width, height, area, mirror = false) {
  const rect = fitRect(width, height, area);
  ctx.save();
  if (mirror) {
    ctx.translate(rect.x * 2 + rect.width, 0);
    ctx.scale(-1, 1);
  }
  ctx.drawImage(image, rect.x, rect.y, rect.width, rect.height);
  ctx.restore();
}

function paint() {
  ctx.fillStyle = '#080b10';
  ctx.fillRect(0, 0, canvas.width, canvas.height);
  const full = { x: 0, y: 0, width: canvas.width, height: canvas.height };
  if (desktop?.frame) drawContained(desktop.frame, desktop.frame.width, desktop.frame.height, full);
  if (camera && video.readyState >= 2 && video.videoWidth) {
    const area = desktop ? cameraRect(Number($('camera-size').value), $('camera-position').value, full) : full;
    if (desktop) {
      ctx.fillStyle = '#080b10';
      ctx.fillRect(area.x, area.y, area.width, area.height);
      ctx.strokeStyle = '#e2e8f0';
      ctx.lineWidth = 2;
      ctx.strokeRect(area.x - 1, area.y - 1, area.width + 2, area.height + 2);
    }
    drawContained(video, video.videoWidth, video.videoHeight, area, $('mirror-camera').checked);
  }
}

function render(time) {
  if (time - lastPaint >= 1000 / 30) {
    if (desktop || camera) paint();
    if (camera && video.currentTime !== lastVideoTime) {
      cameraFrames++;
      lastVideoTime = video.currentTime;
    }
    lastPaint = time;
  }
  requestAnimationFrame(render);
}

async function saveSnapshot() {
  if ((!desktop && !camera) || snapshotBusy) return;
  snapshotBusy = true;
  updateUI();
  try {
    paint();
    const blob = await new Promise((resolve) => canvas.toBlob(resolve, 'image/png'));
    if (!blob) throw new Error('Não foi possível gerar a imagem.');
    if (!invoke) throw new Error('Abra o aplicativo para salvar a imagem.');
    const path = await invoke('save_snapshot', { bytes: Array.from(new Uint8Array(await blob.arrayBuffer())) });
    notify(path ? `Imagem salva em ${path}` : 'Salvamento cancelado.');
  } catch (error) {
    notify(`Não foi possível salvar. ${String(error)}`, true);
  } finally {
    snapshotBusy = false;
    updateUI();
  }
}

$('add-screen').addEventListener('click', () => openSources('screen'));
$('add-window').addEventListener('click', () => openSources('window'));
$('add-camera').addEventListener('click', () => openSources('camera'));
$('close-dialog').addEventListener('click', closeDialog);
$('source-dialog').addEventListener('cancel', (event) => { event.preventDefault(); closeDialog(); });
$('refresh-sources').addEventListener('click', loadSources);
$('stop-all').addEventListener('click', stopAll);
$('save-snapshot').addEventListener('click', saveSnapshot);
$('camera-size').addEventListener('input', () => { $('camera-size-value').textContent = `${$('camera-size').value}%`; });
window.addEventListener('beforeunload', () => {
  desktopVersion++;
  cameraVersion++;
  dialogVersion++;
  clearTimeout(frameTimer);
  camera?.stream.getTracks().forEach((track) => track.stop());
  desktop?.frame?.close();
});
setInterval(() => {
  $('fps').textContent = `${desktop ? frames : camera ? cameraFrames : 0} fps`;
  frames = 0;
  cameraFrames = 0;
}, 1000);
updateUI();
requestAnimationFrame(render);
