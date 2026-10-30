import { fitRect, cameraRect, mediaError } from './studio.js';
import { decodeFrame } from './preview.js';

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
let pendingDesktop = null;
let sessionOperations = Promise.resolve();
let frames = 0;
let cameraFrames = 0;
let cameraCallback = 0;
let cameraCallbackIsVideo = false;
let cameraRevision = 0;
let paintedCameraRevision = 0;
let paintRequest = 0;
let nextPaintAt = 0;
let dirty = false;
const FRAME_INTERVAL = 1000 / 30;
const metrics = window.__miniStudioMetrics = {
  capturedFrames: 0, presentedFrames: 0, captureTotalMs: 0,
  decodeTotalMs: 0, paintTotalMs: 0, compositions: 0,
  captureIntervalsMs: [], presentationIntervalsMs: [],
};
let lastCaptureAt = 0;
let lastPresentationAt = 0;

function recordInterval(samples, elapsed) {
  samples.push(elapsed);
  if (samples.length > 300) samples.shift();
}

function sessionOperation(operation) {
  const result = sessionOperations.catch(() => {}).then(operation);
  sessionOperations = result.then(() => {}, () => {});
  return result;
}

function releaseSession(sessionId) {
  if (!sessionId) return Promise.resolve();
  return sessionOperation(() => invoke('stop_capture', { sessionId })).catch((error) => {
    notify(`Não foi possível encerrar a sessão de captura: ${String(error)}`, true);
  });
}

function cancelPendingDesktop() {
  if (!pendingDesktop) return;
  pendingDesktop.cancelled = true;
  releaseSession(pendingDesktop.sessionId);
  pendingDesktop.sessionId = null;
  pendingDesktop = null;
}

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
  requestPaint();
}

function stopDesktop() {
  desktopVersion++;
  releaseSession(desktop?.sessionId);
  desktop?.frame?.close();
  desktop = null;
  lastCaptureAt = 0;
  lastPresentationAt = 0;
  frames = 0;
  updateUI();
}

