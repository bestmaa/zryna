export const corpus = Object.freeze([
  Object.freeze({ name: 'ordinary', args: Object.freeze([20, 22]), expected: 42 }),
  Object.freeze({ name: 'positive wrap', args: Object.freeze([2147483647, 1]), expected: -2147483648 }),
  Object.freeze({ name: 'negative wrap', args: Object.freeze([-2147483648, -1]), expected: 2147483647 }),
  Object.freeze({ name: 'zero', args: Object.freeze([0, 0]), expected: 0 }),
  Object.freeze({ name: 'negative input', args: Object.freeze([-7, 2]), expected: -5 }),
]);

export function parseI32(text) {
  if (typeof text !== 'string' || text.length > 11 ||
      !/^(?:0|-[1-9][0-9]*|[1-9][0-9]*)$/.test(text)) {
    throw new Error('Input must be a canonical signed i32 decimal');
  }
  const value = Number(text);
  if (!Number.isInteger(value) || value < -2147483648 || value > 2147483647) {
    throw new Error('Input is outside signed i32 range');
  }
  return value;
}
