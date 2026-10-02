use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use sha2::{Digest, Sha256};
use zryna_backend_webassembly::WitWorldAudit;
use zryna_diagnostics::Diagnostic;
use zryna_source::SourceMap;

use super::{INVALID, command_h1::CommandAuthority, error, model::Language};

#[derive(Clone, Debug)]
pub(super) enum VerifiedLanguage {
    I32V1 { program: zryna_ir::VerifiedProgram, sources: SourceMap },
    CommandH1V1(Box<CommandAuthority>),
}

impl VerifiedLanguage {
    fn source_fingerprint(&self) -> Result<[u8; 32], Vec<Diagnostic>> {
        let sources = match self {
            Self::I32V1 { program, sources } => {
                let mut spans =
                    program.functions().flat_map(zryna_ir::VerifiedFunction::expressions);
                let first = spans.next();
                let valid = first.is_some()
                    && first
                        .into_iter()
                        .chain(spans)
                        .all(|expression| sources.resolve(expression.span).is_ok());
                if !valid {
                    return Err(vec![error(
                        INVALID,
                        "verified language program is not bound to its exact source authority",
                    )]);
                }
                sources
            }
            Self::CommandH1V1(command) => command.sources()?,
        };
        let mut bytes = Vec::new();
        for index in 0..sources.len() {
            let id = sources
                .verify_file_id(u32::try_from(index).expect("bounded source map"))
                .map_err(|_| vec![error(INVALID, "cannot bind verified source authority")])?;
            let source = sources.source(id).expect("verified dense source id");
            bytes.extend_from_slice(source.path().as_str().as_bytes());
            bytes.push(0);
            bytes.extend_from_slice(source.text().as_bytes());
            bytes.push(0xff);
        }
        Ok(Sha256::digest(bytes).into())
    }

    fn fingerprint(&self) -> [u8; 32] {
        let program = match self {
            Self::I32V1 { program, .. } => program,
            Self::CommandH1V1(command) => return *command.artifact_binding(),
        };
        let mut bytes = Vec::new();
        for function in program.functions() {
            bytes.extend_from_slice(function.export_name().as_str().as_bytes());
            bytes.push(0);
            bytes.extend_from_slice(format!("{:?}", function.parameters()).as_bytes());
            bytes.extend_from_slice(format!("{:?}", function.return_type()).as_bytes());
            bytes.extend_from_slice(
                &serde_json::to_vec(function.expressions())
                    .expect("verified scalar expressions serialize"),
            );
            bytes.extend_from_slice(format!("{:?}", function.body()).as_bytes());
        }
        Sha256::digest(bytes).into()
    }

    fn language(&self) -> Language {
        match self {
            Self::I32V1 { .. } => Language::I32V1,
            Self::CommandH1V1(_) => Language::CommandH1V1,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct InstanceAuthority {
    pub programs: Vec<VerifiedLanguage>,
}

#[derive(Clone, Debug)]
pub(super) struct Authorities {
    pub instances: BTreeMap<String, InstanceAuthority>,
    pub wit: Option<WitWorldAudit>,
}

#[derive(Clone, Eq, PartialEq)]
pub(super) struct Binding {
    instances: Vec<(String, Vec<ProgramBinding>)>,
    wit: Option<WitWorldAudit>,
    commands: BTreeMap<String, super::command_h1::CommandBinding>,
}

// Preserve the historical I32-only representation consumed by Graph::binding.
impl fmt::Debug for Binding {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut fields = formatter.debug_struct("Binding");
        fields.field("instances", &self.instances).field("wit", &self.wit);
        if !self.commands.is_empty() {
            fields.field("command_h1_v1", &self.commands);
        }
        fields.finish()
    }
}

type ProgramBinding = (Language, [u8; 32], [u8; 32]);

impl Authorities {
    pub(super) fn binding(&self, expected: &BTreeSet<String>) -> Result<Binding, Vec<Diagnostic>> {
        if self.instances.keys().ne(expected.iter()) {
            return Err(vec![error(
                INVALID,
                "verified instance authorities do not match the fixed graph",
            )]);
        }
        let mut instances = Vec::with_capacity(self.instances.len());
        let mut commands = BTreeMap::new();
        for (id, authority) in &self.instances {
            if authority.programs.len() > 3 {
                return Err(vec![error(
                    INVALID,
                    "instance sealed program authority bound (1..=3) exceeded",
                )]);
            }
            if authority.programs.len() != 1
                && authority
                    .programs
                    .iter()
                    .any(|program| matches!(program, VerifiedLanguage::CommandH1V1(_)))
            {
                return Err(vec![error(INVALID, "command requires its sole semantic authority")]);
            }
            let mut languages = Vec::with_capacity(authority.programs.len());
            for program in &authority.programs {
                languages.push((
                    program.language(),
                    program.source_fingerprint()?,
                    program.fingerprint(),
                ));
                if let VerifiedLanguage::CommandH1V1(command) = program {
                    commands.insert(id.clone(), command.binding());
                }
            }
            languages.sort_by_key(|(language, _, _)| *language);
            if languages.is_empty() || languages.windows(2).any(|pair| pair[0].0 == pair[1].0) {
                return Err(vec![error(
                    INVALID,
                    "instance requires one distinct sealed authority per language",
                )]);
            }
            instances.push((id.clone(), languages));
        }
        Ok(Binding { instances, wit: self.wit.clone(), commands })
    }
}

impl Binding {
    pub(super) fn validate_command_requirements(
        &self,
        input: &super::model::Input,
    ) -> Result<(), Vec<Diagnostic>> {
        super::command_h1::validate_requirements(input, &self.commands)
    }

    pub(super) fn command_quota<'a>(
        &self,
        ids: impl Iterator<Item = &'a str>,
        quota: &mut [u64; 10],
    ) -> Result<(), Vec<Diagnostic>> {
        for id in ids {
            if let Some(command) = self.commands.get(id) {
                for (index, amount) in command.quota().into_iter().enumerate() {
                    quota[index] = quota[index].checked_add(amount).ok_or_else(|| {
                        vec![error(super::RESOURCE, "command reservation arithmetic overflow")]
                    })?;
                }
            }
        }
        Ok(())
    }

    pub(super) fn has_language(&self, id: &str, language: Language) -> bool {
        self.instances
            .iter()
            .find(|(candidate, _)| candidate == id)
            .is_some_and(|(_, programs)| programs.iter().any(|entry| entry.0 == language))
    }

    pub(super) const fn wit(&self) -> Option<&WitWorldAudit> {
        self.wit.as_ref()
    }
}
