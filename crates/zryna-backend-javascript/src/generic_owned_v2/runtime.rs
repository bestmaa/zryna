//! Exact private runtime binding. Helpers obey the retained logical status/handle declarations.
use super::internal;
use zryna_diagnostics::Diagnostic;
use zryna_ir::generic_v1::{owned_v2::raw::Operation, raw};

pub(super) const PRELUDE: &str = r"
const $rt = globalThis[Symbol.for('zryna.generic.ownership.runtime.v1')];
if (!$rt || ['stringFromUtf8Copy','stringClone','stringRelease'].some(name => typeof $rt[name] !== 'function')) throw new Error('ZRYNA-RT-BINDING');
const $from = $rt.stringFromUtf8Copy.bind($rt), $clone = $rt.stringClone.bind($rt), $release = $rt.stringRelease.bind($rt);
let $busy = false;
function $fresh(bytes) { return $handle($from(new Uint8Array(bytes), bytes.length)); }
function $handle(result) {
  if (!result || !Number.isInteger(result.status)) throw new Error('ZRYNA-RT-ABI');
  if (result.status !== 0) throw new Error('ZRYNA-RT-STATUS-' + result.status);
  if (!result.handle || typeof result.handle !== 'object') throw new Error('ZRYNA-RT-ABI');
  return result.handle;
}
function $free(handle) { if ($release(handle) !== 0) throw new Error('ZRYNA-RT-ABI'); }
";

pub(super) fn operation(function: &raw::Function, op: &Operation) -> Result<String, Diagnostic> {
    Ok(match op {
        Operation::StringLiteral(bytes) => {
            format!("$fresh([{}])", bytes.iter().map(u8::to_string).collect::<Vec<_>>().join(","))
        }
        Operation::Move(id) | Operation::Borrow { value: id, .. } => format!("$v{id}"),
        Operation::EndLoan(_) => "undefined".into(),
        Operation::CloneString(id) | Operation::CloneBorrowedString(id) => {
            format!("$handle($clone($v{id}))")
        }
        Operation::Drop(id) => {
            let ty = super::cleanup::value_type(function, *id)?;
            let raw::Type::Stored(index) = ty else {
                return Err(internal());
            };
            format!("$drop{index}($v{id})")
        }
    })
}
