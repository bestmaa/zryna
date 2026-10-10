use crate::wit_world_audit::AuthenticatedCommandWorld;
use wasm_encoder::{
    ComponentBuilder,
    reencode::{ReencodeComponent, RoundtripReencoder},
};
use wasmparser::{ComponentType, ComponentTypeDeclaration, Parser, Payload};
use zryna_diagnostics::Diagnostic;
pub(crate) const RUN_INTERFACE: &str = "wasi:cli/run@0.2.12";
pub(crate) const ENVIRONMENT_INTERFACE: &str = "wasi:cli/environment@0.2.12";

pub(crate) fn import_world(
    component: &mut ComponentBuilder,
    world: &AuthenticatedCommandWorld,
) -> Result<(u32, u32), Diagnostic> {
    let metadata = wit_component::metadata::encode(
        world.resolve(),
        world.world(),
        wit_component::StringEncoding::UTF8,
        None,
    )
    .map_err(|_| invalid("authenticated command world type could not be encoded"))?;
    import_declarations(component, world_declarations(&metadata)?)
}
fn world_declarations(bytes: &[u8]) -> Result<Box<[ComponentTypeDeclaration<'_>]>, Diagnostic> {
    let mut world = None;
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::ComponentTypeSection(types) =
            payload.map_err(|_| invalid("encoded world metadata is malformed"))?
        {
            for ty in types {
                let ComponentType::Component(wrapper) =
                    ty.map_err(|_| invalid("encoded world wrapper is malformed"))?
                else {
                    return Err(invalid("encoded world wrapper is not a component type"));
                };
                let mut wrapper = Vec::from(wrapper).into_iter();
                let Some(ComponentTypeDeclaration::Type(ComponentType::Component(declarations))) =
                    wrapper.next()
                else {
                    return Err(invalid("encoded world wrapper has an unexpected body"));
                };
                match wrapper.next() {
                    Some(ComponentTypeDeclaration::Export { name, ty })
                        if name.name == "zryna:capability-profiles/command@0.1.0"
                            && ty == wasmparser::ComponentTypeRef::Component(0) => {}
                    _ => return Err(invalid("encoded world wrapper has a different identity")),
                }
                if wrapper.next().is_some() || world.replace(declarations).is_some() {
                    return Err(invalid("encoded world metadata contains extra declarations"));
                }
            }
        }
    }
    world.ok_or_else(|| invalid("encoded command world type is missing"))
}

fn import_declarations(
    component: &mut ComponentBuilder,
    declarations: Box<[ComponentTypeDeclaration<'_>]>,
) -> Result<(u32, u32), Diagnostic> {
    let mut reencoder = RoundtripReencoder;
    let mut run_type = None;
    let mut environment = None;
    for declaration in Vec::from(declarations) {
        match declaration {
            ComponentTypeDeclaration::Type(ty) => {
                let (_, encoder) = component.ty(None);
                reencoder
                    .parse_component_type(encoder, ty)
                    .map_err(|_| invalid("command world type reencoding failed"))?;
            }
            ComponentTypeDeclaration::Alias(alias) => {
                let alias = reencoder
                    .component_alias(alias)
                    .map_err(|_| invalid("command world type alias reencoding failed"))?;
                component.alias(None, alias);
            }
            ComponentTypeDeclaration::Import(import) => {
                let ty = reencoder
                    .component_type_ref(import.ty)
                    .map_err(|_| invalid("command world import type reencoding failed"))?;
                let index = component.import(import.name, ty);
                if import.name.name == ENVIRONMENT_INTERFACE && environment.replace(index).is_some()
                {
                    return Err(invalid("command environment interface is duplicated"));
                }
            }
            ComponentTypeDeclaration::Export { name, ty } => match ty {
                wasmparser::ComponentTypeRef::Instance(index)
                    if name.name == RUN_INTERFACE && run_type.replace(index).is_none() => {}
                _ => return Err(invalid("command world has an unexpected export")),
            },
            ComponentTypeDeclaration::CoreType(_) => {
                return Err(invalid("command world unexpectedly declares a core type"));
            }
        }
    }
    Ok((
        run_type.ok_or_else(|| invalid("command run interface is missing"))?,
        environment.ok_or_else(|| invalid("command environment interface is missing"))?,
    ))
}

fn invalid(message: &str) -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-W4011",
        None,
        message,
        "restore the authenticated command world and bounded private bridge",
    )
}
