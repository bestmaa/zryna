use sha2::Digest as _;
use std::sync::{
    OnceLock,
    atomic::{AtomicU32, Ordering},
};
use wasmtime::{
    Config, Engine, Store, StoreLimits, StoreLimitsBuilder,
    component::{Component, Linker, ResourceType, Val, types::ComponentItem},
};
use zryna_source::{SourceFileInput, SourceMap};
use zryna_syntax::{command_h1_v1, v4};

mod interface;

struct Host {
    limits: StoreLimits,
    value: Option<String>,
    deny: bool,
    calls: usize,
    denied: bool,
}

fn candidate(name: &str) -> (Vec<u8>, Vec<super::run_audit::TrapSite>) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/wasi-command-source-fixtures");
    let text = std::fs::read_to_string(root.join(format!("{name}.zry"))).expect("command source");
    let provider = std::fs::read(root.join(format!("{name}.json"))).expect("provider source");
    let sources = SourceMap::build(vec![SourceFileInput { path: "src/main.zry".into(), text }])
        .expect("source map");
    let syntax =
        v4::verify_snapshot(v4::decode_snapshot(&provider).expect("provider decode"), &sources)
            .expect("source-bound snapshot");
    let source = command_h1_v1::admit(&syntax, &sources).expect("whole-source admission");
    let program =
        zryna_semantics::command_h1_v1::lower(&source, &sources).expect("verified command body");
    let artifact = super::artifact::Artifact::emit(
        program.verified_ir(),
        program.runtime_abi(),
        &sources,
        &crate::pinned_wit_sources(),
    )
    .expect("complete immutable command artifact");
    let sites = artifact.traps().to_vec();
    assert_eq!(sites.len(), 5);
    assert_eq!(artifact.issuer(), program.verified_ir().identity());
    assert_eq!(artifact.program().identity(), artifact.issuer());
    assert_eq!(
        artifact.source_digest(),
        &<[u8; 32]>::from(sha2::Sha256::digest(
            sources.source(source.syntax().files()[0].id()).expect("source").text().as_bytes()
        ))
    );
    assert_eq!(
        artifact.language_digest(),
        &<[u8; 32]>::from(sha2::Sha256::digest(artifact.language()))
    );
    assert_eq!(
        artifact.storage_digest(),
        &<[u8; 32]>::from(sha2::Sha256::digest(artifact.storage()))
    );
    assert_eq!(
        artifact.component_digest(),
        &<[u8; 32]>::from(sha2::Sha256::digest(artifact.bytes()))
    );
    assert_eq!(artifact.world().packages().len(), 8);
    let bytes = artifact.bytes().to_vec();
    wasmparser::Validator::new_with_features(
        wasmparser::WasmFeatures::WASM1.union(wasmparser::WasmFeatures::COMPONENT_MODEL),
    )
    .validate_all(&bytes)
    .expect("complete binary and component type validation");
    (bytes, sites)
}

fn linker(engine: &Engine, component: &Component) -> Linker<Host> {
    static NEXT_RESOURCE: AtomicU32 = AtomicU32::new(1);
    let mut linker: Linker<Host> = Linker::new(engine);
    let mut resources = Vec::new();
    for (name, import) in component.component_type().imports(engine) {
        let ComponentItem::ComponentInstance(interface) = import.ty else {
            panic!("closed interface import");
        };
        let mut instance = linker.instance(name).expect("exact imported interface");
        for (operation, export) in interface.exports(engine) {
            match export.ty {
                ComponentItem::ComponentFunc(_) => {
                    let environment =
                        name == "wasi:cli/environment@0.2.12" && operation == "get-environment";
                    instance
                        .func_new(operation, move |mut store, _, parameters, results| {
                            if environment && !store.data().deny {
                                assert!(parameters.is_empty());
                                assert_eq!(results.len(), 1);
                                store.data_mut().calls += 1;
                                let values =
                                    store.data().value.as_ref().map_or_else(Vec::new, |value| {
                                        vec![Val::Tuple(vec![
                                            Val::String("MODE".into()),
                                            Val::String(value.clone()),
                                        ])]
                                    });
                                results[0] = Val::List(values);
                                return Ok(());
                            }
                            store.data_mut().denied = true;
                            Err(wasmtime::format_err!("command host capability denied"))
                        })
                        .expect("closed dynamic host function");
                }
                ComponentItem::Resource(guest_type) => {
                    let host_type = if let Some((_, host_type)) =
                        resources.iter().find(|(ty, _)| *ty == guest_type)
                    {
                        *host_type
                    } else {
                        let id = NEXT_RESOURCE
                            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| {
                                id.checked_add(1)
                            })
                            .expect("finite resource identities");
                        let host_type = ResourceType::host_dynamic(id);
                        resources.push((guest_type, host_type));
                        host_type
                    };
                    instance
                        .resource(operation, host_type, |mut store, _| {
                            store.data_mut().denied = true;
                            Err(wasmtime::format_err!("command resource capability denied"))
                        })
                        .expect("closed distinct resource binding");
                }
                ComponentItem::Type(_) => {}
                _ => panic!("pinned world contains only closed interface items"),
            }
        }
    }
    linker
}

