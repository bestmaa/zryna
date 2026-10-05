use super::{Builder, Closed, Failure, Owned, RawStatementKind, raw, reject};
impl Builder<'_, '_> {
    pub(super) fn source_block(&mut self, index: u32, depth: usize) -> Result<bool, Failure> {
        if depth > 128 {
            return Err(crate::generic_v1::budget("source replay nesting exceeds 128"));
        }
        let source = &self.original.body.blocks[index as usize];
        let mut returned = false;
        for id in &source.statements {
            if returned {
                return Err(reject("owned executable lane does not admit statements after return"));
            }
            let statement = &self.original.body.statements[*id as usize];
            match &statement.kind {
                RawStatementKind::LocalDeclaration {
                    name,
                    type_syntax,
                    initializer,
                    mutable,
                    ..
                } => {
                    let mut value = self.expression(*initializer, depth + 1)?;
                    if value.ty != self.resolve(*type_syntax)? {
                        return Err(reject(
                            "source initializer differs from its declared symbolic type",
                        ));
                    }
                    if *mutable {
                        if matches!(value.ty, Closed::Borrow(..) | Closed::Unit) {
                            return Err(reject("mutable binding requires a complete stored value"));
                        }
                        // A Copy binding has its own place identity; borrowing it must not
                        // alias a parameter or another immutable observation.
                        if !self.affine[value.id as usize] {
                            value = self.emit(
                                value.ty,
                                statement.span,
                                raw::Operation::Copy { value: value.id },
                            )?;
                        }
                    }
                    self.bind(&name.text, value)?;
                    self.locals.last_mut().ok_or(Failure::InternalFailure)?.mutable = *mutable;
                }
                RawStatementKind::Assignment { target, value, .. } => {
                    self.assign(*target, *value, statement.span, depth + 1)?;
                }
                RawStatementKind::Return { value, .. } => {
                    let at = self.original.body.expressions[*value as usize].span;
                    let value = self.expression(*value, depth + 1)?;
                    if matches!(value.ty, Closed::Borrow(..)) {
                        return Err(self.locate(
                            super::owners::ownership("loan cannot escape function result"),
                            at,
                        ));
                    }
                    if value.ty != self.result {
                        return Err(reject("source return differs from original symbolic result"));
                    }
                    self.blocks[self.block].span = statement.span;
                    self.blocks[self.block].terminator = raw::Terminator::Return(value.id);
                    returned = true;
                }
                RawStatementKind::ExpressionStatement { expression, .. } => {
                    let value = self.expression(*expression, depth + 1)?;
                    if self.affine[value.id as usize] {
                        self.ext(Closed::Unit, statement.span, Owned::Drop(value.id))?;
                    }
                    if matches!(value.ty, Closed::Borrow(..)) {
                        self.ext(Closed::Unit, statement.span, Owned::EndLoan(value.id))?;
                    }
                }
                RawStatementKind::Block { block } => {
                    returned = self.scoped_block(*block, statement.span, depth + 1)?;
                }
                RawStatementKind::If { condition, then_block, else_clause, .. } => {
                    returned = self.branch(
                        *condition,
                        *then_block,
                        else_clause.as_ref().map(|clause| clause.block),
                        statement.span,
                        depth + 1,
                    )?;
                }
                _ => {
                    return Err(reject(
                        "source statement requires a successor lane beyond immutable Copy replay",
                    ));
                }
            }
        }
        if !returned && depth == 0 {
            if self.result != Closed::Unit {
                return Err(reject("source function lacks an exact return"));
            }
            let value = self.emit(Closed::Unit, source.span, raw::Operation::Unit)?;
            self.blocks[self.block].span = source.span;
            self.blocks[self.block].terminator = raw::Terminator::Return(value.id);
        }
        Ok(returned)
    }

    pub(super) fn scoped_block(
        &mut self,
        block: u32,
        span: zryna_source::UntrustedSpan,
        depth: usize,
    ) -> Result<bool, Failure> {
        let start = self.next as usize;
        let locals = self.locals.len();
        let prior = self.scope_start;
        self.scope_start = locals;
        let returned = self.source_block(block, depth)?;
        if !returned {
            // Whole replacement may move a new owner into an outer binding.
            // Its obligation survives this block's temporary/local cleanup.
            let mut retained = crate::generic_v1::reserve(locals)?;
            retained.extend(
                self.locals[..locals].iter().map(|b| b.value.id).filter(|id| *id as usize >= start),
            );
            retained.sort_unstable();
            self.close_scope_except(start, &retained, span)?;
        }
        self.locals.truncate(locals);
        self.scope_start = prior;
        Ok(returned)
    }

    fn assign(
        &mut self,
        target: u32,
        expression: u32,
        span: zryna_source::UntrustedSpan,
        depth: usize,
    ) -> Result<(), Failure> {
        let at = self.original.body.expressions[target as usize].span;
        self.assign_inner(target, expression, span, depth).map_err(|e| self.locate(e, at))
    }
    fn assign_inner(
        &mut self,
        target: u32,
        expression: u32,
        span: zryna_source::UntrustedSpan,
        depth: usize,
    ) -> Result<(), Failure> {
        use super::owners::ownership;
        let zryna_syntax::v5::RawExpressionKind::Reference { name } =
            &self.original.body.expressions[target as usize].kind
        else {
            return Err(reject("replacement requires a complete lexical local"));
        };
        let index = self
            .locals
            .iter()
            .rposition(|b| b.name == name.text)
            .ok_or_else(|| ownership("unknown exact assignment binding"))?;
        let destination = self.locals[index].clone();
        if !destination.mutable || matches!(destination.value.ty, Closed::Borrow(..) | Closed::Unit)
        {
            return Err(ownership("replacement requires a mutable stored binding"));
        }
        if self.loans.values().any(|(root, _)| *root == destination.value.id) {
            return Err(ownership("replacement of a borrowed original place"));
        }
        // Complete the replacement first. Failure plans still own the previous value.
        let mut value = self.expression(expression, depth + 1)?;
        if value.ty != destination.value.ty {
            return Err(reject("replacement differs from original binding type"));
        }
        if !self.affine[value.id as usize] {
            value = self.emit(value.ty, span, raw::Operation::Copy { value: value.id })?;
        }
        if self.affine[destination.value.id as usize] && self.alive[destination.value.id as usize] {
            self.ext(Closed::Unit, span, Owned::Drop(destination.value.id))?;
        }
        self.locals[index].value = value;
        Ok(())
    }
}
