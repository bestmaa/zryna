//! Genuine ordinary M3 source authorities exercise the shared legacy clone frontiers.

use std::sync::OnceLock;
use wasmtime::{
    Config, Engine, Instance, Module, Store, StoreLimits, StoreLimitsBuilder, TypedFunc,
};
use zryna_ir::data_ownership_v1::{
    VerifiedDropAction, VerifiedDropActionKind as D, VerifiedFunction,
    VerifiedInstructionKind as K, VerifiedModule, VerifiedPlaceKind, VerifiedTerminatorKind,
};
use zryna_layout::TypeCategory;
use zryna_semantics::data_ownership_v1::{self as semantics, SemanticInput};
use zryna_source::{SourceFileInput, SourceMap};
use zryna_syntax::v4;

struct Ordinary {
    program: semantics::VerifiedProgram,
    store: Store<StoreLimits>,
    main: TypedFunc<(), i32>,
    observation: TypedFunc<i32, i32>,
}

struct Expected {
    module: i32,
    declaration: i32,
    destination: i32,
    survivor: Vec<i32>,
    success: Vec<i32>,
}

impl Ordinary {
    fn new(name: &str) -> Self {
        static ENGINE: OnceLock<Engine> = OnceLock::new();
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/m3-fixtures");
        let text =
            std::fs::read_to_string(root.join(format!("{name}.zry"))).expect("ordinary source");
        let bytes =
            std::fs::read(root.join(format!("{name}.json"))).expect("pinned provider response");
        let sources = SourceMap::build(vec![SourceFileInput { path: "src/main.zry".into(), text }])
            .expect("original source authority");
        let syntax =
            v4::verify_snapshot(v4::decode_snapshot(&bytes).expect("provider decoding"), &sources)
                .expect("source-bound ordinary syntax");
        let entry = sources.verify_file_id(0).expect("one real source file");
        let input =
            SemanticInput::try_new(&syntax, &sources, entry).expect("ordinary semantic input");
        let program = semantics::lower(input).expect("actual mandatory ordinary M3 verification");
        let artifact = crate::emit_data_ownership(program.verified_ir(), program.runtime_abi())
            .expect("actual M3 ABI-bound audited artifact");
        let exports = program
            .modules()
            .flat_map(VerifiedModule::functions)
            .filter_map(VerifiedFunction::public_export)
            .collect::<Vec<_>>();
        let [export] = exports.as_slice() else {
            panic!("one scalar public source entry");
        };
        let export_name = export.webassembly_name();
        let engine = ENGINE.get_or_init(|| {
            let mut config = Config::new();
            config.consume_fuel(true).max_wasm_stack(65_536);
            Engine::new(&config).expect("pinned ordinary fixture engine")
        });
        let module = Module::new(engine, artifact.bytes()).expect("actual audited module");
        let limits = StoreLimitsBuilder::new()
            .memory_size(16_777_216)
            .instances(1)
            .memories(1)
            .tables(0)
            .build();
        let mut store = Store::new(engine, limits);
        store.limiter(|limits| limits);
        store.set_fuel(50_000_000).expect("finite ordinary fixture fuel");
        let instance =
            Instance::new(&mut store, &module, &[]).expect("import-free ordinary module");
        let main = instance
            .get_typed_func(&mut store, export_name.as_str())
            .expect("actual typed scalar export");
        let observation = instance
            .get_typed_func(&mut store, "$zryna$observation")
            .expect("existing private observation export");
        Self { program, store, main, observation }
    }

