import test from 'node:test';
import assert from 'node:assert/strict';
import { parseFrameBuffer, decodeFrame } from '../src/preview.js';

function packet(width = 2, height = 1, sequence = 42) {
  const buffer = new ArrayBuffer(12 + width * height * 4);
  const header = new DataView(buffer);
  header.setUint32(0, sequence, true);
  header.setUint32(4, width, true);
  header.setUint32(8, height, true);
  new Uint8Array(buffer, 12).set([12, 34, 56, 255].slice(0, width * height * 4));
  return buffer;
}

test('lê sequence, width e height em little-endian e mantém RGBA', () => {
  const parsed = parseFrameBuffer(packet(2, 1, 0x12345678));
  assert.equal(parsed.sequence, 0x12345678);
  assert.equal(parsed.width, 2);
  assert.equal(parsed.height, 1);
  assert.deepEqual([...parsed.pixels], [12, 34, 56, 255, 0, 0, 0, 0]);
});

test('buffer vazio representa ausência de quadro', async () => {
  assert.equal(parseFrameBuffer(new ArrayBuffer(0)), null);
  assert.equal(parseFrameBuffer(new Uint8Array(0)), null);
  assert.equal(await decodeFrame(new ArrayBuffer(0)), null);
});

test('respeita offset e comprimento de Uint8Array sem copiar pixels', () => {
  const original = new Uint8Array(packet());
  const storage = new Uint8Array(original.length + 11);
  storage.set(original, 5);
  const input = storage.subarray(5, 5 + original.length);
  const parsed = parseFrameBuffer(input);
  assert.equal(parsed.pixels.buffer, storage.buffer);
  assert.equal(parsed.pixels.byteOffset, 17);
  assert.equal(parsed.pixels.byteLength, 8);
  storage[17] = 99;
  assert.equal(parsed.pixels[0], 99);
  parsed.pixels[1] = 88;
  assert.equal(storage[18], 88);
});

test('aceita limites de dimensão e wrap da sequência u32', () => {
  assert.equal(parseFrameBuffer(packet(1, 1, 0)).sequence, 0);
  const parsed = parseFrameBuffer(packet(1280, 720, 0xffffffff));
  assert.equal(parsed.sequence, 0xffffffff);
  assert.equal(parsed.pixels.length, 1280 * 720 * 4);
});

test('rejeita cabeçalhos truncados e tipos de entrada inválidos', () => {
  for (const size of [1, 4, 8, 11]) {
    assert.throws(() => parseFrameBuffer(new ArrayBuffer(size)), /cabeçalho/);
  }
  for (const input of [null, undefined, [], 'RGBA', new DataView(new ArrayBuffer(20)), new Uint16Array(10)]) {
    assert.throws(() => parseFrameBuffer(input), TypeError);
  }
});

test('rejeita dimensões nulas, acima do limite e inteiros extremos antes de criar pixels', () => {
  for (const [width, height] of [[0, 1], [1, 0], [1281, 1], [1, 721], [0xffffffff, 0xffffffff]]) {
    const buffer = new ArrayBuffer(12);
    const header = new DataView(buffer);
    header.setUint32(4, width, true);
    header.setUint32(8, height, true);
    assert.throws(() => parseFrameBuffer(buffer), /dimensões/);
  }
});

test('rejeita pixels truncados e bytes excedentes', () => {
  const valid = packet();
  assert.throws(() => parseFrameBuffer(valid.slice(0, 12)), /tamanho/);
  assert.throws(() => parseFrameBuffer(valid.slice(0, valid.byteLength - 1)), /tamanho/);
  const extra = new Uint8Array(valid.byteLength + 1);
  extra.set(new Uint8Array(valid));
  assert.throws(() => parseFrameBuffer(extra), /tamanho/);
});
