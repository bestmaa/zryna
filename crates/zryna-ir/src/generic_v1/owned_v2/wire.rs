//! Separate bounded v2 domain; embedded frozen v1 graph grants no owned authority.
use super::{Failure, raw};
use crate::generic_v1::{budget, reject, reserve};
#[cfg(test)]
mod tests;
/// Distinct owned wire domain.
pub const HEADER: &[u8] = b"ZRYNA-GENERIC-OWNED-IR-V2\0";
/// Separate schema version.
pub const VERSION: u32 = 2;
/// Complete message ceiling.
pub const MAX_BYTES: usize = 32 * 1024 * 1024;
/// Complete nested vector amplification ceiling.
pub const MAX_CHILDREN: usize = 1_048_576;
/// Bounded untrusted decoded claims, never an executable authority.
#[derive(Debug)]
pub struct DecodedProgram(pub(super) raw::Program);
impl DecodedProgram {
    /// Immutable untrusted claims for hostile independent verification.
    #[must_use]
    pub const fn claims(&self) -> &raw::Program {
        &self.0
    }
}
struct Writer(Vec<u8>);
impl Writer {
    fn bytes(&mut self, bytes: &[u8]) -> Result<(), Failure> {
        if self.0.len().checked_add(bytes.len()).is_none_or(|n| n > MAX_BYTES) {
            return Err(budget("owned wire byte ceiling"));
        }
        self.0.try_reserve(bytes.len()).map_err(|_| Failure::AllocationFailure)?;
        self.0.extend_from_slice(bytes);
        Ok(())
    }
    fn u32(&mut self, n: usize) -> Result<(), Failure> {
        self.bytes(
            &u32::try_from(n).map_err(|_| budget("owned wire count overflow"))?.to_le_bytes(),
        )
    }
    fn ids(&mut self, ids: &[u32]) -> Result<(), Failure> {
        self.u32(ids.len())?;
        for id in ids {
            self.bytes(&id.to_le_bytes())?;
        }
        Ok(())
    }
}
/// Encodes complete raw claims in the owned domain, granting no authority.
/// # Errors
/// Rejects count, byte, core-graph and total-child amplification.
pub fn encode(claim: &raw::Program) -> Result<Vec<u8>, Failure> {
    let mut w = Writer(Vec::new());
    w.bytes(HEADER)?;
    w.u32(VERSION as usize)?;
    let core = crate::generic_v1::wire::encode(&claim.graph)?;
    w.u32(core.len())?;
    w.bytes(&core)?;
    w.u32(claim.extensions.len())?;
    for extensions in &claim.extensions {
        w.u32(extensions.len())?;
        for e in extensions {
            w.bytes(&e.result.to_le_bytes())?;
            match &e.operation {
                raw::Operation::StringLiteral(bytes) => {
                    w.bytes(&[1])?;
                    w.u32(bytes.len())?;
                    w.bytes(bytes)?;
                }
                raw::Operation::Move(id) => {
                    w.bytes(&[2])?;
                    w.bytes(&id.to_le_bytes())?;
                }
                raw::Operation::Borrow { value, exclusive } => {
                    w.bytes(&[3])?;
                    w.bytes(&value.to_le_bytes())?;
                    w.bytes(&[u8::from(*exclusive)])?;
                }
                raw::Operation::EndLoan(id) => {
                    w.bytes(&[4])?;
                    w.bytes(&id.to_le_bytes())?;
                }
                raw::Operation::Drop(id) => {
                    w.bytes(&[5])?;
                    w.bytes(&id.to_le_bytes())?;
                }
                raw::Operation::CloneString(id) => {
                    w.bytes(&[6])?;
                    w.bytes(&id.to_le_bytes())?;
                }
            }
        }
    }
    w.u32(claim.plans.len())?;
    for plan in &claim.plans {
        w.u32(plan.steps.len())?;
        for s in &plan.steps {
            w.bytes(&s.block.to_le_bytes())?;
            w.bytes(&s.position.to_le_bytes())?;
            w.bytes(&[u8::from(s.failure)])?;
            w.ids(&s.end_loans)?;
            w.ids(&s.cleanup)?;
        }
    }
    // The independent decoder checks the complete nested child budget too.
    decode(&w.0)?;
    Ok(w.0)
}
struct Reader<'a> {
    bytes: &'a [u8],
    position: usize,
    children: usize,
}
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], Failure> {
        let end = self.position.checked_add(n).ok_or_else(invalid)?;
        let out = self.bytes.get(self.position..end).ok_or_else(invalid)?;
        self.position = end;
        Ok(out)
    }
    fn u32(&mut self) -> Result<u32, Failure> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().map_err(|_| invalid())?))
    }
    fn boolean(&mut self) -> Result<bool, Failure> {
        match self.take(1)?[0] {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(invalid()),
        }
    }
    fn vector<T>(
        &mut self,
        limit: usize,
        min: usize,
        mut read: impl FnMut(&mut Self) -> Result<T, Failure>,
    ) -> Result<Vec<T>, Failure> {
        let n = self.u32()? as usize;
        if n > limit || self.children.checked_add(n).is_none_or(|s| s > MAX_CHILDREN) {
            return Err(budget("owned wire vector amplification"));
        }
        if n.checked_mul(min).is_none_or(|s| s > self.bytes.len() - self.position) {
            return Err(invalid());
        }
        self.children += n;
        let mut out = reserve(n)?;
        for _ in 0..n {
            out.push(read(self)?);
        }
        Ok(out)
    }
    fn ids(&mut self) -> Result<Vec<u32>, Failure> {
        self.vector(16_384, 4, Self::u32)
    }
    fn operation(&mut self) -> Result<raw::Operation, Failure> {
        Ok(match self.take(1)?[0] {
            1 => {
                let n = self.u32()? as usize;
                if n > 65_536 {
                    return Err(budget("owned String literal ceiling"));
                }
                let bytes = self.take(n)?;
                std::str::from_utf8(bytes).map_err(|_| invalid())?;
                let mut v = reserve(n)?;
                v.extend_from_slice(bytes);
                raw::Operation::StringLiteral(v)
            }
            2 => raw::Operation::Move(self.u32()?),
            3 => raw::Operation::Borrow { value: self.u32()?, exclusive: self.boolean()? },
            4 => raw::Operation::EndLoan(self.u32()?),
            5 => raw::Operation::Drop(self.u32()?),
            6 => raw::Operation::CloneString(self.u32()?),
            _ => return Err(invalid()),
        })
    }
}
/// Decodes exact v2 tags, UTF-8, booleans and bounded complete vectors.
/// # Errors
/// Rejects old/unknown domains, truncation, invalid tags, amplification and trailing bytes.
pub fn decode(bytes: &[u8]) -> Result<DecodedProgram, Failure> {
    if bytes.len() > MAX_BYTES {
        return Err(budget("owned wire byte ceiling"));
    }
    let mut r = Reader { bytes, position: 0, children: 0 };
    if r.take(HEADER.len())? != HEADER || r.u32()? != VERSION {
        return Err(invalid());
    }
    let core_len = r.u32()? as usize;
    let graph = crate::generic_v1::wire::decode(r.take(core_len)?)?.claims().clone();
    let extensions = r.vector(65_536, 4, |r| {
        r.vector(16_384, 6, |r| Ok(raw::Extension { result: r.u32()?, operation: r.operation()? }))
    })?;
    let plans = r.vector(65_536, 4, |r| {
        Ok(raw::Plan {
            steps: r.vector(20_480, 17, |r| {
                Ok(raw::Step {
                    block: r.u32()?,
                    position: r.u32()?,
                    failure: r.boolean()?,
                    end_loans: r.ids()?,
                    cleanup: r.ids()?,
                })
            })?,
        })
    })?;
    if r.position != bytes.len() {
        return Err(invalid());
    }
    Ok(DecodedProgram(raw::Program { graph, extensions, plans }))
}
fn invalid() -> Failure {
    reject("malformed distinct owned successor v2 wire")
}