    fn expected(&self, kind: D) -> Expected {
        let functions =
            self.program.modules().flat_map(VerifiedModule::functions).collect::<Vec<_>>();
        assert_eq!(
            functions.len(),
            3,
            "legacy callee, private owned consumer, scalar public caller"
        );
        let callee = functions
            .iter()
            .copied()
            .find(|function| {
                function
                    .blocks()
                    .flat_map(zryna_ir::data_ownership_v1::VerifiedBlock::instructions)
                    .any(|instruction| matches!(instruction.kind(), K::ClonePlace | K::VecClone))
            })
            .expect("actual private copied function");
        let entry = functions
            .iter()
            .copied()
            .find(|function| function.public_export().is_some())
            .expect("actual public caller");
        let consumer = functions
            .iter()
            .copied()
            .find(|function| function.public_export().is_none() && function.id() != callee.id())
            .expect("private owned result consumption context");
        let blocks = callee.blocks().collect::<Vec<_>>();
        assert_eq!(blocks.len(), 1, "legacy callee is straight-line");
        let instruction = blocks[0]
            .instructions()
            .find(|instruction| matches!(instruction.kind(), K::ClonePlace | K::VecClone))
            .expect("genuine legacy clone kind, not structural label inference");
        let preparation = blocks[0]
            .instructions()
            .take_while(|instruction| !matches!(instruction.kind(), K::ClonePlace | K::VecClone))
            .filter_map(|instruction| match instruction.kind() {
                kind @ (K::StringFromUtf8 | K::StructConstruct | K::VecConstruct) => Some(kind),
                K::InitializePlace
                | K::MoveFromPlace
                | K::GenericMoveFromPlace
                | K::CopyFromPlace => None,
                kind => panic!("unexpected operation before the legacy clone: {kind:?}"),
            })
            .collect::<Vec<_>>();
        let construction = match kind {
            D::AggregateInitializedPrefix => K::StructConstruct,
            D::VecInitializedPrefix => K::VecConstruct,
            _ => panic!("explicit legacy fixture categories"),
        };
        assert_eq!(
            preparation,
            [K::StringFromUtf8, K::StringFromUtf8, construction],
            "two source Strings and one owner construct before the clone"
        );
        let actions = match kind {
            D::AggregateInitializedPrefix => {
                assert_eq!(instruction.kind(), K::ClonePlace);
                instruction.aggregate_clone_element_failure_drop_actions().collect::<Vec<_>>()
            }
            D::VecInitializedPrefix => {
                assert_eq!(instruction.kind(), K::VecClone);
                instruction.vec_clone_element_failure_drop_actions().collect()
            }
            _ => panic!("explicit legacy fixture categories"),
        };
        assert_eq!(actions.len(), 2, "destination prefix and one live source root");
        assert_eq!(actions[0].kind(), kind);
        assert_eq!(&actions[1..], instruction.derived_drop_actions().collect::<Vec<_>>());
        let result = instruction.result().expect("unpublished actual clone value");
        let destination = callee
            .places()
            .find(|place| place.kind() == VerifiedPlaceKind::Temporary(result))
            .expect("distinct sealed destination owner");
        assert_eq!(actions[0].root(), destination.id());
        assert_ne!(actions[0].root(), actions[1].root());
        assert_eq!(instruction.place_operands().next(), Some(actions[1].root()));
        self.assert_two_strings(callee, &actions[1], kind);
        let constructed = blocks[0]
            .instructions()
            .find(|instruction| matches!(instruction.kind(), K::StructConstruct | K::VecConstruct))
            .expect("two owned source children are actually constructed");
        assert_eq!(constructed.value_operands().len(), 2);
        let survivor = root_words(callee, &actions[1], &[2, 1, 1]);
        let success = self.caller_success(callee, consumer, entry, kind);
        Expected {
            module: i32::try_from(0x2000_0000 + callee.id().module())
                .expect("bounded source module"),
            declaration: i32::try_from(callee.id().declaration()).expect("bounded source function"),
            destination: i32::try_from(destination.id().index())
                .expect("bounded destination place"),
            survivor,
            success,
        }
    }

    fn caller_success(
        &self,
        callee: VerifiedFunction<'_>,
        consumer: VerifiedFunction<'_>,
        entry: VerifiedFunction<'_>,
        kind: D,
    ) -> Vec<i32> {
        for function in [consumer, entry] {
            let calls = function
                .blocks()
                .flat_map(zryna_ir::data_ownership_v1::VerifiedBlock::instructions)
                .filter(|instruction| instruction.kind() == K::DirectCall)
                .collect::<Vec<_>>();
            let [call] = calls.as_slice() else {
                panic!("one genuine caller operation");
            };
            assert_eq!(
                call.derived_drop_actions().len(),
                0,
                "caller has no owner before failed call publication"
            );
        }
        assert_eq!(normal_drops(entry).len(), 0, "public scalar caller owns no aggregate");
        let mut success = Vec::new();
        for function in [callee, consumer] {
            let drops = normal_drops(function);
            assert_eq!(drops.len(), 1, "each successful function drops one completed owner");
            for action in drops {
                self.assert_two_strings(function, &action, kind);
                success.extend(root_words(function, &action, &[2, 1, 1]));
            }
        }
        success
    }

    fn assert_two_strings(
        &self,
        function: VerifiedFunction<'_>,
        action: &VerifiedDropAction,
        kind: D,
    ) {
        assert_eq!(action.kind(), D::Place);
        assert_eq!(action.initialized_projections().len(), 0, "complete initialized owner");
        assert_eq!(action.moved_projections().len(), 0);
        let place = function
            .places()
            .find(|place| place.id() == action.root())
            .expect("exact sealed complete source owner");
        let layouts = self.program.verified_ir().linear32_layouts();
        let ty = layouts.type_by_id(place.ty()).expect("actual complete owner layout");
        match kind {
            D::AggregateInitializedPrefix => {
                assert_eq!(ty.category(), TypeCategory::Struct);
                assert_eq!(ty.fields().len(), 2);
                for field in ty.fields() {
                    assert_eq!(
                        layouts.type_by_id(field.ty()).expect("owned field layout").category(),
                        TypeCategory::String
                    );
                }
            }
            D::VecInitializedPrefix => {
                assert_eq!(ty.category(), TypeCategory::Vec);
                assert_eq!(
                    layouts
                        .type_by_id(ty.referenced_type().expect("element identity"))
                        .expect("owned element layout")
                        .category(),
                    TypeCategory::String
                );
            }
            _ => panic!("explicit legacy shape"),
        }
    }

