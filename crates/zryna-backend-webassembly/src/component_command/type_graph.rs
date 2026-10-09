//! Compare the decoded public type graph with authenticated WIT, without subtyping.

use std::collections::{BTreeMap, HashMap, HashSet};

use wit_parser::{
    Function, FunctionKind, Handle, InterfaceId, Resolve, Type, TypeDefKind, TypeId, TypeOwner,
    WorldId, WorldItem, WorldKey,
};
use zryna_diagnostics::Diagnostic;

use crate::wit_world_audit::AuthenticatedCommandWorld;

const MAX_TYPES: usize = 4096;
const MAX_EDGES: usize = 16384;
const MAX_DEPTH: usize = 32;

pub(crate) fn compare(
    authority: &AuthenticatedCommandWorld,
    actual: &Resolve,
    world: WorldId,
) -> Result<(), Diagnostic> {
    if actual.types.len() > MAX_TYPES || actual.interfaces.len() > 32 {
        return Err(invalid("decoded command type graph exceeds its envelope"));
    }
    let expected = authority.resolve();
    let mut graph = Graph {
        expected,
        actual,
        visited: HashSet::new(),
        resources: HashMap::new(),
        reverse_resources: HashMap::new(),
        edges: 0,
    };
    for export in [false, true] {
        let expected_items = interfaces(expected, authority.world(), export)?;
        let actual_items = interfaces(actual, world, export)?;
        if expected_items.keys().ne(actual_items.keys()) {
            return Err(invalid(
                "command interface names or versions differ from authenticated WIT",
            ));
        }
        for (name, expected_id) in expected_items {
            graph.interface(expected_id, actual_items[&name])?;
        }
    }
    Ok(())
}

fn interfaces(
    resolve: &Resolve,
    world: WorldId,
    export: bool,
) -> Result<BTreeMap<String, InterfaceId>, Diagnostic> {
    let world = &resolve.worlds[world];
    let items = if export { &world.exports } else { &world.imports };
    let mut result = BTreeMap::new();
    for (key, item) in items {
        let WorldItem::Interface { id, .. } = item else {
            return Err(invalid("command world contains a non-interface item"));
        };
        let identity = resolve.id_of(*id).ok_or_else(|| invalid("unnamed command interface"))?;
        let key_matches = match key {
            WorldKey::Interface(key_id) => key_id == id,
            WorldKey::Name(name) => name == &identity,
        };
        if !key_matches || result.insert(identity, *id).is_some() {
            return Err(invalid("command interface identity is ambiguous"));
        }
    }
    Ok(result)
}

struct Graph<'a> {
    expected: &'a Resolve,
    actual: &'a Resolve,
    visited: HashSet<(TypeId, TypeId)>,
    resources: HashMap<TypeId, TypeId>,
    reverse_resources: HashMap<TypeId, TypeId>,
    edges: usize,
}

