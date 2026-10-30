const HEADER_BYTES = 12;
const MAX_WIDTH = 1280;
const MAX_HEIGHT = 720;

export function parseFrameBuffer(bytes) {
  const buffer = bytes instanceof ArrayBuffer
    ? new Uint8Array(bytes)
    : bytes;
  if (!(buffer instanceof Uint8Array) || !(buffer.buffer instanceof ArrayBuffer)) {
    throw new TypeError('O quadro deve ser um ArrayBuffer ou Uint8Array.');
  }
  if (buffer.byteLength === 0) return null;
  if (buffer.byteLength < HEADER_BYTES) {
    throw new RangeError('O cabeçalho do quadro está incompleto.');
  }

  const header = new DataView(buffer.buffer, buffer.byteOffset, HEADER_BYTES);
  const sequence = header.getUint32(0, true);
  const width = header.getUint32(4, true);
  const height = header.getUint32(8, true);
  if (!width || !height || width > MAX_WIDTH || height > MAX_HEIGHT) {
    throw new RangeError('As dimensões do quadro devem estar entre 1 × 1 e 1280 × 720.');
  }
  const pixelBytes = width * height * 4;
  if (buffer.byteLength !== HEADER_BYTES + pixelBytes) {
    throw new RangeError('O tamanho dos pixels não corresponde às dimensões do quadro.');
  }

  // A view compartilha o buffer recebido: não duplica os pixels na passagem pelo JS.
  const pixels = new Uint8ClampedArray(buffer.buffer, buffer.byteOffset + HEADER_BYTES, pixelBytes);
  return { sequence, width, height, pixels };
}

export async function decodeFrame(bytes) {
  const frame = parseFrameBuffer(bytes);
  if (!frame) return null;
  const bitmap = await createImageBitmap(new ImageData(frame.pixels, frame.width, frame.height));
  return { sequence: frame.sequence, bitmap };
}
