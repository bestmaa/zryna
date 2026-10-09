//! Complete immutable local resolution and source-bound function checking.

use super::{Binding, BodyError, FlowStep, Frame, TypedFunction, Value, ValueType};
use zryna_source::FileId;
use zryna_syntax::native_c_source_v0::raw as syntax;

impl<'a> Frame<'a> {
    pub(super) fn new(
        declarations: &'a super::super::VerifiedDeclarationSet,
        file: FileId,
        function: &'a syntax::Function,
    ) -> Result<Self, BodyError> {
        let mut frame = Self {
            declarations,
            file,
            function,
            bindings: Vec::new(),
            names: std::collections::BTreeMap::default(),
            tokens: Vec::new(),
            calls: Vec::new(),
            owners: Vec::new(),
            typed: vec![None; function.expressions.len()],
            steps: Vec::new(),
            guard_call: None,
        };
        let result = ValueType::from(function.result);
        if !result.boundary() {
            return Err(frame.resource_error(function.range, "foreign-token-return"));
        }
        if function.exported && !result.scalar() {
            return Err(frame.type_error(function.range, "owned-export-result"));
        }
        for parameter in &function.parameters {
            let ty = ValueType::from(parameter.ty);
            if !ty.boundary() {
                return Err(frame.resource_error(parameter.range, "foreign-token-parameter"));
            }
            if function.exported && !ty.scalar() {
                return Err(frame.type_error(parameter.range, "owned-export-parameter"));
            }
            frame.bind(parameter, Value::plain(ty))?;
        }
        Ok(frame)
    }

    pub(super) fn bind(
        &mut self,
        binding: &syntax::Binding,
        mut value: Value,
    ) -> Result<(), BodyError> {
        if self.names.contains_key(&binding.name) {
            return Err(self.source_error(binding.range, "duplicate-body-binding"));
        }
        let ty = ValueType::from(binding.ty);
        if ty != value.ty {
            return Err(self.type_error(binding.range, "binding-type"));
        }
        if self.function.exported && !ty.scalar() {
            return Err(self.type_error(binding.range, "owned-export-local"));
        }
        if value.ty.linear()
            && let Some(previous) = value.binding
        {
            self.bindings[previous].available = false;
        }
        let index = self.bindings.len();
        value.binding = Some(index);
        self.bindings.push(Binding { value, available: true });
        self.names.insert(binding.name.clone(), index);
        Ok(())
    }

    pub(super) fn local(&self, name: &str, range: syntax::Range) -> Result<Value, BodyError> {
        let index =
            *self.names.get(name).ok_or_else(|| self.source_error(range, "unknown-local"))?;
        let binding = &self.bindings[index];
        if !binding.available {
            return Err(self.resource_error(range, "moved-binding"));
        }
        if let Some(token) = binding.value.token
            && !self.tokens[token].live
        {
            return Err(self.resource_error(range, "stale-token"));
        }
        Ok(binding.value.clone())
    }

    pub(super) fn check(mut self, source_function: usize) -> Result<TypedFunction, BodyError> {
        let mut returned = false;
        let function = self.function;
        for statement in &function.statements {
            if returned {
                return Err(self.source_error(statement.range, "statement-after-terminal-return"));
            }
            match &statement.kind {
                syntax::StatementKind::Const(binding, expression) => {
                    let value = self.evaluate(*expression, 1)?;
                    self.bind(binding, value)?;
                }
                syntax::StatementKind::Expression(expression) => {
                    let value = self.evaluate(*expression, 1)?;
                    if matches!(value.ty, ValueType::Terminal | ValueType::Key) {
                        return Err(
                            self.type_error(statement.range, "nonvalue-expression-statement")
                        );
                    }
                }
                syntax::StatementKind::Guard(status, expression) => {
                    self.guard(status, *expression, statement.range)?;
                }
                syntax::StatementKind::Return(expression) => {
                    let value = self.evaluate(*expression, 1)?;
                    if value.ty != ValueType::from(self.function.result) {
                        return Err(self.type_error(statement.range, "function-return-type"));
                    }
                    if self.calls.iter().any(|call| call.status_mode && !call.proved_zero) {
                        return Err(self.resource_error(statement.range, "unhandled-call-status"));
                    }
                    self.steps.push(FlowStep::Return {
                        expression: *expression,
                        ty: value.ty,
                        cleanup: self.cleanup(None),
                    });
                    returned = true;
                }
            }
        }
        if !returned {
            return Err(self.source_error(self.function.range, "missing-terminal-return"));
        }
        let export_operation = self.export_operation()?;
        let expressions = self.typed.into_iter().collect::<Option<Vec<_>>>().ok_or(BodyError {
            code: "ZRYNA-C4106",
            detail: "unvisited-source-expression",
            span: None,
        })?;
        Ok(TypedFunction {
            file: self.file,
            source_function,
            name: self.function.name.clone(),
            result: ValueType::from(self.function.result),
            expressions,
            steps: self.steps,
            owners: self.owners.into_iter().map(|inner| super::OwnerOrigin { inner }).collect(),
            source_range: self.function.range,
            parameters: self.function.parameters.clone(),
            statements: self.function.statements.clone(),
            export_operation,
        })
    }

    fn export_operation(&self) -> Result<Option<usize>, BodyError> {
        if self.function.exported {
            let file = self
                .declarations
                .syntax
                .files()
                .iter()
                .find(|file| file.file_id() == self.file)
                .ok_or_else(|| {
                    self.source_error(self.function.range, "body-function-source-file")
                })?;
            Ok(Some(
                self.declarations
                    .declarations
                    .operations
                    .iter()
                    .position(|operation| {
                        operation.direction == zryna_syntax::native_c_v0::raw::Direction::Export
                            && operation.source_binding.path == file.path()
                            && operation.source_binding.start
                                == u64::from(self.function.range.start)
                            && operation.source_binding.end == u64::from(self.function.range.end)
                    })
                    .ok_or_else(|| self.source_error(self.function.range, "body-export-binding"))?,
            ))
        } else {
            Ok(None)
        }
    }
}
