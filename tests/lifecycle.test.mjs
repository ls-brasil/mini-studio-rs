import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { chromium } from 'playwright';

let server, browser, baseURL;
before(async () => {
  server = createServer(async (req, res) => {
    const name = req.url === '/' ? 'index.html' : req.url.slice(1);
    if (!['index.html', 'main.js', 'studio.js', 'preview.js', 'styles.css'].includes(name)) { res.writeHead(404).end(); return; }
    res.setHeader('Content-Type', name.endsWith('.js') ? 'text/javascript' : name.endsWith('.css') ? 'text/css' : 'text/html');
    res.end(await readFile(new URL(`../src/${name}`, import.meta.url)));
  });
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  baseURL = `http://127.0.0.1:${server.address().port}`;
  browser = await chromium.launch({ channel: 'msedge', headless: true, args: ['--use-fake-device-for-media-stream', '--use-fake-ui-for-media-stream'] });
});
after(async () => {
  await browser?.close();
  await new Promise((resolve) => server ? server.close(resolve) : resolve());
});

async function setup(t) {
  const context = await browser.newContext();
  const page = await context.newPage();
  const errors = [];
  page.on('pageerror', (error) => errors.push(error.message));
  t.after(async () => { await context.close(); assert.deepEqual(errors, []); });
  await page.addInitScript(() => {
    window.testState = { frames: 0, delay: 0, fail: false, tracks: [], cameraDelay: 0, cameraError: false, sessions: [], stopped: [], starts: 0 };
    const originalPlay = HTMLMediaElement.prototype.play;
    HTMLMediaElement.prototype.play = function () {
      const result = originalPlay.call(this);
      return window.testState.holdPlay ? result.then(() => new Promise(() => {})) : result;
    };
    const original = navigator.mediaDevices.getUserMedia.bind(navigator.mediaDevices);
    navigator.mediaDevices.getUserMedia = async (constraints) => {
      if (window.testState.cameraError) throw new DOMException('denied', 'NotAllowedError');
      const stream = await original(constraints);
      window.testState.tracks.push(...stream.getTracks());
      if (window.testState.cameraDelay) await new Promise((resolve) => setTimeout(resolve, window.testState.cameraDelay));
      return stream;
    };
    window.__TAURI__ = { core: { invoke: async (command, args) => {
      if (command === 'list_capture_sources') return [{ id: 1, name: args.kind === 'screen' ? 'Monitor de teste' : 'Janela de teste', detail: '1280 × 720' }];
      if (command === 'start_capture') {
        if (window.testState.startDelay) await new Promise((resolve) => setTimeout(resolve, window.testState.startDelay));
        const id = `session-${++window.testState.starts}`;
        window.testState.sessions.push(id);
        return id;
      }
      if (command === 'stop_capture') {
        window.testState.sessions = window.testState.sessions.filter((id) => id !== args.sessionId);
        window.testState.stopped.push(args.sessionId);
        return;
      }
      if (command === 'capture_frame') {
        window.testState.frames++;
        await new Promise((resolve) => setTimeout(resolve, window.testState.delay || 33));
        if (!window.testState.sessions.includes(args.sessionId)) throw new Error('Sessão encerrada.');
        if (window.testState.fail) throw new Error('A janela foi fechada. Selecione outra fonte.');
        if (window.testState.static && args.afterSequence > 0) {
          await new Promise((resolve) => setTimeout(resolve, 100));
          return new ArrayBuffer(0);
        }
        const bytes = new ArrayBuffer(12 + 320 * 180 * 4);
        const header = new DataView(bytes);
        header.setUint32(0, window.testState.frames, true);
        header.setUint32(4, 320, true);
        header.setUint32(8, 180, true);
        const pixels = new Uint8Array(bytes, 12);
        for (let offset = 0; offset < pixels.length; offset += 4) {
          pixels[offset] = 255; pixels[offset + 3] = 255;
        }
        return bytes;
      }
      if (command === 'save_snapshot') {
        const image = await createImageBitmap(new Blob([new Uint8Array(args.bytes)]));
        window.testState.saved = { width: image.width, height: image.height, signature: args.bytes.slice(0, 8) };
        image.close();
        return 'C:\\teste\\composicao.png';
      }
    } } };
  });
  await page.goto(baseURL);
  return page;
}

async function select(page, kind) {
  await page.locator(`#add-${kind}`).click();
  await page.locator('.source-option').first().click();
  await page.waitForFunction(() => !document.querySelector('#source-dialog').open);
}

