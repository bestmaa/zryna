use super::BodyTypeContext;
use super::{
    DeclarationIdentity, InstanceContext, InstantiationFailure, TypeShape, budget, keys, push,
    reserve,
};
use super::{MAX_DATA_INSTANCES, MAX_EDGES, MAX_FUNCTION_INSTANCES};
use zryna_source::UntrustedSpan;

#[derive(Clone, Debug)]
pub(super) struct Node {
    pub(super) key: Vec<u8>,
    pub(super) shape: TypeShape,
    pub(super) arguments: [Option<usize>; 2],
    pub(super) depth: u32,
    pub(super) processed: bool,
    pub(super) members: Vec<usize>,
    pub(super) value: bool,
}
#[derive(Debug)]
pub(super) struct Function {
    pub(super) key: Vec<u8>,
    pub(super) owner: DeclarationIdentity,
    pub(super) arguments: [Option<usize>; 2],
    pub(super) generic: bool,
    pub(super) processed: bool,
}

#[derive(Clone, Copy)]
pub(super) struct Environment {
    pub(super) owner: DeclarationIdentity,
    pub(super) arguments: [Option<usize>; 2],
}

pub(super) struct Builder<'b, 'c, 's> {
    pub(super) bodies: &'b BodyTypeContext<'c, 's>,
    pub(super) types: Vec<Node>,
    pub(super) type_order: Vec<usize>,
    pub(super) functions: Vec<Function>,
    pub(super) function_order: Vec<usize>,
    pub(super) edges: Vec<(Vec<u8>, Vec<u8>)>,
    pub(super) generated: Vec<(usize, usize)>,
    pub(super) generated_reverse: Vec<Vec<usize>>,
    pub(super) argument_uses: Vec<(usize, Option<UntrustedSpan>)>,
    pub(super) bounds: super::value_bounds::Bounds,
    generic_data: usize,
    generic_functions: usize,
}

impl<'b, 'c, 's> Builder<'b, 'c, 's> {
    pub(super) fn new(bodies: &'b BodyTypeContext<'c, 's>) -> Result<Self, InstantiationFailure> {
        let mut builder = Self {
            bodies,
            types: reserve(3)?,
            type_order: reserve(3)?,
            functions: Vec::new(),
            function_order: Vec::new(),
            edges: Vec::new(),
            generated: Vec::new(),
            generated_reverse: Vec::new(),
            argument_uses: Vec::new(),
            bounds: super::value_bounds::Bounds::new(bodies)?,
            generic_data: 0,
            generic_functions: 0,
        };
        for (tag, shape) in [(0, TypeShape::Bool), (1, TypeShape::I32), (2, TypeShape::String)] {
            builder.intern(
                Node {
                    key: vec![tag],
                    shape,
                    arguments: [None; 2],
                    depth: 0,
                    processed: true,
                    members: Vec::new(),
                    value: true,
                },
                None,
            )?;
        }
        Ok(builder)
    }

    pub(super) fn generic(&self, shape: TypeShape) -> bool {
        match shape {
            TypeShape::Option | TypeShape::Result => true,
            TypeShape::Nominal(owner) => self
                .bodies
                .declarations()
                .declaration(owner)
                .is_some_and(|declaration| declaration.type_parameters().next().is_some()),
            _ => false,
        }
    }

    pub(super) fn intern(
        &mut self,
        node: Node,
        at: Option<UntrustedSpan>,
    ) -> Result<usize, InstantiationFailure> {
        let index = self.type_order.binary_search_by(|id| self.types[*id].key.cmp(&node.key));
        let Err(position) = index else {
            return Ok(self.type_order[index.expect("found type")]);
        };
        let generic = self.generic(node.shape);
        if generic && self.generic_data == MAX_DATA_INSTANCES {
            return Err(budget(
                self.bodies,
                "closed generic data instances",
                MAX_DATA_INSTANCES,
                self.generic_data + 1,
                at,
            ));
        }
        if self.types.len() == zryna_layout::MAX_TYPE_NODES {
            return Err(budget(
                self.bodies,
                "closed types",
                zryna_layout::MAX_TYPE_NODES,
                self.types.len() + 1,
                at,
            ));
        }
        self.type_order.try_reserve(1).map_err(|_| InstantiationFailure::AllocationFailure)?;
        self.generated_reverse
            .try_reserve(1)
            .map_err(|_| InstantiationFailure::AllocationFailure)?;
        let id = self.types.len();
        push(&mut self.types, node)?;
        self.generated_reverse.push(Vec::new());
        self.type_order.insert(position, id);
        self.generic_data += usize::from(generic);
        Ok(id)
    }

