use std::collections::{BTreeMap, BTreeSet};

use sha2::{Digest, Sha256};
use zryna_backend_webassembly::WitWorldAudit;
use zryna_diagnostics::Diagnostic;
use zryna_source::SourceMap;

use super::{INVALID, error, model::Language};

#[derive(Clone, Debug)]
pub(super) enum VerifiedLanguage {
    I32V1 { program: zryna_ir::VerifiedProgram, sources: SourceMap },
}

impl VerifiedLanguage {
    fn source_fingerprint(&self) -> Result<[u8; 32], Vec<Diagnostic>> {
        let Self::I32V1 { program, sources } = self;
        let mut spans = program.functions().flat_map(zryna_ir::VerifiedFunction::expressions);
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
        let Self::I32V1 { program, .. } = self;
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Binding {
    instances: Vec<(String, Vec<ProgramBinding>)>,
    wit: Option<WitWorldAudit>,
}

type ProgramBinding = (Language, [u8; 32], [u8; 32]);

impl Authorities {
    pub(super) fn source_identities(
        &self,
    ) -> BTreeMap<String, Vec<zryna_source::SourceMapIdentity>> {
        self.instances
            .iter()
            .map(|(id, authority)| {
                let identities = authority
                    .programs
                    .iter()
                    .map(|program| {
                        let VerifiedLanguage::I32V1 { sources, .. } = program;
                        sources.identity()
                    })
                    .collect();
                (id.clone(), identities)
            })
            .collect()
    }

    pub(super) fn binding(&self, expected: &BTreeSet<String>) -> Result<Binding, Vec<Diagnostic>> {
        if self.instances.keys().ne(expected.iter()) {
            return Err(vec![error(
                INVALID,
                "verified instance authorities do not match the fixed graph",
            )]);
        }
        let mut instances = Vec::with_capacity(self.instances.len());
        for (id, authority) in &self.instances {
            if authority.programs.len() > 3 {
                return Err(vec![error(
                    INVALID,
                    "instance sealed program authority bound (1..=3) exceeded",
                )]);
            }
            let mut languages = Vec::with_capacity(authority.programs.len());
            for program in &authority.programs {
                languages.push((
                    Language::I32V1,
                    program.source_fingerprint()?,
                    program.fingerprint(),
                ));
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
        Ok(Binding { instances, wit: self.wit.clone() })
    }
}

impl Binding {
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