test('tela/janela, composição com câmera, PNG e liberação de recursos', async (t) => {
  const page = await setup(t);
  await select(page, 'screen');
  await page.waitForFunction(() => window.testState.frames > 2);
  const pixel = await page.locator('#preview').evaluate((canvas) => Array.from(canvas.getContext('2d').getImageData(640, 360, 1, 1).data));
  assert.ok(pixel[0] > 240 && pixel[1] < 10);
  await select(page, 'window');
  assert.equal(await page.locator('.source-card').count(), 1);
  assert.match(await page.locator('#source-list').innerText(), /Janela de teste/);
  await select(page, 'camera');
  assert.equal(await page.locator('.source-card').count(), 2);
  await page.locator('#camera-position').selectOption('top-left');
  await page.locator('#mirror-camera').check();
  await page.locator('#save-snapshot').click();
  await page.waitForFunction(() => window.testState.saved);
  assert.deepEqual(await page.evaluate(() => window.testState.saved), { width: 1280, height: 720, signature: [137, 80, 78, 71, 13, 10, 26, 10] });
  await page.locator('.source-remove').first().click();
  assert.equal(await page.locator('.source-card').count(), 1);
  assert.equal(await page.locator('#empty-state').isHidden(), true);
  await page.locator('#stop-all').click();
  await page.waitForFunction(() => !window.testState.sessions.length);
  assert.equal(await page.locator('.source-card').count(), 0);
  assert.equal(await page.locator('#save-snapshot').isDisabled(), true);
  assert.ok(await page.evaluate(() => window.testState.tracks.every((track) => track.readyState === 'ended')));
});

test('cancelar seleção descarta quadro pendente e câmera concedida tardiamente', async (t) => {
  const page = await setup(t);
  await page.evaluate(() => { window.testState.delay = 350; });
  await page.locator('#add-screen').click();
  await page.locator('.source-option').click();
  await page.locator('#close-dialog').click();
  await page.waitForTimeout(500);
  assert.equal(await page.locator('.source-card').count(), 0);
  assert.equal(await page.evaluate(() => window.testState.sessions.length), 0);
  await page.evaluate(() => { window.testState.cameraDelay = 500; });
  await page.locator('#add-camera').click();
  await page.locator('.source-option').first().click();
  await page.waitForFunction(() => window.testState.tracks.length > 0);
  await page.locator('#close-dialog').click();
  await page.waitForFunction(() => window.testState.tracks.every((track) => track.readyState === 'ended'));
  assert.equal(await page.locator('.source-card').count(), 0);
});

test('erros de fonte e permissão permitem tentar novamente', async (t) => {
  const page = await setup(t);
  await select(page, 'window');
  await page.evaluate(() => { window.testState.fail = true; });
  await page.waitForFunction(() => !document.querySelector('.source-card'));
  assert.match(await page.locator('#notice').innerText(), /janela foi fechada/);
  await page.evaluate(() => { window.testState.cameraError = true; });
  await page.locator('#add-camera').click();
  await page.locator('.source-option').first().click();
  await page.waitForFunction(() => document.querySelector('#dialog-message').textContent.includes('Permissão'));
  await page.evaluate(() => { window.testState.cameraError = false; });
  await page.locator('.source-option').first().click();
  await page.waitForFunction(() => !document.querySelector('#source-dialog').open);
  await page.locator('#stop-all').click();
});

test('cancelar câmera durante inicialização do vídeo libera dispositivo imediatamente', async (t) => {
  const page = await setup(t);
  await page.evaluate(() => { window.testState.holdPlay = true; });
  await page.locator('#add-camera').click();
  await page.locator('.source-option').first().click();
  await page.waitForFunction(() => window.testState.tracks.length > 0);
  await page.locator('#close-dialog').click();
  assert.ok(await page.evaluate(() => window.testState.tracks.every((track) => track.readyState === 'ended')));
  assert.equal(await page.locator('.source-card').count(), 0);
});

test('cancelar criação nativa tardia não deixa sessão ativa', async (t) => {
  const page = await setup(t);
  await page.evaluate(() => { window.testState.startDelay = 250; });
  await page.locator('#add-window').click();
  await page.locator('.source-option').click();
  await page.locator('#close-dialog').click();
  await page.waitForFunction(() => window.testState.stopped.length === 1);
  assert.equal(await page.evaluate(() => window.testState.sessions.length), 0);
  assert.equal(await page.locator('.source-card').count(), 0);
});

test('prévia não redesenha conteúdo estático e libera captura ao parar', async (t) => {
  const page = await setup(t);
  await page.evaluate(() => { window.testState.static = true; });
  await select(page, 'screen');
  await page.waitForFunction(() => window.__miniStudioMetrics.presentedFrames === 1);
  const before = await page.evaluate(() => window.__miniStudioMetrics.compositions);
  await page.waitForTimeout(350);
  assert.equal(await page.evaluate(() => window.__miniStudioMetrics.compositions), before);
  await page.locator('#stop-all').click();
  await page.waitForFunction(() => !window.testState.sessions.length);
  await page.waitForTimeout(200);
  const requests = await page.evaluate(() => window.testState.frames);
  await page.waitForTimeout(150);
  assert.equal(await page.evaluate(() => window.testState.frames), requests);
});