fn run(
    name: &str,
    value: Option<String>,
    deny: bool,
) -> (wasmtime::Result<Result<(), ()>>, usize, bool) {
    let (result, calls, denied, _) = run_audited(name, value, deny);
    (result, calls, denied)
}

fn run_audited(
    name: &str,
    value: Option<String>,
    deny: bool,
) -> (
    wasmtime::Result<Result<(), ()>>,
    usize,
    bool,
    Option<zryna_ir::data_ownership_v1::VerifiedTrapIdentity>,
) {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    let engine = ENGINE.get_or_init(|| {
        let mut config = Config::new();
        config.consume_fuel(true).max_wasm_stack(65_536).generate_address_map(true);
        Engine::new(&config).expect("exact pinned component engine")
    });
    let (bytes, sites) = candidate(name);
    let component = Component::new(engine, bytes).expect("engine accepts complete command");
    let imports = linker(engine, &component);
    let limits = StoreLimitsBuilder::new()
        .memory_size(16_777_216)
        .instances(2)
        .memories(1)
        .tables(0)
        .build();
    let mut store = Store::new(engine, Host { limits, value, deny, calls: 0, denied: false });
    store.limiter(|host| &mut host.limits);
    store.set_fuel(100_000).expect("candidate command fuel limit");
    let instance =
        imports.instantiate(&mut store, &component).expect("two real cores and one shared memory");
    let interface = instance
        .get_export_index(&mut store, None, "wasi:cli/run@0.2.12")
        .expect("sole run interface");
    let export =
        instance.get_export_index(&mut store, Some(&interface), "run").expect("sole run function");
    let run = instance
        .get_typed_func::<(), (Result<(), ()>,)>(&mut store, &export)
        .expect("pinned run result type");
    let result = run.call(&mut store, ()).map(|(result,)| result);
    let calls = store.data().calls;
    let denied = store.data().denied;
    let controlled = if denied {
        None
    } else {
        result.as_ref().err().and_then(|error| controlled_identity(error, &component, &sites))
    };
    drop(store);
    (result, calls, denied, controlled)
}

fn controlled_identity(
    error: &wasmtime::Error,
    component: &Component,
    sites: &[super::run_audit::TrapSite],
) -> Option<zryna_ir::data_ownership_v1::VerifiedTrapIdentity> {
    if error.downcast_ref::<wasmtime::Trap>() != Some(&wasmtime::Trap::UnreachableCodeReached) {
        return None;
    }
    let trace = error.downcast_ref::<wasmtime::WasmBacktrace>()?;
    let frame = trace.frames().first()?;
    if frame.module().image_range() != component.image_range()
        || frame.module().exports().map(|export| export.name()).collect::<Vec<_>>()
            != ["run", "$zryna$observation"]
    {
        return None;
    }
    let offset = u64::try_from(frame.module_offset()?).ok()?;
    sites
        .iter()
        .find(|site| site.function_index == frame.func_index() && site.module_offset == offset)
        .map(|site| site.identity)
}

#[test]
fn command_component_executes_real_canonical_environment_lowering_with_storage_first() {
    for name in ["environment-match", "environment-helper", "environment-live-prefix"] {
        for value in [String::new(), "on".into(), "हिन्दी🙂".into(), "a".repeat(1024)]
        {
            let (result, calls, denied) = run(name, Some(value), false);
            assert_eq!(result.expect("actual canonical Found result"), Ok(()));
            assert_eq!(calls, 1);
            assert!(!denied);
        }
        let (result, calls, denied) = run(name, None, false);
        assert_eq!(result.expect("actual canonical Missing result"), Err(()));
        assert_eq!(calls, 1);
        assert!(!denied);
    }
}

#[test]
fn command_component_pure_entry_uses_no_host_callback_and_denial_has_no_wit_result() {
    let (result, calls, denied) = run("pure-entry", None, true);
    assert_eq!(result.expect("pure run"), Ok(()));
    assert_eq!(calls, 0);
    assert!(!denied);
    let (result, calls, denied) = run("environment-match", Some("on".into()), true);
    assert!(result.is_err());
    assert_eq!(calls, 0);
    assert!(denied);
}

#[test]
fn command_component_controlled_identity_requires_its_exact_audited_run_trap_site() {
    let (result, calls, denied, identity) = run_audited("language-bounds", None, true);
    assert!(result.is_err(), "bounds failure has no WIT run result");
    assert_eq!(calls, 0);
    assert!(!denied);
    assert_eq!(identity, Some(zryna_ir::data_ownership_v1::VerifiedTrapIdentity::BoundsV1));
    let (result, calls, denied, identity) = run_audited("fuel-exhaustion", None, true);
    assert!(result.is_err(), "fuel exhaustion has no WIT run result");
    assert_eq!(calls, 0);
    assert!(!denied);
    assert_eq!(identity, None, "fuel exhaustion cannot inherit a language identity");
    let (_, _, denied, identity) = run_audited("environment-match", Some("on".into()), true);
    assert!(denied);
    assert_eq!(identity, None, "host-origin denial cannot inherit a language identity");
}
