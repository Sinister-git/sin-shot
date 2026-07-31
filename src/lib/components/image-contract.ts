/** Validation for the native capture contract: tightly packed RGBA bytes. */
export interface RgbaImageContract {
  data: string;
  width: number;
  height: number;
}

export function decodeBase64Bytes(data: string): Uint8Array {
  if (!data.trim()) throw new Error('Captured image payload is empty');
  let binary: string;
  try {
    binary = atob(data);
  } catch {
    throw new Error('Captured image payload is not valid base64');
  }
  return Uint8Array.from(binary, (character) => character.charCodeAt(0));
}

export function validateRgbaImage(image: RgbaImageContract): Uint8Array {
  if (!Number.isSafeInteger(image.width) || !Number.isSafeInteger(image.height) || image.width < 1 || image.height < 1) {
    throw new Error('Captured image dimensions are invalid');
  }
  const expectedLength = image.width * image.height * 4;
  if (!Number.isSafeInteger(expectedLength)) {
    throw new Error('Captured image dimensions are too large');
  }
  const bytes = decodeBase64Bytes(image.data);
  if (bytes.length !== expectedLength) {
    throw new Error(`Captured image payload has ${bytes.length} bytes; expected ${expectedLength}`);
  }
  return bytes;
}