impl Graph<'_> {
    fn interface(&mut self, left: InterfaceId, right: InterfaceId) -> Result<(), Diagnostic> {
        let expected = self.expected;
        let actual = self.actual;
        let left = &expected.interfaces[left];
        let right = &actual.interfaces[right];
        if left.types.len() != right.types.len() || left.functions.len() != right.functions.len() {
            return Err(invalid("command interface type or function set differs"));
        }
        for (name, ty) in &left.types {
            let other = right.types.get(name).ok_or_else(|| invalid("command type is missing"))?;
            self.ty(Type::Id(*ty), Type::Id(*other), 0)?;
        }
        for (name, function) in &left.functions {
            let other =
                right.functions.get(name).ok_or_else(|| invalid("command function is missing"))?;
            self.function(function, other)?;
        }
        Ok(())
    }

    fn function(&mut self, left: &Function, right: &Function) -> Result<(), Diagnostic> {
        if left.name != right.name || left.params.len() != right.params.len() {
            return Err(invalid("command function name or arity differs"));
        }
        match (&left.kind, &right.kind) {
            (FunctionKind::Freestanding, FunctionKind::Freestanding) => {}
            (FunctionKind::Method(a), FunctionKind::Method(b))
            | (FunctionKind::Static(a), FunctionKind::Static(b))
            | (FunctionKind::Constructor(a), FunctionKind::Constructor(b)) => {
                self.ty(Type::Id(*a), Type::Id(*b), 0)?;
            }
            _ => return Err(invalid("command function kind differs or is asynchronous")),
        }
        for (left, right) in left.params.iter().zip(&right.params) {
            if left.name != right.name {
                return Err(invalid("command function parameter name differs"));
            }
            self.ty(left.ty, right.ty, 0)?;
        }
        self.optional(left.result, right.result, 0)
    }

    fn optional(
        &mut self,
        left: Option<Type>,
        right: Option<Type>,
        depth: usize,
    ) -> Result<(), Diagnostic> {
        match (left, right) {
            (None, None) => Ok(()),
            (Some(a), Some(b)) => self.ty(a, b, depth + 1),
            _ => Err(invalid("command optional type differs")),
        }
    }

    fn ty(&mut self, left: Type, right: Type, depth: usize) -> Result<(), Diagnostic> {
        self.edges += 1;
        if depth > MAX_DEPTH || self.edges > MAX_EDGES {
            return Err(invalid("command type traversal exceeds its envelope"));
        }
        let left = canonical(self.expected, left)?;
        let right = canonical(self.actual, right)?;
        match (left, right) {
            (Type::Id(a), Type::Id(b)) => {
                if !self.visited.insert((a, b)) {
                    return Ok(());
                }
                self.definition(a, b, depth + 1)
            }
            (a, b) if a == b && a != Type::ErrorContext => Ok(()),
            _ => Err(invalid("command value type differs")),
        }
    }

    fn definition(&mut self, left: TypeId, right: TypeId, depth: usize) -> Result<(), Diagnostic> {
        if depth > MAX_DEPTH {
            return Err(invalid("command type definition depth exceeds its envelope"));
        }
        let expected = self.expected;
        let actual = self.actual;
        match (&expected.types[left].kind, &actual.types[right].kind) {
            (TypeDefKind::Resource, TypeDefKind::Resource) => {
                // A bijection catches both splitting one resource and collapsing distinct ones.
                // Following aliases above means every use must reach this same identity pair.
                if resource_name(expected, left)? != resource_name(actual, right)?
                    || self.resources.get(&left).is_some_and(|id| *id != right)
                    || self.reverse_resources.get(&right).is_some_and(|id| *id != left)
                {
                    return Err(invalid("command resource identity or alias equivalence differs"));
                }
                self.resources.insert(left, right);
                self.reverse_resources.insert(right, left);
            }
            (TypeDefKind::Handle(Handle::Own(a)), TypeDefKind::Handle(Handle::Own(b)))
            | (TypeDefKind::Handle(Handle::Borrow(a)), TypeDefKind::Handle(Handle::Borrow(b))) => {
                self.ty(Type::Id(*a), Type::Id(*b), depth)?;
            }
            (TypeDefKind::Record(a), TypeDefKind::Record(b))
                if a.fields.len() == b.fields.len() =>
            {
                for (a, b) in a.fields.iter().zip(&b.fields) {
                    if a.name != b.name {
                        return Err(invalid("command record field differs"));
                    }
                    self.ty(a.ty, b.ty, depth)?;
                }
            }
            (TypeDefKind::Variant(a), TypeDefKind::Variant(b))
                if a.cases.len() == b.cases.len() =>
            {
                for (a, b) in a.cases.iter().zip(&b.cases) {
                    if a.name != b.name {
                        return Err(invalid("command variant case differs"));
                    }
                    self.optional(a.ty, b.ty, depth)?;
                }
            }
            (TypeDefKind::Flags(a), TypeDefKind::Flags(b))
                if a.flags.iter().map(|x| &x.name).eq(b.flags.iter().map(|x| &x.name)) => {}
            (TypeDefKind::Enum(a), TypeDefKind::Enum(b))
                if a.cases.iter().map(|x| &x.name).eq(b.cases.iter().map(|x| &x.name)) => {}
            (TypeDefKind::Tuple(a), TypeDefKind::Tuple(b)) if a.types.len() == b.types.len() => {
                for (a, b) in a.types.iter().zip(&b.types) {
                    self.ty(*a, *b, depth)?;
                }
            }
            (TypeDefKind::Option(a), TypeDefKind::Option(b))
            | (TypeDefKind::List(a), TypeDefKind::List(b)) => self.ty(*a, *b, depth)?,
            (TypeDefKind::Result(a), TypeDefKind::Result(b)) => {
                self.optional(a.ok, b.ok, depth)?;
                self.optional(a.err, b.err, depth)?;
            }
            _ => {
                return Err(invalid(
                    "command defined type differs or is outside the reviewed profile",
                ));
            }
        }
        Ok(())
    }
}

fn canonical(resolve: &Resolve, mut ty: Type) -> Result<Type, Diagnostic> {
    for _ in 0..=MAX_DEPTH {
        let Type::Id(id) = ty else { return Ok(ty) };
        match resolve.types[id].kind {
            TypeDefKind::Type(next) => ty = next,
            _ => return Ok(ty),
        }
    }
    Err(invalid("command type alias chain exceeds its envelope"))
}

fn resource_name(resolve: &Resolve, id: TypeId) -> Result<(String, &str), Diagnostic> {
    let definition = &resolve.types[id];
    let TypeOwner::Interface(interface) = definition.owner else {
        return Err(invalid("command resource has no canonical interface owner"));
    };
    let interface =
        resolve.id_of(interface).ok_or_else(|| invalid("command resource owner is unnamed"))?;
    let name = definition.name.as_deref().ok_or_else(|| invalid("command resource is unnamed"))?;
    Ok((interface, name))
}

fn invalid(message: &str) -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-W4012",
        None,
        message,
        "rebuild the command from its authenticated WIT, verified source and private bridge",
    )
}
