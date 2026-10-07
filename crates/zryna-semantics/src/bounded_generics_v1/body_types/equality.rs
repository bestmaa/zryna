use super::super::DeclarationIdentity;
use super::model::{Head, Tables, Ty};
use super::{BodyTypeFailure, resources, substitution};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Pair {
    function: DeclarationIdentity,
    left: Ty,
    right: Ty,
}

#[derive(Clone, Copy, Debug)]
struct Entry {
    pair: Pair,
    equal: bool,
}

/// A bounded optimization only. Eviction changes neither equality nor source admission.
pub(super) struct Cache {
    entries: Vec<Entry>,
    cursor: usize,
}

impl Cache {
    pub(super) fn new(capacity: usize) -> Result<Self, BodyTypeFailure> {
        Ok(Self { entries: resources::reserve(capacity)?, cursor: 0 })
    }

    fn get(&self, pair: Pair) -> Option<bool> {
        self.entries.iter().find(|entry| entry.pair == pair).map(|entry| entry.equal)
    }

    fn put(&mut self, pair: Pair, equal: bool) {
        if self.entries.capacity() == 0 {
            return;
        }
        let entry = Entry { pair, equal };
        if self.entries.len() < self.entries.capacity() {
            self.entries.push(entry);
        } else {
            self.entries[self.cursor] = entry;
            self.cursor = (self.cursor + 1) % self.entries.len();
        }
    }
}

#[derive(Clone, Copy)]
struct Frame {
    pair: Pair,
    heads: Option<(Head, Head)>,
    child: usize,
    previous: Option<Pair>,
}

pub(super) fn equal(
    tables: &Tables,
    function: DeclarationIdentity,
    left: Ty,
    right: Ty,
    cache: &mut Cache,
) -> Result<bool, BodyTypeFailure> {
    let (Some(left), Some(right)) = (
        substitution::normalize(tables, function, left)?,
        substitution::normalize(tables, function, right)?,
    ) else {
        return Ok(false);
    };
    let root = Pair { function, left, right };
    let mut stack = resources::reserve(129)?;
    stack.push(Frame { pair: root, heads: None, child: 0, previous: None });
    while let Some(frame) = stack.last_mut() {
        if frame.heads.is_none() {
            if let Some(equal) = cache.get(frame.pair) {
                if !equal {
                    cache.put(root, false);
                    return Ok(false);
                }
                stack.pop();
                continue;
            }
            let (Some(left), Some(right)) = (
                substitution::head(tables, function, frame.pair.left)?,
                substitution::head(tables, function, frame.pair.right)?,
            ) else {
                // Invalid records, including an identical invalid origin, are never equal.
                cache.put(root, false);
                return Ok(false);
            };
            if left.kind != right.kind {
                cache.put(frame.pair, false);
                cache.put(root, false);
                return Ok(false);
            }
            if frame.pair.left == frame.pair.right {
                cache.put(frame.pair, true);
                stack.pop();
                continue;
            }
            frame.heads = Some((left, right));
        }
        if frame.child == 2 {
            cache.put(frame.pair, true);
            stack.pop();
            continue;
        }
        let (left, right) = frame.heads.expect("resolved exact heads");
        let child = frame.child;
        frame.child += 1;
        match (left.children[child], right.children[child]) {
            (None, None) => {}
            (Some(left), Some(right)) => {
                let (Some(left), Some(right)) = (
                    substitution::normalize(tables, function, left)?,
                    substitution::normalize(tables, function, right)?,
                ) else {
                    return Ok(false);
                };
                let pair = Pair { function, left, right };
                // Repeated normalized children need one proof in this frame, independently
                // of optimization cache capacity. A failed child exits before this reuse.
                if frame.previous == Some(pair) {
                    continue;
                }
                frame.previous = Some(pair);
                stack.try_reserve(1).map_err(|_| BodyTypeFailure::AllocationFailure)?;
                stack.push(Frame { pair, heads: None, child: 0, previous: None });
            }
            _ => {
                cache.put(root, false);
                return Ok(false);
            }
        }
    }
    Ok(true)
}
