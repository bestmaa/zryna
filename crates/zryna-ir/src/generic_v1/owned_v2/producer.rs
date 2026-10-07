//! Raw source-claim producer; only the independent wire/source verifier grants authority.

use crate::generic_v1::{Failure, raw, reject, reserve, source::Originals};
use zryna_layout::generic_v1::VerifiedLayouts;
use zryna_source::SourceMap;
use zryna_syntax::v5::VerifiedProjectSyntaxV5;

/// Produces complete owned successor raw bodies from authenticated source and supplied
/// generic discovery keys. Every nongeneric original root is added automatically.
///
/// The result is raw, must cross the new wire decoder and cannot be supplied to a backend.
///
/// # Errors
/// Rejects unknown keys, signature substitution, unsupported original bodies and demand mismatch.
pub fn produce_claim(
    syntax: &VerifiedProjectSyntaxV5,
    sources: &SourceMap,
    linear: &VerifiedLayouts,
    linux: &VerifiedLayouts,
    generic_keys: &[&[u8]],
) -> Result<super::raw::Program, Failure> {
    if generic_keys.len() > 4096 {
        return Err(crate::generic_v1::budget("raw producer generic key inventory exceeds 4096"));
    }
    if !syntax.is_bound_to(sources) {
        return Err(reject("raw producer syntax belongs to a foreign source map"));
    }
    let mut modules = reserve(syntax.files().len())?;
    let mut declarations = reserve(0)?;
    let mut function_keys = reserve(generic_keys.len())?;
    for key in generic_keys {
        function_keys.push(key.to_vec());
    }
    for unit in syntax.files() {
        modules.push(raw::Module {
            id: unit.id,
            functions: u32::try_from(unit.functions.len()).map_err(|_| Failure::InternalFailure)?,
        });
        for (index, function) in unit.functions.iter().enumerate() {
            let index = u32::try_from(index).map_err(|_| Failure::InternalFailure)?;
            let arity = function.type_parameters.as_ref().map_or(0, |list| list.parameters.len());
            declarations.try_reserve(1).map_err(|_| Failure::AllocationFailure)?;
            declarations.push(raw::Declaration {
                module: unit.id,
                function: index,
                parameters: u32::try_from(arity).map_err(|_| Failure::InternalFailure)?,
                span: function.span,
            });
            if arity == 0 {
                let mut key = reserve(9)?;
                key.push(0x41);
                key.extend_from_slice(&unit.id.to_le_bytes());
                key.extend_from_slice(&index.to_le_bytes());
                function_keys.try_reserve(1).map_err(|_| Failure::AllocationFailure)?;
                function_keys.push(key);
            }
        }
    }
    function_keys.sort();
    if function_keys.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(reject("raw producer received duplicate generic discovery keys"));
    }
    let mut type_keys = reserve(linear.types().len())?;
    for ty in linear.types() {
        type_keys.push(ty.key().to_vec());
    }
    let mut program = raw::Program {
        modules,
        declarations,
        type_keys,
        universe: *linear.universe_identity(),
        linear32: *linear.fingerprint(),
        linux_x86_64: *linux.fingerprint(),
        functions: reserve(function_keys.len())?,
    };
    let originals = Originals::check(&program, syntax)?;
    crate::generic_v1::copy_v1::signatures(&mut program, &originals, function_keys)?;
    // Validate key and source/universe claims before indexing source body arenas.
    crate::generic_v1::inventory::check(&program, sources, linear, linux)?;
    crate::generic_v1::substitution::check(&program, linear, &originals)?;
    let extensions = super::source::produce(&mut program, &originals, sources)?;
    let mut claim = super::raw::Program { graph: program, extensions, plans: Vec::new() };
    super::source::check(&claim, &originals, sources)?;
    let inventory = crate::generic_v1::inventory::check(&claim.graph, sources, linear, linux)?;
    super::typed::check(&claim, sources, linear, &inventory)?;
    claim.plans = super::plan::derive(&claim.graph, &claim.extensions, linear)?;
    Ok(claim)
}
