use super::{Failure, graph, raw};
use crate::generic_v1::{body, cfg, inventory::Inventory, reject, reserve};
use zryna_layout::generic_v1::VerifiedLayouts;
use zryna_source::SourceMap;

pub(super) fn check(
    claim: &raw::Program,
    sources: &SourceMap,
    layouts: &VerifiedLayouts,
    inventory: &Inventory,
) -> Result<(), Failure> {
    if claim.extensions.len() != claim.graph.functions.len() {
        return Err(reject("incomplete extension inventory"));
    }
    let views = layouts.types().collect::<Vec<_>>();
    if views.iter().any(|ty| !matches!(ty.key().first(), Some(0 | 1 | 2 | 0x14 | 0x15))) {
        return Err(reject("owned seal contains a type outside its admitted execution lane"));
    }
    let mut calls = reserve(0)?;
    for (fi, function) in claim.graph.functions.iter().enumerate() {
        if matches!(function.result, graph::Type::Borrow { .. }) {
            return Err(reject("escaping function result loan"));
        }
        let g = cfg::build(function, sources, views.len())?;
        let ext = &claim.extensions[fi];
        if ext.windows(2).any(|p| p[0].result >= p[1].result) {
            return Err(reject("duplicate or unordered owned effects"));
        }
        let mut seen = 0;
        for (bi, block) in function.blocks.iter().enumerate() {
            for (pi, ins) in block.instructions.iter().enumerate() {
                let operand = |id| g.operand(id, bi, pi);
                let extended = ext
                    .binary_search_by_key(&ins.result.id, |e| e.result)
                    .ok()
                    .map(|i| &ext[i].operation);
                let expected = if let Some(op) = extended {
                    seen += 1;
                    if ins.operation != graph::Operation::Unit {
                        return Err(reject("owned effect lacks exact placeholder"));
                    }
                    match op {
                        raw::Operation::StringLiteral(bytes) => {
                            std::str::from_utf8(bytes)
                                .map_err(|_| reject("invalid String UTF8"))?;
                            graph::Type::Stored(2)
                        }
                        raw::Operation::Move(id) => {
                            let ty = operand(*id)?;
                            if !matches!(ty, graph::Type::Stored(_)) {
                                return Err(reject("move cannot carry unit or loan"));
                            }
                            ty
                        }
                        raw::Operation::Borrow { value, exclusive } => {
                            let graph::Type::Stored(referent) = operand(*value)? else {
                                return Err(reject("loan requires stored root"));
                            };
                            graph::Type::Borrow { referent, exclusive: *exclusive }
                        }
                        raw::Operation::EndLoan(id) => {
                            if !matches!(operand(*id)?, graph::Type::Borrow { .. }) {
                                return Err(reject("end of nonloan"));
                            }
                            graph::Type::Unit
                        }
                        raw::Operation::Drop(id) => {
                            let graph::Type::Stored(i) = operand(*id)? else {
                                return Err(reject("drop requires complete owned value"));
                            };
                            if views[i as usize].drop_kind() == 0 {
                                return Err(reject("drop claim for Copy value"));
                            }
                            graph::Type::Unit
                        }
                        raw::Operation::CloneString(id) => {
                            if operand(*id)? != graph::Type::Stored(2) {
                                return Err(reject("clone requires exact String"));
                            }
                            graph::Type::Stored(2)
                        }
                    }
                } else {
                    body::operation(
                        &claim.graph,
                        &views,
                        inventory.generic_count,
                        &ins.operation,
                        &operand,
                    )?
                };
                if expected != ins.result.ty {
                    return Err(reject("owned operation result type mismatch"));
                }
                if let Some(target) =
                    body::call_target(&claim.graph, inventory.generic_count, &ins.operation)?
                {
                    calls.push((inventory.declarations[fi], inventory.declarations[target]));
                }
            }
            body::terminator(&views, function, &block.terminator, &|id| {
                g.operand(id, bi, block.instructions.len())
            })?;
        }
        if seen != ext.len() {
            return Err(reject("orphan owned effect claim"));
        }
    }
    crate::generic_v1::calls::check(claim.graph.declarations.len(), &calls)
}
