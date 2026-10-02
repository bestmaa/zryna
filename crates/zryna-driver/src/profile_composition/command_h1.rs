//! One-node command composition retains semantics, factory and exact root approval.
//! No request value or grant-file path enters this authority or its content binding.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

use sha2::{Digest, Sha256};
use zryna_backend_webassembly::{ValidatedCommandH1Artifact, pinned_wit_sources};
use zryna_diagnostics::Diagnostic;
use zryna_ir::command_h1_v1::ProgramIdentity;
use zryna_semantics::command_h1_v1::VerifiedProgram;
use zryna_source::SourceMap;

use super::{
    INVALID, ValidatedComposition,
    authority::{Authorities, InstanceAuthority, VerifiedLanguage},
    error, graph,
    model::{
        Capability, Claim, Input, Instance, Language, POLICY, Requirement, Reservation, Row,
        Selection, Summary, VERSION,
    },
    verify,
};

const ROOT: &str = "command-source";
const WORLD: &str = "zryna:capability-profiles/command@0.1.0";
const ENVIRONMENT: &str = "wasi:cli/environment@0.2.12";
const BINDING_DOMAIN: &[u8] = b"zryna.command-composition-binding.v1\0";

#[cfg(test)]
mod tests;

fn invalid() -> Vec<Diagnostic> {
    vec![error(INVALID, "command composition source, issuer, approval or requirement changed")]
}

/// Retains real semantic lowering rather than treating a body hash as source meaning.
#[derive(Clone, Debug)]
pub(super) struct CommandAuthority {
    program: VerifiedProgram,
    sources: SourceMap,
    binding: CommandBinding,
    artifact_binding: [u8; 32],
}

#[derive(Clone, Eq, PartialEq)]
pub(super) struct CommandBinding {
    issuer: ProgramIdentity,
    key: Option<String>,
    approved_key: Option<String>,
}

impl fmt::Debug for CommandBinding {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Issuer equality authenticates a live object; its serial is not stable content.
        formatter
            .debug_struct("CommandBinding")
            .field("version", &"zryna.command-composition-binding.v1")
            .field("key", &self.key)
            .field("approved_key", &self.approved_key)
            .field("static_quota", &self.quota())
            .finish_non_exhaustive()
    }
}

impl CommandAuthority {
    fn new(
        program: &VerifiedProgram,
        artifact: &ValidatedCommandH1Artifact,
        sources: &SourceMap,
        approved_key: Option<&str>,
    ) -> Result<Self, Vec<Diagnostic>> {
        let ir = program.verified_ir();
        let key =
            ir.source().environment().map(zryna_syntax::command_h1_v1::EnvironmentRequirement::key);
        if !ir.source().is_bound_to(sources)
            || key != approved_key
            || ir.identity() != artifact.issuer()
        {
            return Err(invalid());
        }
        artifact
            .revalidate(ir, program.runtime_abi(), sources, &pinned_wit_sources())
            .map_err(|_| invalid())?;
        Ok(Self {
            program: program.clone(),
            sources: sources.clone(),
            binding: CommandBinding {
                issuer: ir.identity(),
                key: key.map(str::to_owned),
                approved_key: approved_key.map(str::to_owned),
            },
            artifact_binding: *artifact.program_binding(),
        })
    }

    pub(super) fn sources(&self) -> Result<&SourceMap, Vec<Diagnostic>> {
        let ir = self.program.verified_ir();
        if !ir.source().is_bound_to(&self.sources)
            || ir.identity() != self.binding.issuer
            || ir
                .source()
                .environment()
                .map(zryna_syntax::command_h1_v1::EnvironmentRequirement::key)
                != self.binding.key.as_deref()
            || self.binding.key != self.binding.approved_key
        {
            return Err(invalid());
        }
        Ok(&self.sources)
    }

    pub(super) const fn artifact_binding(&self) -> &[u8; 32] {
        &self.artifact_binding
    }

    pub(super) fn binding(&self) -> CommandBinding {
        self.binding.clone()
    }
}

impl CommandBinding {
    fn requirements(&self) -> BTreeSet<Requirement> {
        if self.key.is_some() { BTreeSet::from([environment()]) } else { BTreeSet::new() }
    }

    /// A static worst-case reservation: one 64-byte key plus a 1024-byte value.
    /// Missing and present-empty input share this source-derived reservation.
    pub(super) fn quota(&self) -> [u64; 10] {
        let mut quota = [0; 10];
        if self.key.is_some() {
            quota[2] = 1;
            quota[3] = 1088;
        }
        quota
    }
}

fn environment() -> Requirement {
    Requirement { capability: Capability::Environment, interface: ENVIRONMENT.to_owned() }
}

