import {
  failInvariant,
} from '../boundary/errors.mjs';

const utf8OffsetMaps = new WeakMap();

function buildUtf8OffsetMap(sourceFile) {
  const offsets = new Uint32Array(sourceFile.text.length + 1);
  let bytes = 0;
  for (let index = 0; index < sourceFile.text.length; index += 1) {
    offsets[index] = bytes;
    const codeUnit = sourceFile.text.charCodeAt(index);
    if (codeUnit <= 0x7f) bytes += 1;
    else if (codeUnit <= 0x7ff) bytes += 2;
    else if (codeUnit >= 0xd800 && codeUnit <= 0xdbff) {
      offsets[index + 1] = 0xffff_ffff;
      index += 1;
      bytes += 4;
    } else bytes += 3;
  }
  offsets[sourceFile.text.length] = bytes;
  utf8OffsetMaps.set(sourceFile, offsets);
}

function utf8ByteOffset(sourceFile, offset) {
  const offsets = utf8OffsetMaps.get(sourceFile);
  if (!offsets || !Number.isInteger(offset) || offset < 0 || offset > sourceFile.text.length) {
    failInvariant('TypeScript returned an invalid UTF-16 source offset');
  }
  if (offsets[offset] === 0xffff_ffff) failInvariant('TypeScript returned an offset inside a surrogate pair');
  return offsets[offset];
}

function spanFromOffsets(sourceFile, file, start, end) {
  return { file, start: utf8ByteOffset(sourceFile, start), end: utf8ByteOffset(sourceFile, end) };
}

function nodeSpan(node, sourceFile, file) {
  return spanFromOffsets(sourceFile, file, node.getStart(sourceFile), node.getEnd());
}

export {
  buildUtf8OffsetMap,
  nodeSpan,
  spanFromOffsets
};
