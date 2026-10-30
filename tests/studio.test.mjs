import { test } from 'node:test';
import assert from 'node:assert/strict';
import { fitRect, cameraRect } from '../src/studio.js';

const stage = { x: 0, y: 0, width: 1280, height: 720 };
test('composição preserva proporção de janela vertical e centraliza sem cortar', () => {
  assert.deepEqual(fitRect(1080, 1920, stage), { x: 437.5, y: 0, width: 405, height: 720 });
});
test('câmera permanece dentro da composição em todos os cantos e tamanhos', () => {
  for (const corner of ['top-left', 'top-right', 'bottom-left', 'bottom-right']) {
    for (const size of [15, 25, 40]) {
      const rect = cameraRect(size, corner, stage);
      assert.ok(rect.x >= 24 && rect.y >= 24);
      assert.ok(rect.x + rect.width <= 1256 && rect.y + rect.height <= 696);
      assert.equal(rect.width / rect.height, 16 / 9);
    }
  }
});
