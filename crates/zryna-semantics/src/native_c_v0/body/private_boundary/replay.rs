//! Mandatory complete candidate replay, independent of caller preparation and cleanup claims.

use super::super::{FlowStep, VerifiedForeignBodies};
use super::{BoundaryError, Candidate, PrivatePreparation, cleanup, copies, loans, owners};
use zryna_ir::data_ownership_v1::MAX_VALUES_PER_PROGRAM;
use zryna_source::SourceMap;

pub(super) fn verify(
    sources: &SourceMap,
    bodies: &VerifiedForeignBodies,
    candidate: &Candidate,
) -> Result<(), BoundaryError> {
    if !bodies.belongs_to(sources) {
        return Err(BoundaryError::source("boundary-original-source-map"));
    }
    owners::preflight(bodies.functions())?;
    candidate.layouts.check(sources)?;
    if candidate.functions.len() != bodies.functions().len() {
        return Err(BoundaryError::source("boundary-complete-function-inventory"));
    }
    let mut program_values = 0;
    for (claim, function) in candidate.functions.iter().zip(bodies.functions()) {
        owners::check_candidate_counts(claim)?;
        program_values = owners::checked_count(
            program_values,
            claim.expression_origins.len(),
            MAX_VALUES_PER_PROGRAM,
        )?;
        let source = owners::derive(function, &candidate.layouts)
            .map_err(|error| error.at(bodies, function))?;
        let reject = |detail| BoundaryError::source(detail).at(bodies, function);
        if claim.file != function.file_id()
            || claim.source_function != function.source_function_index()
            || claim.result_category != function.result_type()
            || claim.result_type != candidate.layouts.type_id(function.result_type())?
            || claim.parameter_types.len() != function.parameters().len()
            || claim.expression_origins != source.origins
            || claim.private_owners != source.owners
            || claim.steps.len() != function.steps().len()
        {
            return Err(reject("boundary-original-function-occupants"));
        }
        for (claimed, parameter) in claim.parameter_types.iter().zip(function.parameters()) {
            if *claimed != candidate.layouts.type_id(parameter.ty.into())? {
                return Err(reject("boundary-private-entry-type"));
            }
        }
        let mut registry = cleanup::Registry::entry(&source);
        for (index, (claimed, step)) in claim.steps.iter().zip(function.steps()).enumerate() {
            if claimed.source_step != index {
                return Err(reject("boundary-complete-step-order"));
            }
            let check = match (&claimed.preparation, step) {
                (Some(PrivatePreparation::Loan(loan)), FlowStep::PrepareLoan { .. }) => {
                    loans::check(loan, step, &source, &candidate.layouts)
                }
                (Some(PrivatePreparation::Copy(copy)), FlowStep::Copy { .. }) => {
                    copies::check(copy, step, &candidate.layouts)
                }
                (None, FlowStep::PrepareLoan { .. } | FlowStep::Copy { .. }) | (Some(_), _) => {
                    Err(reject("boundary-private-preparation-inventory"))
                }
                (None, _) => Ok(()),
            };
            check.map_err(|error| error.at(bodies, function))?;
            if claimed.completed != cleanup::Registry::completions(step) {
                return Err(BoundaryError::owned("boundary-completion-origin").at(bodies, function));
            }
            let rules = cleanup::rules(
                step,
                claimed.preparation.as_ref(),
                cleanup::release_call(function.steps(), index),
            );
            cleanup::check(
                &claimed.exits,
                &rules,
                &registry,
                &source,
                cleanup::returned(step, &source),
            )
            .map_err(|error| error.at(bodies, function))?;
            registry.advance(step).map_err(|error| error.at(bodies, function))?;
        }
    }
    Ok(())
}
