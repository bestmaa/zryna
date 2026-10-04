//! Separate Linux x86-64 private owned representation; older MIR/ABI constructors stay separate.

use zryna_diagnostics::Diagnostic;
use zryna_ir::generic_v1::{owned_v2::VerifiedOwnedProgram, raw};
use zryna_layout::{StorageTarget, TypeCategory, generic_v1::TypeView};

mod plan;

/// Maximum private i64 lanes of one represented type, independent of source admission.
pub const MAX_TYPE_LANES: u32 = 256;
/// Maximum flattened private parameters including the result pointer.
pub const MAX_PARAMETERS: u32 = 256;
/// Maximum complete lane definitions per function.
pub const MAX_VALUE_LANES: u32 = 65_536;
/// Maximum aggregate code generation work across the complete program.
pub const MAX_CODEGEN_UNITS: u64 = 1_000_000;

/// Separate backend authority retaining the exact source-bound owned program and lane plan.
///
/// ```compile_fail
/// fn fake(raw: zryna_ir::generic_v1::raw::Program) {
///     let _: zryna_native_mir::generic_owned_v2::VerifiedProgram<'_, '_> = raw;
/// }
/// ```
#[derive(Debug)]
pub struct VerifiedProgram<'p, 'a> {
    program: &'p VerifiedOwnedProgram<'a>,
    widths: Vec<Option<u32>>,
    functions: Vec<FunctionPlan>,
}

/// Immutable checked private function shape and dominance-safe emission order.
#[derive(Debug)]
pub struct FunctionPlan {
    symbol: String,
    parameters: u32,
    result: u32,
    order: Vec<usize>,
}

impl FunctionPlan {
    /// Canonical internal symbol generated only from the sealed function index.
    #[must_use]
    pub fn symbol(&self) -> &str {
        &self.symbol
    }
    /// Flattened i64 parameters; the private result pointer is additional.
    #[must_use]
    pub const fn parameters(&self) -> u32 {
        self.parameters
    }
    /// Full initialized i64 result width transported through caller-owned stack storage.
    #[must_use]
    pub const fn result(&self) -> u32 {
        self.result
    }
    /// Reachable reverse postorder computed from the sealed graph.
    #[must_use]
    pub fn block_order(&self) -> &[usize] {
        &self.order
    }
}

impl<'p, 'a> VerifiedProgram<'p, 'a> {
    /// Exact stored Linux representation retained by this MIR authority.
    #[must_use]
    pub fn stored_type(&self, index: u32) -> Option<TypeView<'_>> {
        self.program.layouts(StorageTarget::LinuxX8664V1).types().nth(index as usize)
    }
    /// Exact immutable owned authority, with its original layouts/runtime/scalar ABI.
    #[must_use]
    pub const fn program(&self) -> &'p VerifiedOwnedProgram<'a> {
        self.program
    }
    /// Checked private function plans in the sealed canonical key order.
    #[must_use]
    pub fn functions(&self) -> &[FunctionPlan] {
        &self.functions
    }
    /// Width of a stored type, unit or private loan's exact referent representation.
    ///
    /// # Errors
    /// Rejects foreign/unused raw type indices or unsupported referent claims.
    pub fn width(&self, ty: raw::Type) -> Result<u32, Diagnostic> {
        match ty {
            raw::Type::Unit => Ok(0),
            raw::Type::Stored(i) => {
                self.widths.get(i as usize).copied().flatten().ok_or_else(invariant)
            }
            raw::Type::Borrow { referent, .. } => self.width(raw::Type::Stored(referent)),
        }
    }
    /// Resolves the exact direct source root without aliases or guessed symbols.
    ///
    /// # Errors
    /// Rejects an absent source root identity.
    pub fn source_call(&self, module: u32, function: u32) -> Result<usize, Diagnostic> {
        let mut key = [0u8; 9];
        key[0] = 0x41;
        key[1..5].copy_from_slice(&module.to_le_bytes());
        key[5..].copy_from_slice(&function.to_le_bytes());
        self.program
            .functions()
            .binary_search_by(|f| f.key.as_slice().cmp(&key))
            .map_err(|_| invariant())
    }
}

