//! Malformed test-host canonical output is never an admitted driver request.

use super::*;

fn classify(
    error: &wasmtime::Error,
    component: &Component,
    sites: &[crate::CommandInterfaceTrapSite],
) -> Option<&'static str> {
    if error.downcast_ref::<wasmtime::Trap>() != Some(&wasmtime::Trap::UnreachableCodeReached) {
        return None;
    }
    let frame = error.downcast_ref::<wasmtime::WasmBacktrace>()?.frames().first()?;
    if frame.module().image_range() != component.image_range()
        || frame.module().exports().map(|export| export.name()).collect::<Vec<_>>()
            != [
                "memory",
                "arena",
                "status",
                "drops",
                "live",
                "peak",
                "references",
                "allocate",
                "copy",
                "realloc",
                "validate",
                "drain",
                "canonical-state",
            ]
    {
        return None;
    }
    let offset = u64::try_from(frame.module_offset()?).ok()?;
    sites
        .iter()
        .find(|site| site.function_index() == frame.func_index() && site.module_offset() == offset)
        .map(|site| site.identity())
}

#[test]
fn actual_embedded_canonical_guard_requires_exact_component_image_and_coordinates() {
    let mut config = Config::new();
    config
        .consume_fuel(true)
        .generate_address_map(true)
        .wasm_backtrace_max_frames(std::num::NonZeroUsize::new(1));
    let engine = Engine::new(&config).expect("pinned actual engine");
    let (bytes, language_sites) = candidate("environment-match");
    let sites = crate::command_h1_v1::interface_audit::audit(
        &crate::command_h1_v1::storage::encode(),
        &bytes,
    )
    .expect("independently audited storage coordinates");
    assert!(
        sites.iter().all(|site| bytes[usize::try_from(site.module_offset()).expect("offset")] == 0)
    );
    let component = Component::new(&engine, &bytes).expect("actual sealed component");
    let (foreign_bytes, _) = candidate("pure-entry");
    let foreign = Component::new(&engine, foreign_bytes).expect("different valid component image");
    for length in [1025, 4097] {
        let imports = linker(&engine, &component);
        let limits = StoreLimitsBuilder::new()
            .memory_size(16_777_216)
            .instances(2)
            .memories(1)
            .tables(0)
            .build();
        let mut store = Store::new(
            &engine,
            Host { limits, value: Some("x".repeat(length)), deny: false, calls: 0, denied: false },
        );
        store.limiter(|host| &mut host.limits);
        store.set_fuel(100_000).expect("same finite fuel");
        let instance = imports.instantiate(&mut store, &component).expect("real instantiation");
        let interface = instance
            .get_export_index(&mut store, None, "wasi:cli/run@0.2.12")
            .expect("run interface");
        let export = instance.get_export_index(&mut store, Some(&interface), "run").expect("run");
        let run = instance
            .get_typed_func::<(), (Result<(), ()>,)>(&mut store, &export)
            .expect("canonical declared run type");
        let error = run.call(&mut store, ()).expect_err("malformed host transfer is fatal");
        assert_eq!(store.data().calls, 1);
        assert!(!store.data().denied, "allocator failure is not policy denial");
        assert_eq!(
            classify(&error, &component, &sites),
            Some("zryna.command.interface-violation.v1")
        );
        assert_eq!(
            classify(&error, &foreign, &sites),
            None,
            "coordinates do not authorize a foreign image"
        );
        assert_eq!(classify(&error, &component, &[]), None, "no unaudited identity");
        assert_eq!(
            controlled_identity(&error, &component, &language_sites),
            None,
            "canonical failure is not language E1"
        );
        drop(store);
    }
    assert_eq!(
        run("environment-match", Some("on".into()), false).0.expect("fresh recovery"),
        Ok(())
    );
}
