use std::sync::OnceLock;
use wasmtime::{
    Caller, Config, Engine, Global, Instance, Linker, Memory, Module, Store, StoreLimits,
    StoreLimitsBuilder, TypedFunc, Val,
};
use zryna_source::{SourceFileInput, SourceMap};
use zryna_syntax::{command_h1_v1, v4};

mod cleanup;
mod frontier;

type Realloc = TypedFunc<(i32, i32, i32, i32), i32>;

#[derive(Clone)]
enum Input {
    Missing,
    Present(Vec<u8>),
    Denied,
    WrongKey,
    FailLanguage,
    LanguageStatus(i32),
}

struct Host {
    limits: StoreLimits,
    input: Input,
    calls: usize,
}

struct Command {
    store: Store<Host>,
    storage: Instance,
    run: TypedFunc<(), i32>,
    module: Module,
    sites: Vec<super::run_audit::TrapSite>,
    observation: TypedFunc<i32, i32>,
    program: zryna_ir::command_h1_v1::VerifiedProgram,
}

struct Transfer {
    realloc: Realloc,
    memory: Memory,
    arena: Global,
    status: Global,
}

impl Command {
    fn new(name: &str, input: Input) -> Self {
        static ENGINE: OnceLock<Engine> = OnceLock::new();
        let engine = ENGINE.get_or_init(|| {
            let mut config = Config::new();
            config.consume_fuel(true).max_wasm_stack(65_536).generate_address_map(true);
            Engine::new(&config).expect("pinned command test engine")
        });
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/wasi-command-source-fixtures");
        let text = std::fs::read_to_string(root.join(format!("{name}.zry")))
            .expect("command source fixture");
        let bytes =
            std::fs::read(root.join(format!("{name}.json"))).expect("pinned provider fixture");
        let sources = SourceMap::build(vec![SourceFileInput { path: "src/main.zry".into(), text }])
            .expect("source map");
        let syntax =
            v4::verify_snapshot(v4::decode_snapshot(&bytes).expect("decoded source"), &sources)
                .expect("bound source");
        let source =
            command_h1_v1::admit(&syntax, &sources).expect("whole-file H1 source admission");
        let program =
            zryna_semantics::command_h1_v1::lower(&source, &sources).expect("mandatory command IR");
        let core = crate::data_ownership_v1::encode::command(program.verified_ir())
            .expect("command core encoding");
        let sites = super::language_audit::audit(&core, program.verified_ir())
            .expect("whole-body command capability and run audit");
        assert_eq!(sites.len(), 5);
        wasmparser::Validator::new_with_features(wasmparser::WasmFeatures::WASM1)
            .validate_all(&core)
            .expect("command core validation");
        let storage_bytes = super::storage::encode();
        super::storage::audit::audit(&storage_bytes).expect("independent fixed storage audit");
        let storage_module = Module::new(engine, storage_bytes).expect("storage module");
        let core_module = Module::new(engine, core).expect("language module");
        let limits = StoreLimitsBuilder::new()
            .memory_size(16_777_216)
            .instances(2)
            .memories(1)
            .tables(0)
            .build();
        let mut store = Store::new(engine, Host { limits, input, calls: 0 });
        store.limiter(|host| &mut host.limits);
        store.set_fuel(50_000_000).expect("finite fixture fuel");
        let storage = Instance::new(&mut store, &storage_module, &[]).expect("storage first");
        let realloc: Realloc =
            storage.get_typed_func(&mut store, "realloc").expect("storage realloc");
        let memory = storage.get_memory(&mut store, "memory").expect("shared fixed memory");
        let arena = storage.get_global(&mut store, "arena").expect("language arena");
        let status = storage.get_global(&mut store, "status").expect("private language status");
        let transfer = Transfer { realloc, memory, arena, status };
        let key = source
            .environment()
            .map_or_else(|| "MODE".to_owned(), |effect| effect.key().to_owned());
        let mut linker = Linker::new(engine);
        linker.instance(&mut store, "storage", storage).expect("exact shared storage aliases");
        linker
            .func_wrap(
                "host",
                "get-environment",
                move |mut caller: Caller<'_, Host>, result: i32| {
                    caller.data_mut().calls += 1;
                    let input = caller.data().input.clone();
                    callback(&mut caller, result, &key, &input, &transfer)
                },
            )
            .expect("closed result-area callback");
        let instance = linker
            .instantiate(&mut store, &core_module)
            .expect("language consumes sole shared memory");
        let run = instance.get_typed_func(&mut store, "run").expect("closed run carrier");
        let observation = instance
            .get_typed_func(&mut store, "$zryna$observation")
            .expect("existing private observation carrier");
        Self {
            store,
            storage,
            run,
            module: core_module,
            sites,
            observation,
            program: program.verified_ir().clone(),
        }
    }

    fn canonical_state(&mut self, selector: i32) -> i32 {
        self.storage
            .get_typed_func::<i32, i32>(&mut self.store, "canonical-state")
            .expect("read-only storage state")
            .call(&mut self.store, selector)
            .expect("core test observation")
    }
}

