//! Frozen logical runtime imports and bounded private records in trusted shared memory.
use super::bytes::Bytes;
use wasm_encoder::{Instruction as Op, MemArg};
use zryna_diagnostics::Diagnostic;
pub(super) const NAMESPACE: &str = "zryna.generic.ownership.runtime.v1";
pub(super) const NAMES: [&str; 3] = ["stringFromUtf8Copy", "stringClone", "stringRelease"];
pub(super) fn imports(module: &mut Bytes) -> Result<(), Diagnostic> {
    let mut imports = Bytes::new();
    imports.u32(4)?;
    for (i, name) in NAMES.iter().enumerate() {
        imports.name(NAMESPACE)?;
        imports.name(name)?;
        imports.extend(&[0])?;
        imports.count(i)?;
    }
    imports.name(NAMESPACE)?;
    imports.name("memory")?;
    imports.extend(&[2, 1])?;
    imports.u32(1024)?;
    imports.u32(1024)?;
    module.section(2, &imports)
}
pub(super) fn record(body: &mut Bytes, frame: u32, offset: i32) -> Result<(), Diagnostic> {
    body.op(&Op::LocalGet(frame))?;
    body.op(&Op::I32Const(32))?;
    body.op(&Op::I32Mul)?;
    body.op(&Op::I32Const(offset))?;
    body.op(&Op::I32Add)
}
pub(super) fn load(body: &mut Bytes, frame: u32, lane: u32) -> Result<(), Diagnostic> {
    record(body, frame, 0)?;
    body.op(&Op::I32Load(MemArg { offset: u64::from(lane * 4), align: 2, memory_index: 0 }))
}