    fn trace(&mut self) -> Vec<i32> {
        let count = self
            .observation
            .call(&mut self.store, 0x4000_0000)
            .expect("private actual trace count");
        assert!((0..=4096).contains(&count));
        (1..=count)
            .map(|index| {
                self.observation
                    .call(&mut self.store, 0x4000_0000 + index)
                    .expect("actual recorded word")
            })
            .collect()
    }

    fn check(name: &str, kind: D, fault: i32, prefix: &[i32]) {
        let mut ordinary = Self::new(name);
        let expected = ordinary.expected(kind);
        let mut failed = Vec::new();
        if !prefix.is_empty() {
            failed.extend(prefix.iter().map(|kind| 0x1000_0000 + kind));
        }
        failed.extend(&expected.survivor);
        // The exact sealed setup above contributes three code-2 probes before
        // the first clone helper probe. Keep faults relative to that clone.
        assert_eq!(
            ordinary
                .observation
                .call(&mut ordinary.store, 0x2200_0000 + 3 + fault)
                .expect("existing clone-probe fault selector"),
            0
        );
        assert_eq!(
            ordinary.main.call(&mut ordinary.store, ()).expect("ordinary checked failure carrier"),
            0
        );
        assert_eq!(
            ordinary.observation.call(&mut ordinary.store, 0).expect("ordinary retained status"),
            2
        );
        let trace = ordinary.trace();
        assert!(
            !trace
                .windows(3)
                .any(|words| words == [expected.module, expected.declaration, expected.destination]),
            "an unpublished destination has no logical cleanup label"
        );
        assert_eq!(trace, failed, "prefix drops precede exact reverse source cleanup");
        assert_eq!(
            ordinary
                .observation
                .call(&mut ordinary.store, 0x2000_0001)
                .expect("remove fault and retain observation"),
            0
        );
        assert_eq!(
            ordinary
                .main
                .call(&mut ordinary.store, ())
                .expect("same-instance stale-state recovery"),
            1
        );
        assert_eq!(ordinary.observation.call(&mut ordinary.store, 0).expect("recovered status"), 0);
        assert_eq!(
            ordinary.trace(),
            expected.success,
            "success publishes one distinct owner to caller"
        );
        let mut fresh = Self::new(name);
        assert_eq!(fresh.main.call(&mut fresh.store, ()).expect("fresh ordinary recovery"), 1);
    }
}

fn normal_drops(function: VerifiedFunction<'_>) -> Vec<VerifiedDropAction> {
    let blocks = function.blocks().collect::<Vec<_>>();
    assert_eq!(blocks.len(), 1, "normal source path is independently straight-line");
    assert_eq!(blocks[0].terminator().kind(), VerifiedTerminatorKind::Return);
    let mut drops = blocks[0]
        .instructions()
        .filter(|instruction| instruction.kind() == K::DropPlace)
        .flat_map(zryna_ir::data_ownership_v1::VerifiedInstruction::derived_drop_actions)
        .collect::<Vec<_>>();
    drops.extend(blocks[0].terminator().derived_drop_actions());
    drops
}

fn root_words(
    function: VerifiedFunction<'_>,
    action: &VerifiedDropAction,
    values: &[i32],
) -> Vec<i32> {
    let mut words = vec![
        i32::try_from(0x2000_0000 + function.id().module()).expect("module label"),
        i32::try_from(function.id().declaration()).expect("declaration label"),
        i32::try_from(action.root().index()).expect("exact root label"),
    ];
    words.extend(values.iter().map(|kind| 0x1000_0000 + kind));
    words
}

#[test]
fn ordinary_aggregate_clone_uses_actual_aggregate_prefix_before_source_cleanup() {
    for (fault, prefix) in [(1, &[][..]), (2, &[2][..]), (3, &[2, 1][..])] {
        Ordinary::check("clone-prefix-aggregate", D::AggregateInitializedPrefix, fault, prefix);
    }
}

#[test]
fn ordinary_vector_clone_uses_actual_vector_prefix_before_source_cleanup() {
    for (fault, prefix) in [(1, &[][..]), (2, &[2][..]), (3, &[2, 1][..])] {
        Ordinary::check("clone-prefix-vector", D::VecInitializedPrefix, fault, prefix);
    }
}