fn callback(
    caller: &mut Caller<'_, Host>,
    result: i32,
    key: &str,
    input: &Input,
    transfer: &Transfer,
) -> wasmtime::Result<()> {
    let memory = transfer.memory;
    if matches!(input, Input::Denied) {
        return Err(wasmtime::Error::msg("permission denied"));
    }
    let result = usize::try_from(result)?;
    if matches!(input, Input::Missing) {
        memory.write(&mut *caller, result, &[0; 8])?;
        return Ok(());
    }
    let value = match input {
        Input::Present(bytes) => bytes.as_slice(),
        _ => b"on",
    };
    let key = if matches!(input, Input::WrongKey) { b"FAIL".as_slice() } else { key.as_bytes() };
    let key_pointer = transfer.realloc.call(&mut *caller, (0, 0, 1, i32::try_from(key.len())?))?;
    memory.write(&mut *caller, usize::try_from(key_pointer)?, key)?;
    let value_pointer =
        transfer.realloc.call(&mut *caller, (0, 0, 1, i32::try_from(value.len())?))?;
    memory.write(&mut *caller, usize::try_from(value_pointer)?, value)?;
    let list = transfer.realloc.call(&mut *caller, (0, 0, 4, 16))?;
    let tuple =
        [key_pointer, i32::try_from(key.len())?, value_pointer, i32::try_from(value.len())?]
            .into_iter()
            .flat_map(i32::to_le_bytes)
            .collect::<Vec<_>>();
    memory.write(&mut *caller, usize::try_from(list)?, &tuple)?;
    let returned = [list, 1].into_iter().flat_map(i32::to_le_bytes).collect::<Vec<_>>();
    memory.write(&mut *caller, result, &returned)?;
    if matches!(input, Input::FailLanguage) {
        transfer.arena.set(&mut *caller, Val::I32(15_728_640))?;
    }
    if let Input::LanguageStatus(status) = input {
        transfer.status.set(&mut *caller, Val::I32(*status))?;
    }
    Ok(())
}

#[test]
fn command_language_core_executes_owned_control_flow_and_handles_without_host_calls() {
    for name in ["pure-entry", "owned-aggregates", "control-flow", "weak-upgrade"] {
        let mut command = Command::new(name, Input::Denied);
        assert_eq!(command.run.call(&mut command.store, ()).expect("pure command run"), 0);
        assert_eq!(command.store.data().calls, 0);
        assert_eq!(command.canonical_state(1), 0);
    }
}

#[test]
fn command_language_found_empty_missing_utf8_and_helper_results_are_distinct() {
    for name in [
        "environment-match",
        "environment-helper",
        "environment-trailing",
        "environment-live-prefix",
    ] {
        for value in [Vec::new(), b"on".to_vec(), "हिन्दी🙂".as_bytes().to_vec(), vec![b'a'; 1024]]
        {
            let mut command = Command::new(name, Input::Present(value));
            assert_eq!(command.run.call(&mut command.store, ()).expect("owned Found run"), 0);
            assert_eq!(command.store.data().calls, 1);
            assert_eq!(command.canonical_state(1), 0);
            assert_eq!(command.canonical_state(0), 15_728_640);
        }
        let mut command = Command::new(name, Input::Missing);
        assert_eq!(command.run.call(&mut command.store, ()).expect("Missing run"), 1);
        assert_eq!(command.store.data().calls, 1);
        assert_eq!(command.canonical_state(2), 0);
    }
}

#[test]
fn command_language_denial_and_checked_language_failure_have_no_run_return() {
    let mut denied = Command::new("environment-match", Input::Denied);
    assert!(denied.run.call(&mut denied.store, ()).is_err());
    assert_eq!(denied.store.data().calls, 1);
    assert_eq!(denied.canonical_state(2), 0);
    let mut failed = Command::new("environment-live-prefix", Input::FailLanguage);
    assert!(failed.run.call(&mut failed.store, ()).is_err());
    assert_eq!(failed.canonical_state(1), 0);
    assert_eq!(failed.canonical_state(3), 0);
}

#[test]
fn command_language_rejects_independently_injected_wrong_key_and_invalid_utf8() {
    let mut wrong = Command::new("environment-match", Input::WrongKey);
    assert!(wrong.run.call(&mut wrong.store, ()).is_err());
    assert_eq!(wrong.canonical_state(3), 1);
    for bytes in [
        vec![0xc0, 0x80],
        vec![0xed, 0xa0, 0x80],
        vec![0xf4, 0x90, 0x80, 0x80],
        vec![0xe2, 0x82],
        vec![0x80],
    ] {
        let mut command = Command::new("environment-match", Input::Present(bytes));
        assert!(command.run.call(&mut command.store, ()).is_err());
        assert_eq!(command.canonical_state(3), 1);
    }
}

#[test]
fn command_core_injected_statuses_keep_exact_engine_trap_locations_after_drain() {
    use zryna_ir::data_ownership_v1::VerifiedTrapIdentity as T;
    for (status, expected) in [
        (1, Some(T::BoundsV1)),
        (2, Some(T::AllocationV1)),
        (3, Some(T::CapacityV1)),
        (4, Some(T::RefcountV1)),
        (5, Some(T::Utf8V1)),
        (255, None),
    ] {
        let mut command = Command::new("environment-live-prefix", Input::LanguageStatus(status));
        let error = command.run.call(&mut command.store, ()).expect_err("injected checked failure");
        assert_eq!(
            error.downcast_ref::<wasmtime::Trap>(),
            Some(&wasmtime::Trap::UnreachableCodeReached)
        );
        let trace = error.downcast_ref::<wasmtime::WasmBacktrace>().expect("pinned engine trace");
        let frame = trace.frames().first().expect("exact language run frame");
        assert!(Module::same(frame.module(), &command.module));
        let offset = u64::try_from(frame.module_offset().expect("enabled instruction map"))
            .expect("bounded offset");
        let observed = command
            .sites
            .iter()
            .find(|site| site.function_index == frame.func_index() && site.module_offset == offset)
            .map(|site| site.identity);
        assert_eq!(observed, expected);
        assert_eq!(command.canonical_state(1), 0);
        assert_eq!(command.canonical_state(3), 0);
        let arena =
            command.storage.get_global(&mut command.store, "arena").expect("language state");
        assert_eq!(arena.get(&mut command.store).i32(), Some(65_536));
    }
}