pub(super) fn validate_requirements(
    input: &Input,
    commands: &BTreeMap<String, CommandBinding>,
) -> Result<(), Vec<Diagnostic>> {
    if commands.is_empty() {
        return Ok(());
    }
    let [node] = input.instances.as_slice() else { return Err(invalid()) };
    let [selection] = input.selections.as_slice() else { return Err(invalid()) };
    let Some(command) = commands.get(&node.id) else { return Err(invalid()) };
    let required = command.requirements();
    let restrictions = required.iter().map(|requirement| requirement.capability).collect();
    if commands.len() != 1
        || input.language != Language::CommandH1V1
        || input.root != node.id
        || !input.edges.is_empty()
        || node.rows != BTreeSet::from([Row::WitCommand])
        || node.requirements != required
        || node.restrictions != restrictions
        || node.reservation != Reservation::default()
        || selection.row != Row::WitCommand
        || selection.policy_version != POLICY
        || selection.world.as_deref() != Some(WORLD)
        || selection.approved != required
        || selection.ceilings != command.quota()
        || command.key != command.approved_key
    {
        return Err(invalid());
    }
    Ok(())
}

/// An immutable composition result. It approves neither host input nor execution.
pub(crate) struct CommandH1Composition {
    input: Input,
    authorities: Authorities,
    composition: ValidatedComposition,
    identity: [u8; 32],
    required_key: Option<String>,
    static_quota: [u64; 10],
}

impl CommandH1Composition {
    pub(crate) fn admit(
        program: &VerifiedProgram,
        artifact: &ValidatedCommandH1Artifact,
        sources: &SourceMap,
        approved_key: Option<&str>,
    ) -> Result<Self, Vec<Diagnostic>> {
        let command = CommandAuthority::new(program, artifact, sources, approved_key)?;
        let requirements = command.binding.requirements();
        let static_quota = command.binding.quota();
        let input = input(&requirements, static_quota);
        let authorities = authorities(command, artifact);
        let graph = graph::validate(&input)?;
        let binding = graph.binding(&authorities.binding(&graph.ids())?)?;
        let claim = Claim {
            binding,
            summaries: BTreeMap::from([(
                ROOT.to_owned(),
                Summary { requirements: requirements.clone(), quota: static_quota },
            )]),
            witnesses: requirements
                .into_iter()
                .map(|requirement| (requirement, vec![ROOT.to_owned()]))
                .collect(),
        };
        let composition = verify(&input, &authorities, &claim)?;
        Ok(Self {
            input,
            authorities,
            composition,
            identity: identity(binding),
            required_key: approved_key.map(str::to_owned),
            static_quota,
        })
    }

    pub(crate) const fn identity(&self) -> &[u8; 32] {
        &self.identity
    }

    pub(crate) fn required_key(&self) -> Option<&str> {
        self.required_key.as_deref()
    }

    /// Source-derived static quota, independent of captured host values.
    pub(crate) const fn quota(&self) -> [u64; 10] {
        self.static_quota
    }

    pub(crate) fn revalidate(
        &self,
        program: &VerifiedProgram,
        artifact: &ValidatedCommandH1Artifact,
        sources: &SourceMap,
        approved_key: Option<&str>,
    ) -> Result<(), Vec<Diagnostic>> {
        let command = CommandAuthority::new(program, artifact, sources, approved_key)?;
        if command.binding.key.as_deref() != self.required_key()
            || command.binding.quota() != self.static_quota
        {
            return Err(invalid());
        }
        let authorities = authorities(command, artifact);
        self.composition.revalidate(&self.input, &authorities)?;
        let graph = graph::validate(&self.input)?;
        let binding = authorities.binding(&graph.ids())?;
        if binding != self.authorities.binding(&graph.ids())?
            || identity(graph.binding(&binding)?) != self.identity
        {
            return Err(invalid());
        }
        Ok(())
    }
}

fn identity(binding: [u8; 32]) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(BINDING_DOMAIN);
    digest.update(binding);
    digest.finalize().into()
}

fn authorities(command: CommandAuthority, artifact: &ValidatedCommandH1Artifact) -> Authorities {
    Authorities {
        instances: BTreeMap::from([(
            ROOT.to_owned(),
            InstanceAuthority { programs: vec![VerifiedLanguage::CommandH1V1(Box::new(command))] },
        )]),
        wit: Some(artifact.world().clone()),
    }
}

fn input(requirements: &BTreeSet<Requirement>, quota: [u64; 10]) -> Input {
    Input {
        version: VERSION.to_owned(),
        root: ROOT.to_owned(),
        language: Language::CommandH1V1,
        selections: vec![Selection {
            row: Row::WitCommand,
            policy_version: POLICY.to_owned(),
            world: Some(WORLD.to_owned()),
            approved: requirements.clone(),
            ceilings: quota,
        }],
        instances: vec![Instance {
            id: ROOT.to_owned(),
            rows: BTreeSet::from([Row::WitCommand]),
            requirements: requirements.clone(),
            restrictions: requirements.iter().map(|requirement| requirement.capability).collect(),
            reservation: Reservation::default(),
        }],
        edges: Vec::new(),
    }
}