    pub(super) fn function(
        &mut self,
        owner: DeclarationIdentity,
        arguments: [Option<usize>; 2],
        at: Option<UntrustedSpan>,
    ) -> Result<usize, InstantiationFailure> {
        let children =
            arguments.iter().flatten().map(|id| self.types[*id].key.as_slice()).collect::<Vec<_>>();
        let generic = !children.is_empty();
        let mut lanes = vec![owner.module().index(), owner.source_index()];
        if generic {
            lanes.push(
                u32::try_from(children.len()).map_err(|_| InstantiationFailure::InternalFailure)?,
            );
        }
        let (key, key_bytes) = keys::encode(if generic { 0x40 } else { 0x41 }, &lanes, &children)?;
        if key.is_empty() {
            return Err(budget(
                self.bodies,
                "instance key bytes",
                super::MAX_KEY_BYTES,
                key_bytes,
                at,
            ));
        }
        let index = self.function_order.binary_search_by(|id| self.functions[*id].key.cmp(&key));
        let Err(position) = index else {
            return Ok(self.function_order[index.expect("found function")]);
        };
        if generic && self.generic_functions == MAX_FUNCTION_INSTANCES {
            return Err(budget(
                self.bodies,
                "closed generic functions",
                MAX_FUNCTION_INSTANCES,
                self.generic_functions + 1,
                at,
            ));
        }
        self.function_order.try_reserve(1).map_err(|_| InstantiationFailure::AllocationFailure)?;
        let id = self.functions.len();
        push(&mut self.functions, Function { key, owner, arguments, generic, processed: false })?;
        self.function_order.insert(position, id);
        self.generic_functions += usize::from(generic);
        Ok(id)
    }

    pub(super) fn edge(
        &mut self,
        from: &[u8],
        to: &[u8],
        at: Option<UntrustedSpan>,
    ) -> Result<(), InstantiationFailure> {
        let search = self
            .edges
            .binary_search_by(|(a, b)| a.as_slice().cmp(from).then_with(|| b.as_slice().cmp(to)));
        if let Err(position) = search {
            if self.edges.len() == MAX_EDGES {
                return Err(budget(
                    self.bodies,
                    "instantiation edges",
                    MAX_EDGES,
                    self.edges.len() + 1,
                    at,
                ));
            }
            let mut a = reserve(from.len())?;
            a.extend_from_slice(from);
            let mut b = reserve(to.len())?;
            b.extend_from_slice(to);
            self.edges.try_reserve(1).map_err(|_| InstantiationFailure::AllocationFailure)?;
            self.edges.insert(position, (a, b));
        }
        Ok(())
    }

    pub(super) fn argument_use(
        &mut self,
        id: usize,
        at: Option<UntrustedSpan>,
    ) -> Result<(), InstantiationFailure> {
        if !self.types[id].value {
            return Err(super::failure(
                self.bodies,
                "ZRYNA-M7001",
                at,
                "closed argument contains a non-storable nominal member".into(),
            ));
        }
        let order = at.map_or((1, 0, 0, 0), |at| (0, at.file, at.start, at.end));
        let search = self.argument_uses.binary_search_by(|(other, span)| {
            span.map_or((1, 0, 0, 0), |at| (0, at.file, at.start, at.end))
                .cmp(&order)
                .then_with(|| self.types[*other].key.cmp(&self.types[id].key))
        });
        if let Err(position) = search {
            self.argument_uses
                .try_reserve(1)
                .map_err(|_| InstantiationFailure::AllocationFailure)?;
            self.argument_uses.insert(position, (id, at));
        }
        Ok(())
    }

    pub(super) fn finish(self) -> Result<InstanceContext<'b, 'c, 's>, InstantiationFailure> {
        let mut function_order = reserve(self.generic_functions)?;
        for id in self.function_order {
            if self.functions[id].generic {
                function_order.push(id);
            }
        }
        Ok(InstanceContext {
            bodies: self.bodies,
            types: self.types,
            type_order: self.type_order,
            functions: self.functions,
            function_order,
            edges: self.edges,
        })
    }

    pub(super) fn target(
        &self,
        owner: DeclarationIdentity,
        name: &str,
    ) -> Option<DeclarationIdentity> {
        let module = self.bodies.declarations().modules().nth(owner.module().index() as usize)?;
        module
            .data_declarations()
            .chain(module.functions())
            .find(|declaration| declaration.name() == name)
            .map(super::super::DeclarationView::identity)
            .or_else(|| {
                module
                    .imports()
                    .find(|import| import.local_name() == name)
                    .map(|import| import.target().identity())
            })
    }
}
