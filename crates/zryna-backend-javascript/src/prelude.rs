//! Canonical scalar boundary helpers shared by separately sealed JavaScript lanes.

pub(crate) const JAVASCRIPT_PRELUDE: &str = r#"function $zryna$checkArity($zryna$actual, $zryna$expected) {
  if ($zryna$actual !== $zryna$expected) {
    throw new TypeError("ZRYNA-B2102: scalar ABI arity mismatch");
  }
}

function $zryna$i32($zryna$value) {
  if (typeof $zryna$value !== "number") {
    throw new TypeError("ZRYNA-B2001: expected a primitive JavaScript Number");
  }
  if ($zryna$value !== ($zryna$value | 0) || ($zryna$value === 0 && 1 / $zryna$value < 0)) {
    throw new RangeError("ZRYNA-B2002: expected a canonical signed 32-bit integer");
  }
  return $zryna$value;
}

function $zryna$bool($zryna$value) {
  if (typeof $zryna$value !== "boolean") {
    throw new TypeError("ZRYNA-B2001: expected a primitive JavaScript Boolean");
  }
  return $zryna$value;
}

function $zryna$imul($zryna$left, $zryna$right) {
  const $zryna$leftLow = $zryna$left & 65535;
  const $zryna$leftHigh = $zryna$left >>> 16;
  const $zryna$rightLow = $zryna$right & 65535;
  const $zryna$rightHigh = $zryna$right >>> 16;
  return ($zryna$leftLow * $zryna$rightLow + ((($zryna$leftHigh * $zryna$rightLow + $zryna$leftLow * $zryna$rightHigh) & 65535) << 16)) | 0;
}

"#;

pub(crate) const MAX_CONTROL_FLOW_JAVASCRIPT_BYTES: usize = 32 * 1024 * 1024;
