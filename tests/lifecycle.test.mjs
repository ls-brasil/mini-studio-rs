import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { chromium } from 'playwright';

let server, browser, baseURL;
before(async () => {
  server = createServer(async (req, res) => {
    const name = req.url === '/' ? 'index.html' : req.url.slice(1);
    if (!['index.html', 'main.js', 'studio.js', 'styles.css'].includes(name)) { res.writeHead(404).end(); return; }
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
    window.testState = { frames: 0, delay: 0, fail: false, tracks: [], cameraDelay: 0, cameraError: false };
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
      if (command === 'capture_frame') {
        window.testState.frames++;
        if (window.testState.delay) await new Promise((resolve) => setTimeout(resolve, window.testState.delay));
        if (window.testState.fail) throw new Error('A janela foi fechada. Selecione outra fonte.');
        const canvas = document.createElement('canvas');
        canvas.width = 1280; canvas.height = 720;
        const ctx = canvas.getContext('2d');
        ctx.fillStyle = '#ff0000'; ctx.fillRect(0, 0, 1280, 720);
        return (await new Promise((resolve) => canvas.toBlob(resolve, 'image/jpeg'))).arrayBuffer();
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
