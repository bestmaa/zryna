use super::Probe;
use wasm_encoder::{
    Alias, ComponentBuilder, ComponentExportKind, ComponentOuterAliasKind, ComponentTypeRef,
    ComponentValType, InstanceType, PrimitiveValType, TypeBounds,
};

pub(super) fn import(component: &mut ComponentBuilder, probe: Probe) -> u32 {
    let mut interface = InstanceType::new();
    let (parameter, result) = match probe {
        Probe::Environment => {
            interface
                .ty()
                .defined_type()
                .tuple([ComponentValType::Primitive(PrimitiveValType::String); 2]);
            interface.ty().defined_type().list(ComponentValType::Type(0));
            (None, Some(ComponentValType::Type(1)))
        }
        Probe::Filesystem => {
            resource(component, &mut interface, "wasi:filesystem/types@0.2.12", "descriptor");
            interface.ty().defined_type().tuple([
                ComponentValType::Type(2),
                ComponentValType::Primitive(PrimitiveValType::String),
            ]);
            interface.ty().defined_type().list(ComponentValType::Type(3));
            (None, Some(ComponentValType::Type(4)))
        }
        Probe::Network => {
            resource(component, &mut interface, "wasi:sockets/network@0.2.12", "network");
            (None, Some(ComponentValType::Type(2)))
        }
        Probe::Clock | Probe::Random => {
            (None, Some(ComponentValType::Primitive(PrimitiveValType::U64)))
        }
        Probe::Process => (Some(ComponentValType::Primitive(PrimitiveValType::U8)), None),
    };
    let index = interface.type_count();
    let mut function = interface.ty().function();
    function.params(parameter.map(|ty| ("status-code", ty)));
    function.result(result);
    interface.export(probe.operation(), ComponentTypeRef::Func(index));
    let ty = component.type_instance(None, &interface);
    let instance = component.import(probe.interface(), ComponentTypeRef::Instance(ty));
    component.alias_export(instance, probe.operation(), ComponentExportKind::Func)
}

fn resource(
    component: &mut ComponentBuilder,
    interface: &mut InstanceType,
    name: &str,
    resource: &str,
) {
    let mut types = InstanceType::new();
    types.export(resource, ComponentTypeRef::Type(TypeBounds::SubResource));
    let ty = component.type_instance(None, &types);
    let instance = component.import(name, ComponentTypeRef::Instance(ty));
    let descriptor = component.alias_export(instance, resource, ComponentExportKind::Type);
    interface.alias(Alias::Outer {
        kind: ComponentOuterAliasKind::Type,
        count: 1,
        index: descriptor,
    });
    interface.export(resource, ComponentTypeRef::Type(TypeBounds::Eq(0)));
    interface.ty().defined_type().own(1);
}
