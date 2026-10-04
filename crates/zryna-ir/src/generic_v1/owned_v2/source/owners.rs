use super::{Builder, Closed, Extension, Failure, Owned, Value, raw};
pub(super) fn ownership(message: &str) -> Failure {
    Failure::Diagnostics(vec![zryna_diagnostics::Diagnostic::error(
        "ZRYNA-M7007",
        None,
        message,
        "use complete owners once and end exact loans before consuming their roots",
    )])
}

pub(super) fn affine(ty: &Closed) -> Result<bool, Failure> {
    match ty {
        Closed::Stored(key) => key_affine(key, 0),
        _ => Ok(false),
    }
}
fn key_affine(key: &[u8], depth: usize) -> Result<bool, Failure> {
    if depth > 128 {
        return Err(crate::generic_v1::budget("opaque ownership depth exceeded"));
    }
    let (mut cursor, count) = match key.first() {
        Some(0 | 1) if key.len() == 1 => return Ok(false),
        Some(0x14 | 0x20) => (5usize, 1),
        Some(0x15) => (5usize, 2),
        _ => return Ok(true),
    };
    let mut result = false;
    for _ in 0..count {
        let n = u32::from_le_bytes(
            key.get(cursor..cursor + 4)
                .ok_or(Failure::InternalFailure)?
                .try_into()
                .map_err(|_| Failure::InternalFailure)?,
        ) as usize;
        cursor += 4;
        let end = cursor.checked_add(n).ok_or(Failure::InternalFailure)?;
        result |= key_affine(key.get(cursor..end).ok_or(Failure::InternalFailure)?, depth + 1)?;
        cursor = end;
    }
    if cursor != key.len() {
        return Err(Failure::InternalFailure);
    }
    Ok(result)
}
pub(super) fn core(builder: &mut Builder<'_, '_>, op: &raw::Operation) -> Result<(), Failure> {
    let arguments = match op {
        raw::Operation::ClosedEnumConstruct { payload, .. } => payload.as_slice(),
        raw::Operation::ClosedGenericCall { arguments, .. }
        | raw::Operation::SourceCall { arguments, .. } => arguments.as_slice(),
        _ => &[],
    };
    for id in arguments {
        if builder.affine[*id as usize] {
            builder.consume(*id)?;
        }
    }
    for (i, id) in arguments.iter().enumerate() {
        if let Some((root, exclusive)) = builder.loans.get(id)
            && arguments[..i]
                .iter()
                .filter_map(|id| builder.loans.get(id))
                .any(|(prior, ex)| root == prior && (*exclusive || *ex))
        {
            return Err(ownership("call duplicates an exclusive loan alias"));
        }
    }
    Ok(())
}
impl Builder<'_, '_> {
    pub(super) fn peek(&self, name: &str) -> Result<Value, Failure> {
        let value = self
            .locals
            .iter()
            .rev()
            .find(|(local, _)| *local == name)
            .map(|(_, v)| v.clone())
            .ok_or_else(|| ownership("unknown exact local"))?;
        if !self.alive[value.id as usize] {
            return Err(ownership("use of moved or ended original binding"));
        }
        if self.loans.get(&value.id).is_some_and(|(_, exclusive)| *exclusive)
            && self.loan_parents.values().any(|parent| *parent == value.id)
        {
            return Err(ownership("exclusive original parent loan is frozen by active payload"));
        }
        Ok(value)
    }
    pub(super) fn reference_operand(&self, id: u32) -> Result<Value, Failure> {
        let zryna_syntax::v5::RawExpressionKind::Reference { name } =
            &self.original.body.expressions[id as usize].kind
        else {
            return Err(ownership("owned observation requires a lexical complete place"));
        };
        self.peek(&name.text)
    }
    pub(super) fn consume(&mut self, id: u32) -> Result<(), Failure> {
        if !self.alive.get(id as usize).copied().unwrap_or(false)
            || self.loans.values().any(|(root, _)| *root == id)
        {
            return Err(ownership("move/drop of unavailable or loaned original owner"));
        }
        self.alive[id as usize] = false;
        Ok(())
    }
    pub(super) fn ext(
        &mut self,
        ty: Closed,
        span: zryna_source::UntrustedSpan,
        op: Owned,
    ) -> Result<Value, Failure> {
        match &op {
            Owned::Move(id) | Owned::Drop(id) => self.consume(*id)?,
            Owned::Borrow { value, exclusive } => {
                if !self.alive[*value as usize]
                    || self.loans.values().any(|(root, ex)| *root == *value && (*ex || *exclusive))
                {
                    return Err(ownership("overlapping original exclusive/shared loans"));
                }
            }
            Owned::EndLoan(id) => {
                if self.loan_parents.values().any(|parent| parent == id) {
                    return Err(ownership("ending original loan with live payload child"));
                }
                if self.loans.remove(id).is_none() {
                    return Err(ownership("ending unknown original loan"));
                }
                self.loan_parents.remove(id);
                self.alive[*id as usize] = false;
            }
            Owned::CloneString(id) => {
                if !self.alive[*id as usize]
                    || self.loans.values().any(|(root, ex)| *root == *id && *ex)
                {
                    return Err(ownership(
                        "clone of moved or exclusively borrowed original String",
                    ));
                }
            }
            Owned::StringLiteral(_) => {}
        }
        let value = self.emit(ty, span, raw::Operation::Unit)?;
        if let Owned::Borrow { value: root, exclusive } = op {
            self.loans.insert(value.id, (root, exclusive));
        }
        self.extensions.try_reserve(1).map_err(|_| Failure::AllocationFailure)?;
        self.extensions.push(Extension { result: value.id, operation: op });
        Ok(value)
    }
    pub(super) fn close_scope(
        &mut self,
        from: usize,
        except: Option<u32>,
        span: zryna_source::UntrustedSpan,
    ) -> Result<(), Failure> {
        let end = self.next as usize;
        for id in (from..end).rev() {
            if self.loans.contains_key(&(u32::try_from(id).map_err(|_| Failure::InternalFailure)?))
                && except != Some(u32::try_from(id).map_err(|_| Failure::InternalFailure)?)
            {
                self.ext(
                    Closed::Unit,
                    span,
                    Owned::EndLoan(u32::try_from(id).map_err(|_| Failure::InternalFailure)?),
                )?;
            }
        }
        for id in (from..end).rev() {
            if self.alive[id]
                && self.affine[id]
                && except != Some(u32::try_from(id).map_err(|_| Failure::InternalFailure)?)
            {
                self.ext(
                    Closed::Unit,
                    span,
                    Owned::Drop(u32::try_from(id).map_err(|_| Failure::InternalFailure)?),
                )?;
            }
        }
        Ok(())
    }
}
