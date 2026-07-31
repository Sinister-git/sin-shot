/// <reference types="vitest/globals" />

import { decodeBase64Bytes, validateRgbaImage } from './image-contract';

function encoded(bytes: number[]): string {
  return btoa(String.fromCharCode(...bytes));
}

describe('native RGBA image contract', () => {
  it('accepts a synthetic opaque image and preserves pixels', () => {
    const bytes = [255, 0, 0, 255, 0, 255, 0, 255];
    expect(validateRgbaImage({ data: encoded(bytes), width: 2, height: 1 })).toEqual(new Uint8Array(bytes));
  });

  it('accepts transparent pixels when the payload is structurally valid', () => {
    const bytes = [0, 0, 0, 0, 20, 30, 40, 255];
    expect(validateRgbaImage({ data: encoded(bytes), width: 2, height: 1 })[3]).toBe(0);
  });

  it.each([
    ['', 'empty'],
    ['%%%not-base64%%%', 'base64'],
  ])('rejects malformed %s payloads', (data) => {
    expect(() => decodeBase64Bytes(data)).toThrow();
  });

  it('rejects a raw payload with the wrong byte length', () => {
    expect(() => validateRgbaImage({ data: encoded([1, 2, 3, 255]), width: 2, height: 1 })).toThrow(/expected 8/);
  });

  it('rejects invalid dimensions', () => {
    expect(() => validateRgbaImage({ data: encoded([1, 2, 3, 255]), width: 0, height: 1 })).toThrow(/dimensions/);
    expect(() => validateRgbaImage({ data: encoded([1, 2, 3, 255]), width: 1.5, height: 1 })).toThrow(/dimensions/);
  });
});