function stopCamera() {
  cameraVersion++;
  const previous = camera;
  camera = null;
  connectingCamera = null;
  if (cameraCallbackIsVideo) video.cancelVideoFrameCallback(cameraCallback);
  else cancelAnimationFrame(cameraCallback);
  cameraCallback = 0;
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
  cancelPendingDesktop();
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

async function captureFrame(sessionId, afterSequence) {
  const started = performance.now();
  const bytes = await invoke('capture_frame', { sessionId, afterSequence });
  metrics.captureTotalMs += performance.now() - started;
  const decoding = performance.now();
  const result = await decodeFrame(bytes);
  metrics.decodeTotalMs += performance.now() - decoding;
  return result;
}

async function pollDesktop(version) {
  while (desktop && version === desktopVersion) {
    try {
      // O backend aguarda novidade; uma única requisição em voo por sessão.
      const frame = await captureFrame(desktop.sessionId, desktop.sequence);
      if (!desktop || version !== desktopVersion) {
        frame?.bitmap.close();
        return;
      }
      if (!frame) continue;
      desktop.frame.close();
      desktop.frame = frame.bitmap;
      desktop.sequence = frame.sequence;
      metrics.capturedFrames++;
      const now = performance.now();
      if (lastCaptureAt) recordInterval(metrics.captureIntervalsMs, now - lastCaptureAt);
      lastCaptureAt = now;
      requestPaint();
    } catch (error) {
      if (version !== desktopVersion) return;
      stopDesktop();
      notify(`A captura foi encerrada. ${String(error)}`, true);
      return;
    }
  }
}

async function startDesktop(source, version) {
  const pending = { cancelled: false, sessionId: null };
  pendingDesktop = pending;
  try {
    const sessionId = await sessionOperation(async () => {
      if (pending.cancelled) return null;
      const id = await invoke('start_capture', { kind: source.kind, id: source.id });
      if (pending.cancelled) {
        await invoke('stop_capture', { sessionId: id });
        return null;
      }
      pending.sessionId = id;
      return id;
    });
    if (!sessionId) return;
    let frame = null;
    const deadline = performance.now() + 5000;
    while (!pending.cancelled && !frame && performance.now() < deadline) {
      frame = await captureFrame(sessionId, 0);
    }
    if (pending.cancelled || version !== dialogVersion || !$('source-dialog').open) {
      frame?.bitmap.close();
      return;
    }
    if (!frame) throw new Error('A fonte não enviou imagens. Restaure a janela e tente novamente.');
    pendingDesktop = null;
    pending.sessionId = null;
    stopDesktop();
    desktop = { ...source, sessionId, frame: frame.bitmap, sequence: frame.sequence, presentedSequence: null };
    metrics.capturedFrames++;
    frames = 0;
    updateUI();
    closeDialog();
    notify(source.kind === 'screen' ? 'Tela conectada. Se o Mini Studio estiver nela, a prévia também aparecerá na captura.' : 'Janela conectada. Mantenha a janela restaurada para continuar capturando.');
    pollDesktop(desktopVersion);
  } finally {
    if (pendingDesktop === pending) pendingDesktop = null;
    releaseSession(pending.sessionId);
    pending.sessionId = null;
  }
}

function watchCamera(selected) {
  if (camera !== selected) return;
  const onFrame = () => {
    if (camera !== selected) return;
    cameraRevision++;
    requestPaint();
    watchCamera(selected);
  };
  cameraCallbackIsVideo = typeof video.requestVideoFrameCallback === 'function';
  cameraCallback = cameraCallbackIsVideo ? video.requestVideoFrameCallback(onFrame) : requestAnimationFrame(onFrame);
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
  watchCamera(selected);
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
  const started = performance.now();
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
  metrics.paintTotalMs += performance.now() - started;
  metrics.compositions++;
}

function requestPaint() {
  dirty = true;
  if (!paintRequest) paintRequest = requestAnimationFrame(render);
}

function render(time) {
  paintRequest = 0;
  if (!dirty) return;
  if (time + 0.8 < nextPaintAt) {
    paintRequest = requestAnimationFrame(render);
    return;
  }
  dirty = false;
  paint();
  if (desktop && desktop.presentedSequence !== desktop.sequence) {
    desktop.presentedSequence = desktop.sequence;
    frames++;
    metrics.presentedFrames++;
    if (lastPresentationAt) recordInterval(metrics.presentationIntervalsMs, time - lastPresentationAt);
    lastPresentationAt = time;
  }
  if (camera && cameraRevision !== paintedCameraRevision) {
    paintedCameraRevision = cameraRevision;
    cameraFrames++;
  }
  // Mantém a fase de 30 Hz: zerar o relógio a cada rAF causa deriva em monitores de 60 Hz.
  nextPaintAt += FRAME_INTERVAL * Math.max(1, Math.floor((time - nextPaintAt) / FRAME_INTERVAL) + 1);
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
$('camera-size').addEventListener('input', () => { $('camera-size-value').textContent = `${$('camera-size').value}%`; requestPaint(); });
$('camera-position').addEventListener('change', requestPaint);
$('mirror-camera').addEventListener('change', requestPaint);
window.addEventListener('beforeunload', () => {
  desktopVersion++;
  cameraVersion++;
  dialogVersion++;
  cancelPendingDesktop();
  if (desktop) invoke('stop_capture', { sessionId: desktop.sessionId }).catch(() => {});
  camera?.stream.getTracks().forEach((track) => track.stop());
  desktop?.frame?.close();
});
let statsAt = performance.now();
setInterval(() => {
  const now = performance.now();
  const seconds = (now - statsAt) / 1000;
  $('fps').textContent = `${Math.round((desktop ? frames : camera ? cameraFrames : 0) / seconds)} fps`;
  $('fps').title = 'Quadros novos exibidos por segundo. Conteúdo parado pode gerar menos quadros.';
  statsAt = now;
  frames = 0;
  cameraFrames = 0;
}, 1000);
updateUI();
