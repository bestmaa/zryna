use crate::generic_v1::{Failure, raw as graph, reject};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Loan {
    pub root: u32,
    pub exclusive: bool,
    pub parent: Option<u32>,
}
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct State {
    pub owners: BTreeSet<u32>,
    pub loans: BTreeMap<u32, Loan>,
    pub order: Vec<u32>,
}
impl State {
    pub fn read(&self, id: u32, ty: graph::Type, owned: bool) -> Result<(), Failure> {
        if owned && !self.owners.contains(&id) {
            return Err(reject("use of moved or dropped owner"));
        }
        if matches!(ty, graph::Type::Borrow { .. }) {
            let loan = self.loans.get(&id).ok_or_else(|| reject("use of ended/foreign loan"))?;
            if loan.exclusive && self.loans.values().any(|l| l.parent == Some(id)) {
                return Err(reject("exclusive parent loan is frozen by active payload loan"));
            }
        } else if self.loans.values().any(|l| l.root == id && l.exclusive) {
            return Err(reject("direct read behind exclusive loan"));
        }
        Ok(())
    }
    pub fn consume(&mut self, id: u32, owned: bool) -> Result<(), Failure> {
        if !owned {
            return Ok(());
        }
        if self.loans.values().any(|l| l.root == id) || !self.owners.remove(&id) {
            return Err(reject("move/drop of loaned or unavailable owner"));
        }
        self.order.retain(|i| *i != id);
        Ok(())
    }
    pub fn add(&mut self, id: u32, owned: bool) {
        if owned {
            self.owners.insert(id);
            self.order.push(id);
        }
    }
    pub fn end(&mut self, id: u32) -> Result<(), Failure> {
        if self.loans.values().any(|l| l.parent == Some(id)) || self.loans.remove(&id).is_none() {
            return Err(reject("missing loan or live child at loan end"));
        }
        Ok(())
    }
    pub fn loan(
        &mut self,
        id: u32,
        root: u32,
        exclusive: bool,
        parent: Option<u32>,
    ) -> Result<(), Failure> {
        if parent.is_none()
            && self.loans.values().any(|l| l.root == root && (exclusive || l.exclusive))
        {
            return Err(reject("overlapping shared/exclusive loans"));
        }
        if let Some(parent) = parent {
            let p = self.loans.get(&parent).ok_or_else(|| reject("foreign parent loan"))?;
            if p.root != root
                || p.exclusive != exclusive
                || (exclusive && self.loans.values().any(|l| l.parent == Some(parent)))
            {
                return Err(reject("inexact payload subloan"));
            }
        }
        if self.loans.insert(id, Loan { root, exclusive, parent }).is_some() {
            return Err(reject("repeated loan identity"));
        }
        Ok(())
    }
    pub fn aliases(&self, arguments: &[u32]) -> Result<(), Failure> {
        for (i, id) in arguments.iter().enumerate() {
            if let Some(l) = self.loans.get(id)
                && arguments[..i]
                    .iter()
                    .filter_map(|id| self.loans.get(id))
                    .any(|p| p.root == l.root && (p.exclusive || l.exclusive))
            {
                return Err(reject("exclusive aliases in one call"));
            }
        }
        Ok(())
    }
}
