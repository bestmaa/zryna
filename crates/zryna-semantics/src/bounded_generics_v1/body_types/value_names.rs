use zryna_syntax::v4::RawIdentifierSyntax;

use super::model::Ty;
use super::{BodyTypeFailure, Checker, resources};

#[derive(Clone, Copy)]
pub(super) struct Binding<'a> {
    pub(super) name: &'a RawIdentifierSyntax,
    pub(super) ty: Option<Ty>,
}

pub(super) struct Scope<'a> {
    bindings: Vec<Binding<'a>>,
    starts: Vec<usize>,
}

impl<'a> Scope<'a> {
    pub(super) fn new(count: usize, depth: usize) -> Result<Self, BodyTypeFailure> {
        Ok(Self { bindings: resources::reserve(count)?, starts: resources::reserve(depth)? })
    }

    pub(super) fn enter(&mut self) {
        self.starts.push(self.bindings.len());
    }

    pub(super) fn leave(&mut self) {
        self.bindings.truncate(self.starts.pop().expect("original lexical scope"));
    }

    pub(super) fn get(&self, name: &str) -> Option<Binding<'a>> {
        self.bindings.iter().rev().find(|binding| binding.name.text == name).copied()
    }

    pub(super) fn bind(&mut self, checker: &mut Checker<'_, '_>, binding: Binding<'a>) {
        let start = self.starts.last().copied().unwrap_or(0);
        if self.bindings[start..]
            .iter()
            .any(|existing| existing.name.text.eq_ignore_ascii_case(&binding.name.text))
        {
            let span = checker.span(binding.name.span);
            checker.names.at(
                "ZRYNA-M3002",
                span,
                format!("binding '{}' collides under portable naming rules", binding.name.text),
                "give bindings exact unique portable names within their lexical scope",
            );
        }
        self.bindings.push(binding);
    }
}

pub(super) fn missing(checker: &mut Checker<'_, '_>, name: &RawIdentifierSyntax) {
    let span = checker.span(name.span);
    checker.names.at(
        "ZRYNA-M3002",
        span,
        format!("name '{}' is not declared", name.text),
        "reference one exact parameter, local, or match payload binding",
    );
}
