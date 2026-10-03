//! Independent original-name/import inventory and unused-template call graph.

use super::{Failure, budget, calls, raw, reject, reserve};
use zryna_source::{NormalizedSourcePath, resolve_explicit_zry_import};
use zryna_syntax::v5::{
    RawDataDeclarationKind, RawExpressionKind, RawSourceUnit, VerifiedProjectSyntaxV5,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Target {
    Data(u32, u32),
    Function(u32, u32),
}

struct Name<'a> {
    text: &'a str,
    target: Target,
}

pub(super) struct Originals<'a> {
    pub units: &'a [RawSourceUnit],
    names: Vec<Vec<Name<'a>>>,
}

pub(super) fn data_name(data: &zryna_syntax::v5::RawDataDeclaration) -> &str {
    match &data.kind {
        RawDataDeclarationKind::Struct { name, .. } | RawDataDeclarationKind::Enum { name, .. } => {
            &name.text
        }
    }
}

fn folded(left: &str, right: &str) -> std::cmp::Ordering {
    left.bytes()
        .map(|byte| byte.to_ascii_lowercase())
        .cmp(right.bytes().map(|byte| byte.to_ascii_lowercase()))
}

impl<'a> Originals<'a> {
    pub fn check(
        program: &raw::Program,
        syntax: &'a VerifiedProjectSyntaxV5,
    ) -> Result<Self, Failure> {
        let units = syntax.files();
        if units.len() != program.modules.len() {
            return Err(reject("authenticated module inventory differs from raw claims"));
        }
        let mut names = reserve(units.len())?;
        let mut next = 0;
        for (unit, module) in units.iter().zip(&program.modules) {
            if unit.id != module.id || unit.functions.len() != module.functions as usize {
                return Err(reject(
                    "claimed original function/module inventory differs from syntax",
                ));
            }
            let mut local = reserve(unit.data_declarations.len() + unit.functions.len())?;
            for (index, data) in unit.data_declarations.iter().enumerate() {
                local.push(Name {
                    text: data_name(data),
                    target: Target::Data(
                        unit.id,
                        u32::try_from(index).map_err(|_| Failure::InternalFailure)?,
                    ),
                });
            }
            for (index, function) in unit.functions.iter().enumerate() {
                let claim = program
                    .declarations
                    .get(next)
                    .ok_or_else(|| reject("missing authenticated original function"))?;
                if claim.span != function.span
                    || claim.parameters as usize
                        != function.type_parameters.as_ref().map_or(0, |list| list.parameters.len())
                {
                    return Err(reject(
                        "claimed original range or generic arity differs from syntax",
                    ));
                }
                local.push(Name {
                    text: &function.name.text,
                    target: Target::Function(
                        unit.id,
                        u32::try_from(index).map_err(|_| Failure::InternalFailure)?,
                    ),
                });
                next += 1;
            }
            local.sort_unstable_by(|left, right| folded(left.text, right.text));
            names.push(local);
        }
        let mut result = Self { units, names };
        result.imports()?;
        result.calls(program)?;
        Ok(result)
    }

    pub fn resolve(&self, module: u32, text: &str) -> Result<Target, Failure> {
        let names =
            self.names.get(module as usize).ok_or_else(|| reject("unknown source module"))?;
        let index = names
            .binary_search_by(|name| folded(name.text, text))
            .map_err(|_| reject("source name has no exact original declaration binding"))?;
        let name = &names[index];
        if name.text != text {
            return Err(reject("source name differs from its exact declared/imported spelling"));
        }
        Ok(name.target)
    }

    fn imports(&mut self) -> Result<(), Failure> {
        let mut paths = reserve(self.units.len())?;
        paths.extend(self.units.iter().map(|unit| (unit.path.as_str(), unit.id)));
        paths.sort_unstable_by_key(|row| row.0);
        for unit in self.units {
            let origin = NormalizedSourcePath::new(unit.path.clone())
                .map_err(|_| reject("authenticated module path is not portable"))?;
            for import in &unit.imports {
                let path = resolve_explicit_zry_import(&origin, &import.specifier.text)
                    .map_err(|_| reject("source import path is not an exact portable .zry path"))?;
                let index =
                    paths.binary_search_by(|row| row.0.cmp(path.as_str())).map_err(|_| {
                        reject("source import is absent from the authenticated closure")
                    })?;
                let target_unit = &self.units[paths[index].1 as usize];
                for binding in &import.bindings {
                    // Imports bind original exports, never another import alias/re-export.
                    let target = original_export(target_unit, &binding.imported.text)?;
                    let names = &mut self.names[unit.id as usize];
                    names.try_reserve(1).map_err(|_| Failure::AllocationFailure)?;
                    names.push(Name { text: &binding.local.text, target });
                }
            }
            let names = &mut self.names[unit.id as usize];
            names.sort_unstable_by(|left, right| folded(left.text, right.text));
            if names.windows(2).any(|pair| folded(pair[0].text, pair[1].text).is_eq()) {
                return Err(reject(
                    "original names/import aliases collide under portable case folding",
                ));
            }
        }
        Ok(())
    }

    fn calls(&self, program: &raw::Program) -> Result<(), Failure> {
        let mut edges = reserve(65536)?;
        let mut owner = 0;
        for unit in self.units {
            for function in &unit.functions {
                for expression in &function.body.expressions {
                    if let RawExpressionKind::Call { callee, type_arguments, .. } = &expression.kind
                    {
                        let Target::Function(module, index) =
                            self.resolve(unit.id, &callee.text)?
                        else {
                            return Err(reject("source call target is not an original function"));
                        };
                        let target = &self.units[module as usize].functions[index as usize];
                        if type_arguments.as_ref().map_or(0, |args| args.arguments.len())
                            != target
                                .type_parameters
                                .as_ref()
                                .map_or(0, |list| list.parameters.len())
                        {
                            return Err(reject(
                                "source call generic arity differs from its original",
                            ));
                        }
                        let target = program
                            .declarations
                            .binary_search_by_key(&(module, index), |row| {
                                (row.module, row.function)
                            })
                            .map_err(|_| Failure::InternalFailure)?;
                        if edges.len() == 65536 {
                            return Err(budget("original source call edge limit exceeded"));
                        }
                        edges.push((owner, target));
                    }
                }
                owner += 1;
            }
        }
        calls::check(program.declarations.len(), &edges)
    }
}

fn original_export(unit: &RawSourceUnit, name: &str) -> Result<Target, Failure> {
    if let Some((index, _)) = unit
        .data_declarations
        .iter()
        .enumerate()
        .find(|(_, data)| data_name(data) == name && data.export_span.is_some())
    {
        return Ok(Target::Data(
            unit.id,
            u32::try_from(index).map_err(|_| Failure::InternalFailure)?,
        ));
    }
    if let Some((index, _)) = unit
        .functions
        .iter()
        .enumerate()
        .find(|(_, function)| function.name.text == name && function.export_span.is_some())
    {
        return Ok(Target::Function(
            unit.id,
            u32::try_from(index).map_err(|_| Failure::InternalFailure)?,
        ));
    }
    Err(reject("import does not bind one exact original exported declaration"))
}