/// Derives bounded Linux layout lanes and complete private shapes from the opaque owned seal.
///
/// No raw graph, equality-only fingerprint or older M3 MIR can enter this boundary.
/// # Errors
/// Rejects checked physical amplification/reservation failure or an unexpected sealed invariant.
pub fn lower<'p, 'a>(
    program: &'p VerifiedOwnedProgram<'a>,
) -> Result<VerifiedProgram<'p, 'a>, Diagnostic> {
    let views = program.layouts(StorageTarget::LinuxX8664V1).types();
    let mut types = Vec::new();
    types.try_reserve(views.len()).map_err(|_| budget())?;
    types.extend(views);
    let mut widths = Vec::new();
    widths.try_reserve(types.len()).map_err(|_| budget())?;
    widths.resize(types.len(), None);
    let mut functions = Vec::new();
    functions.try_reserve(program.functions().len()).map_err(|_| budget())?;
    let mut units = 0;
    for (index, function) in program.functions().iter().enumerate() {
        functions.push(plan::function(index, function, &types, &mut widths, &mut units)?);
        for block in &function.blocks {
            for i in &block.instructions {
                if let Some(zryna_ir::generic_v1::owned_v2::raw::Operation::StringLiteral(bytes)) =
                    program.extension(index, i.result.id)
                {
                    plan::take(
                        &mut units,
                        (bytes.len() as u64).checked_mul(64).ok_or_else(budget)?,
                    )?;
                }
            }
        }
        for step in &program.plan(index).ok_or_else(invariant)?.steps {
            plan::take(
                &mut units,
                (step.cleanup.len() as u64).checked_mul(256).ok_or_else(budget)?,
            )?;
        }
    }
    Ok(VerifiedProgram { program, widths, functions })
}

fn width(
    ty: raw::Type,
    types: &[TypeView<'_>],
    cache: &mut [Option<u32>],
    depth: usize,
) -> Result<u32, Diagnostic> {
    if let raw::Type::Borrow { referent, .. } = ty {
        return width(raw::Type::Stored(referent), types, cache, depth);
    }
    let raw::Type::Stored(i) = ty else {
        return if ty == raw::Type::Unit { Ok(0) } else { Err(invariant()) };
    };
    if let Some(n) = cache.get(i as usize).copied().flatten() {
        return Ok(n);
    }
    if depth > zryna_layout::MAX_TRAVERSAL_DEPTH {
        return Err(budget());
    }
    let ty = types.get(i as usize).copied().ok_or_else(invariant)?;
    let mut child = |id: zryna_layout::generic_v1::TypeId| {
        width(raw::Type::Stored(id.index()), types, cache, depth + 1)
    };
    let n = match ty.category() {
        TypeCategory::Bool | TypeCategory::I32 => 1,
        TypeCategory::String => 3,
        TypeCategory::Enum => {
            let mut n = 0;
            for (_, payload) in ty.variants() {
                if let Some(payload) = payload {
                    n = n.max(child(payload)?);
                }
            }
            n.checked_add(1).ok_or_else(budget)?
        }
        TypeCategory::Struct => {
            let mut n = 0u32;
            for (_, field, _) in ty.fields() {
                n = n.checked_add(child(field)?).ok_or_else(budget)?;
            }
            n
        }
        TypeCategory::FixedArray => {
            let element = ty.referenced_type().ok_or_else(invariant)?;
            let lanes = child(element)?;
            let physical = types.get(element.index() as usize).ok_or_else(invariant)?;
            let stride =
                physical.size().checked_add(physical.alignment() - 1).ok_or_else(budget)?
                    / physical.alignment()
                    * physical.alignment();
            if lanes == 0 {
                0
            } else {
                if stride == 0 || ty.size() % stride != 0 {
                    return Err(invariant());
                }
                lanes
                    .checked_mul(u32::try_from(ty.size() / stride).map_err(|_| budget())?)
                    .ok_or_else(budget)?
            }
        }
        _ => return Err(invariant()),
    };
    if n > MAX_TYPE_LANES {
        return Err(budget());
    }
    cache[i as usize] = Some(n);
    Ok(n)
}

fn budget() -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-N7001",
        None,
        "generic native representation exceeds its checked budget",
        "reduce physical type, parameter or code amplification",
    )
}
fn invariant() -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-N7002",
        None,
        "generic native MIR rejected a sealed invariant",
        "report the smallest reproducible source",
    )
}
