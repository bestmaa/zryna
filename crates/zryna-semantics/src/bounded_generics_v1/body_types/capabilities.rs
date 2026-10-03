use zryna_syntax::v5::RawDataDeclarationKind;

use super::super::{DeclarationContext, DeclarationIdentity};
use super::model::{CapabilityStatuses as Statuses, Head, Kind, Origin, Scalar, Tables, Ty};
use super::{BodyTypeFailure, resources, signatures, substitution};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Status {
    True,
    Unknown,
    False,
}

impl Status {
    fn and(self, other: Self) -> Self {
        match (self, other) {
            (Self::False, _) | (_, Self::False) => Self::False,
            (Self::Unknown, _) | (_, Self::Unknown) => Self::Unknown,
            _ => Self::True,
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Rule {
    Source(Option<Head>),
    Nominal,
}

#[derive(Debug)]
struct Row {
    rule: Rule,
    values: Statuses,
    first_reverse: Option<usize>,
    support: Option<usize>,
}

#[derive(Debug)]
struct Supports {
    false_count: [u32; 32],
    unknown_count: [u32; 32],
}

#[derive(Clone, Copy, Debug)]
struct Edge {
    dependent: usize,
    next: Option<usize>,
    /// Only original nominal payload edges participate in incremental support counts.
    payload: bool,
}

/// Source predicates over four capability bits (two original parameters). Clone starts at
/// true and decreases: indirect recursive payloads retain the inherited greatest fixed point.
#[derive(Debug)]
pub(super) struct Graph {
    rows: Vec<Row>,
    offsets: Vec<usize>,
    nominals: Vec<(DeclarationIdentity, usize)>,
    supports: Vec<Supports>,
    edges: Vec<Edge>,
}

impl Graph {
    pub(super) fn storage(&self) -> (usize, usize, usize) {
        use std::mem::size_of;
        let bytes = self.rows.capacity() * size_of::<Row>()
            + self.offsets.capacity() * size_of::<usize>()
            + self.nominals.capacity() * size_of::<(DeclarationIdentity, usize)>()
            + self.supports.capacity() * size_of::<Supports>()
            + self.edges.capacity() * size_of::<Edge>();
        (self.rows.len(), self.edges.len(), bytes)
    }

    pub(super) const fn empty() -> Self {
        Self {
            rows: Vec::new(),
            offsets: Vec::new(),
            nominals: Vec::new(),
            supports: Vec::new(),
            edges: Vec::new(),
        }
    }

    fn source(&self, module: u32, occurrence: u32) -> usize {
        self.offsets[module as usize] + occurrence as usize
    }

    fn nominal(&self, owner: DeclarationIdentity) -> usize {
        let index = self
            .nominals
            .binary_search_by_key(&owner, |(owner, _)| *owner)
            .expect("original nominal predicate");
        self.nominals[index].1
    }

    fn edge(&mut self, child: usize, dependent: usize, payload: bool) {
        let next = self.rows[child].first_reverse;
        self.rows[child].first_reverse = Some(self.edges.len());
        self.edges.push(Edge { dependent, next, payload });
    }

    pub(super) fn derive(
        context: &DeclarationContext<'_>,
        tables: &Tables,
    ) -> Result<Self, BodyTypeFailure> {
        let types = tables
            .sources
            .iter()
            .try_fold(0, |sum, nodes| resources::checked_add(sum, nodes.len()))?;
        let data =
            context.syntax().files().iter().try_fold(0, |sum, unit| {
                resources::checked_add(sum, unit.data_declarations.len())
            })?;
        let members = context
            .syntax()
            .files()
            .iter()
            .flat_map(|unit| &unit.data_declarations)
            .try_fold(0, |sum, declaration| {
                resources::checked_add(
                    sum,
                    match &declaration.kind {
                        RawDataDeclarationKind::Struct { fields, .. } => fields.len(),
                        RawDataDeclarationKind::Enum { variants, .. } => {
                            variants.iter().filter(|variant| variant.payload_type.is_some()).count()
                        }
                    },
                )
            })?;
        let source_edges = tables.sources.iter().flatten().try_fold(0, |sum, node| {
            let count = node.head.map_or(0, |head| {
                head.children.iter().flatten().count()
                    + usize::from(matches!(head.kind, Kind::Nominal(_)))
            });
            resources::checked_add(sum, count)
        })?;
        let mut graph = Self {
            rows: resources::reserve(resources::checked_add(types, data)?)?,
            offsets: resources::reserve(tables.sources.len())?,
            nominals: resources::reserve(data)?,
            supports: resources::reserve(data)?,
            edges: resources::reserve(resources::checked_add(source_edges, members)?)?,
        };
        for nodes in &tables.sources {
            graph.offsets.push(graph.rows.len());
            for node in nodes {
                graph.rows.push(Row {
                    rule: Rule::Source(node.head),
                    values: Statuses::TRUE,
                    first_reverse: None,
                    support: None,
                });
            }
        }
        for module in context.modules() {
            for declaration in module.data_declarations() {
                let row = graph.rows.len();
                graph.nominals.push((declaration.identity(), row));
                graph.rows.push(Row {
                    rule: Rule::Nominal,
                    values: Statuses::TRUE,
                    first_reverse: None,
                    support: Some(graph.supports.len()),
                });
                graph.supports.push(Supports { false_count: [0; 32], unknown_count: [0; 32] });
                match signatures::data(context, declaration.identity()) {
                    RawDataDeclarationKind::Struct { fields, .. } => {
                        for field in fields {
                            graph.edge(
                                graph.source(module.identity().index(), field.type_syntax),
                                row,
                                true,
                            );
                        }
                    }
                    RawDataDeclarationKind::Enum { variants, .. } => {
                        for variant in variants {
                            if let Some(payload) = variant.payload_type {
                                graph.edge(
                                    graph.source(module.identity().index(), payload),
                                    row,
                                    true,
                                );
                            }
                        }
                    }
                }
            }
        }
        graph.nominals.sort_unstable_by_key(|(owner, _)| *owner);
        for row in 0..types {
            let Rule::Source(Some(head)) = graph.rows[row].rule else { continue };
            for ty in head.children.into_iter().flatten() {
                let Origin::Source { module, occurrence } = ty.origin else {
                    return Err(BodyTypeFailure::InternalFailure);
                };
                graph.edge(graph.source(module, occurrence), row, false);
            }
            if let Kind::Nominal(owner) = head.kind {
                graph.edge(graph.nominal(owner), row, false);
            }
        }
        graph.solve()
    }

    fn solve(mut self) -> Result<Self, BodyTypeFailure> {
        let count = self.rows.len();
        if count == 0 {
            return Ok(self);
        }
        let mut queue = resources::repeated(count, 0_usize)?;
        let mut pending = resources::repeated(count, true)?;
        for (row, slot) in queue.iter_mut().enumerate() {
            *slot = row;
        }
        let (mut start, mut length) = (0, count);
        while length > 0 {
            let row = queue[start];
            start = (start + 1) % count;
            length -= 1;
            pending[row] = false;
            let old = self.rows[row].values;
            let next = Statuses::from(self.evaluate(row)?);
            if old == next {
                continue;
            }
            self.rows[row].values = next;
            let mut edge = self.rows[row].first_reverse;
            while let Some(index) = edge {
                let current = self.edges[index];
                if current.payload {
                    let support = self.rows[current.dependent]
                        .support
                        .ok_or(BodyTypeFailure::InternalFailure)?;
                    for output in 0..32 {
                        let supports = &mut self.supports[support];
                        if old.get(output) == Status::False {
                            supports.false_count[output] -= 1;
                        }
                        if old.get(output) == Status::Unknown {
                            supports.unknown_count[output] -= 1;
                        }
                        if next.get(output) == Status::False {
                            supports.false_count[output] += 1;
                        }
                        if next.get(output) == Status::Unknown {
                            supports.unknown_count[output] += 1;
                        }
                    }
                }
                if !pending[current.dependent] {
                    queue[(start + length) % count] = current.dependent;
                    length += 1;
                    pending[current.dependent] = true;
                }
                edge = current.next;
            }
        }
        Ok(self)
    }

    fn evaluate(&self, row: usize) -> Result<[Status; 32], BodyTypeFailure> {
        let mut values = [Status::True; 32];
        for (output, result) in values.iter_mut().enumerate() {
            let valuation = output / 2;
            let clone = output % 2 == 1;
            *result = match self.rows[row].rule {
                Rule::Nominal => {
                    let support = &self.supports
                        [self.rows[row].support.ok_or(BodyTypeFailure::InternalFailure)?];
                    if support.false_count[output] > 0 {
                        Status::False
                    } else if support.unknown_count[output] > 0 {
                        Status::Unknown
                    } else {
                        Status::True
                    }
                }
                Rule::Source(None) => Status::Unknown,
                Rule::Source(Some(head)) => self.source_head(head, valuation, clone)?,
            };
        }
        Ok(values)
    }

    fn source_head(
        &self,
        head: Head,
        valuation: usize,
        clone: bool,
    ) -> Result<Status, BodyTypeFailure> {
        let child = |ty: Ty, clone: bool| -> Result<Status, BodyTypeFailure> {
            let Origin::Source { module, occurrence } = ty.origin else {
                return Err(BodyTypeFailure::InternalFailure);
            };
            Ok(self.rows[self.source(module, occurrence)]
                .values
                .get(valuation * 2 + usize::from(clone)))
        };
        Ok(match head.kind {
            Kind::Parameter(parameter) => {
                let bit = parameter.index() as usize * 2 + usize::from(clone);
                if valuation & (1 << bit) != 0 { Status::True } else { Status::False }
            }
            Kind::Scalar(Scalar::String) | Kind::Shared | Kind::Weak => {
                if clone {
                    Status::True
                } else {
                    Status::False
                }
            }
            Kind::Scalar(_) => Status::True,
            Kind::Borrow | Kind::BorrowMut | Kind::Function(_) => Status::False,
            Kind::Vec if !clone => Status::False,
            Kind::Nominal(owner) => {
                let mut bits = [Status::False; 4];
                for (index, argument) in head.children.into_iter().enumerate() {
                    if let Some(argument) = argument {
                        bits[index * 2] = child(argument, false)?;
                        bits[index * 2 + 1] = child(argument, true)?;
                    }
                }
                self.apply(owner, bits, clone)
            }
            Kind::Vec | Kind::FixedArray(_) | Kind::Option | Kind::Result => {
                let mut result = Status::True;
                for argument in head.children.into_iter().flatten() {
                    result = result.and(child(argument, clone)?);
                }
                result
            }
        })
    }

    fn apply(&self, owner: DeclarationIdentity, bits: [Status; 4], clone: bool) -> Status {
        let row = self.nominal(owner);
        self.refine(row, bits, clone)
    }

    fn refine(&self, row: usize, bits: [Status; 4], clone: bool) -> Status {
        let mut first = None;
        for valuation in 0..16 {
            if bits.iter().enumerate().any(|(bit, status)| match status {
                Status::True => valuation & (1 << bit) == 0,
                Status::False => valuation & (1 << bit) != 0,
                Status::Unknown => false,
            }) {
                continue;
            }
            let value = self.rows[row].values.get(valuation * 2 + usize::from(clone));
            match first {
                None => first = Some(value),
                Some(previous) if previous != value => return Status::Unknown,
                _ => {}
            }
        }
        first.unwrap_or(Status::Unknown)
    }

    pub(super) fn query(
        &self,
        tables: &Tables,
        function: DeclarationIdentity,
        ty: Ty,
        clone: bool,
        assume_parameters: bool,
    ) -> Result<Status, BodyTypeFailure> {
        // Body descriptors are a source/substitution DAG. Use explicit evaluation frames; no
        // nominal field unfolding or call-body recursion is performed by this query.
        enum Work {
            Type(Ty, bool),
            Finish(Head, bool),
        }
        let mut work = resources::reserve(129)?;
        let mut results = resources::reserve(129)?;
        work.push(Work::Type(ty, clone));
        while let Some(item) = work.pop() {
            match item {
                Work::Type(ty, clone) => {
                    let Some(ty) = substitution::normalize(tables, function, ty)? else {
                        results.try_reserve(1).map_err(|_| BodyTypeFailure::AllocationFailure)?;
                        results.push(Status::Unknown);
                        continue;
                    };
                    if let Origin::Source { module, occurrence } = ty.origin {
                        let row = self.source(module, occurrence);
                        let value = if ty.environment == 0 {
                            self.rows[row]
                                .values
                                .get(usize::from(assume_parameters) * 30 + usize::from(clone))
                        } else {
                            let environment = &tables.function(function).environments
                                [ty.environment as usize - 1];
                            if tables.sources[module as usize][occurrence as usize].owner
                                != environment.owner
                            {
                                return Err(BodyTypeFailure::InternalFailure);
                            }
                            self.refine(
                                row,
                                environment.capabilities[usize::from(assume_parameters)],
                                clone,
                            )
                        };
                        results.try_reserve(1).map_err(|_| BodyTypeFailure::AllocationFailure)?;
                        results.push(value);
                        continue;
                    }
                    let Some(head) = substitution::head(tables, function, ty)? else {
                        results.try_reserve(1).map_err(|_| BodyTypeFailure::AllocationFailure)?;
                        results.push(Status::Unknown);
                        continue;
                    };
                    let immediate = match head.kind {
                        Kind::Parameter(_) => {
                            Some(if assume_parameters { Status::True } else { Status::False })
                        }
                        Kind::Borrow | Kind::BorrowMut | Kind::Function(_) => Some(Status::False),
                        Kind::Scalar(Scalar::String) | Kind::Shared | Kind::Weak => {
                            Some(if clone { Status::True } else { Status::False })
                        }
                        Kind::Scalar(_) => Some(Status::True),
                        Kind::Vec if !clone => Some(Status::False),
                        _ => None,
                    };
                    if let Some(value) = immediate {
                        results.try_reserve(1).map_err(|_| BodyTypeFailure::AllocationFailure)?;
                        results.push(value);
                        continue;
                    }
                    work.try_reserve(5).map_err(|_| BodyTypeFailure::AllocationFailure)?;
                    work.push(Work::Finish(head, clone));
                    for child in head.children.into_iter().rev().flatten() {
                        if matches!(head.kind, Kind::Nominal(_)) {
                            work.push(Work::Type(child, true));
                            work.push(Work::Type(child, false));
                        } else {
                            work.push(Work::Type(child, clone));
                        }
                    }
                }
                Work::Finish(head, clone) => {
                    let mut result = Status::True;
                    if let Kind::Nominal(owner) = head.kind {
                        let mut bits = [Status::False; 4];
                        for index in (0..2).rev() {
                            if head.children[index].is_some() {
                                bits[index * 2 + 1] =
                                    results.pop().ok_or(BodyTypeFailure::InternalFailure)?;
                                bits[index * 2] =
                                    results.pop().ok_or(BodyTypeFailure::InternalFailure)?;
                            }
                        }
                        result = self.apply(owner, bits, clone);
                    } else {
                        for _ in head.children.into_iter().flatten() {
                            result =
                                result.and(results.pop().ok_or(BodyTypeFailure::InternalFailure)?);
                        }
                    }
                    results.try_reserve(1).map_err(|_| BodyTypeFailure::AllocationFailure)?;
                    results.push(result);
                }
            }
        }
        results.pop().ok_or(BodyTypeFailure::InternalFailure)
    }
}
