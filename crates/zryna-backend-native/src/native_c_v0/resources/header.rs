//! Fixed compiler-private physical context channel; it is never an admitted foreign parameter.

use super::super::invariant_error;
use std::fmt::Write as _;
use zryna_diagnostics::Diagnostic;
use zryna_native_mir::native_c_v0::VerifiedMirProgram;

pub(super) const MAGIC: i64 = 0x5a43_4841_4e44_4c30;
pub(super) const RECORDS: i32 = 24;
pub(super) const RECORD_BYTES: i32 = 24;
pub(super) const CONTEXT_BYTES: i32 = RECORDS + 64 * RECORD_BYTES;

pub(super) fn generate(
    program: &VerifiedMirProgram,
    selected: &[usize],
) -> Result<String, Diagnostic> {
    let mut header = String::from(
        "#ifndef ZRYNA_NATIVE_C_HANDLE_CONTEXT_V0_H\n#define ZRYNA_NATIVE_C_HANDLE_CONTEXT_V0_H\n#include <stdint.h>\n#include <stddef.h>\n#include <string.h>\n\
#if !defined(__linux__) || !defined(__x86_64__) || defined(__ILP32__)\n#error Native C handle entries require Linux x86-64 LP64\n#endif\n\
/* Driver-private storage, never passed to a foreign operation. No OS containment is implied. */\n\
struct zryna_c_v0_obligation { uint64_t pointer; uint32_t function, owner, release, state; };\n\
struct zryna_c_v0_context { uint64_t magic; uint32_t busy, live, reserved, poisoned; struct zryna_c_v0_obligation owners[64]; };\n\
struct zryna_c_v0_inputs { uint32_t count, padding; uint32_t values[16]; };\n\
struct zryna_c_v0_outcome { uint32_t tag, operation, status, trap; int32_t value; uint32_t unresolved, reserved, padding; };\n\
_Static_assert(sizeof(void *) == 8 && sizeof(int32_t) == 4, \"private LP64 channel\");\n\
_Static_assert(sizeof(struct zryna_c_v0_obligation) == 24 && offsetof(struct zryna_c_v0_context, owners) == 24, \"ledger offsets\");\n\
_Static_assert(sizeof(struct zryna_c_v0_context) == 1560 && sizeof(struct zryna_c_v0_inputs) == 72 && sizeof(struct zryna_c_v0_outcome) == 32, \"channel sizes\");\n\
_Static_assert(_Alignof(struct zryna_c_v0_context) == 8 && _Alignof(struct zryna_c_v0_obligation) == 8 && _Alignof(struct zryna_c_v0_inputs) == 4 && _Alignof(struct zryna_c_v0_outcome) == 4, \"channel alignments\");\n\
_Static_assert(offsetof(struct zryna_c_v0_context, reserved) == 16 && offsetof(struct zryna_c_v0_obligation, release) == 16 && offsetof(struct zryna_c_v0_obligation, state) == 20 && offsetof(struct zryna_c_v0_inputs, values) == 8 && offsetof(struct zryna_c_v0_outcome, value) == 16 && offsetof(struct zryna_c_v0_outcome, unresolved) == 20 && offsetof(struct zryna_c_v0_outcome, reserved) == 24, \"physical field offsets\");\n\
/* Tags mirror the independently checked MIR protocol. Trap 1 is FOREIGN_RESOURCE_LIMIT. */\n\
static inline void zryna_c_v0_context_initialize(struct zryna_c_v0_context *context) {\n  memset(context, 0, sizeof(*context));\n  context->magic = UINT64_C(0x5a4348414e444c30);\n}\n\
uint32_t zryna_c_v0_i_dispatch(struct zryna_c_v0_context *, const struct zryna_c_v0_inputs *, struct zryna_c_v0_outcome *, uint32_t);\n",
    );
    for (ordinal, function) in program.functions().enumerate() {
        if selected.contains(&ordinal) {
            writeln!(header, "uint32_t {}(struct zryna_c_v0_context *, const struct zryna_c_v0_inputs *, struct zryna_c_v0_outcome *);", function.entry().symbol)
                .map_err(|_| invariant_error())?;
        }
    }
    if super::storage::enabled(program, selected) {
        header = super::storage::header(&header);
    }
    header.push_str("#endif\n");
    Ok(header)
}
