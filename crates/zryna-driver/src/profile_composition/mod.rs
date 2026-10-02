//! Fixed-graph verification over sealed source/program and WIT authorities; no host grants.
//!
//! The internal result cannot be constructed or imported by external consumers:
//!
//! ```compile_fail
//! use zryna_driver::profile_composition::ValidatedComposition;
//! ```

mod authority;
mod command_h1;
mod command_source;
mod graph;
mod model;
mod policy;
mod quota;
mod verification;

pub(crate) use command_h1::CommandH1Composition;
pub(crate) use command_source::{PureCommandSource, compile_pure_command};

pub(crate) fn command_registry_ceilings() -> Result<[u64; 10], Vec<Diagnostic>> {
    Ok(policy::Policy::load()?.limits(model::Row::WitCommand))
}

#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};

use zryna_diagnostics::Diagnostic;

use authority::{Authorities, Binding};
use model::{Claim, Input, Requirement, Summary};

const INVALID: &str = "ZRYNA-C4010";
const UNSUPPORTED: &str = "ZRYNA-C4011";
const PROFILE: &str = "ZRYNA-C4012";
const FORBIDDEN: &str = "ZRYNA-C4013";
const RESOURCE: &str = "ZRYNA-C4014";
const EXHAUSTED: &str = "ZRYNA-C4015";

fn error(code: &str, message: impl Into<String>) -> Diagnostic {
    Diagnostic::error(code, None, message, "correct the fixed composition input and revalidate")
}

#[derive(Default)]
struct Report(Vec<Diagnostic>);

impl Report {
    fn push(&mut self, diagnostic: Diagnostic) -> bool {
        if self.0.len() == 255 {
            self.0.push(error(EXHAUSTED, "composition diagnostic limit 255 exceeded at 256"));
            return false;
        }
        self.0.push(diagnostic);
        true
    }

    fn finish(self) -> Result<(), Vec<Diagnostic>> {
        if self.0.is_empty() { Ok(()) } else { Err(self.0) }
    }
}

// Only verification constructs this result. It deliberately exposes neither dispatch nor grants.
#[derive(Debug)]
struct ValidatedComposition {
    input: Input,
    authorities: Binding,
    summaries: BTreeMap<String, Summary>,
    witnesses: BTreeMap<Requirement, Vec<String>>,
}

impl ValidatedComposition {
    #[allow(
        dead_code,
        reason = "private composition verification precedes separately reviewed integration"
    )]
    fn revalidate(&self, input: &Input, authorities: &Authorities) -> Result<(), Vec<Diagnostic>> {
        let graph = graph::validate(input)?;
        let binding = authorities.binding(&graph.ids())?;
        if self.input != graph.input || self.authorities != binding {
            return Err(vec![error(INVALID, "composition input binding changed")]);
        }
        let claim = Claim {
            binding: graph.binding(&binding)?,
            summaries: self.summaries.clone(),
            witnesses: self.witnesses.clone(),
        };
        verify(input, authorities, &claim).map(|_| ())
    }

    #[allow(
        dead_code,
        reason = "private composition verification precedes separately reviewed integration"
    )]
    fn requirements(&self) -> &BTreeSet<Requirement> {
        &self.summaries[&self.input.root].requirements
    }
}

#[allow(
    dead_code,
    reason = "private composition verification precedes separately reviewed integration"
)]
fn verify(
    input: &Input,
    authorities: &Authorities,
    claim: &Claim,
) -> Result<ValidatedComposition, Vec<Diagnostic>> {
    verification::verify(input, authorities, claim)
}
