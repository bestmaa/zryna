use super::{Command, Input};
use zryna_ir::data_ownership_v1::{
    VerifiedDropAction, VerifiedDropActionKind, VerifiedFunction, VerifiedInstructionKind as K,
    VerifiedModule, VerifiedTerminatorKind,
};
use zryna_layout::TypeCategory;

pub(super) fn words(
    function: VerifiedFunction<'_>,
    action: &VerifiedDropAction,
    kind: i32,
) -> [i32; 4] {
    assert_eq!(action.kind(), VerifiedDropActionKind::Place);
    [
        i32::try_from(0x2000_0000 + function.id().module()).expect("module label"),
        i32::try_from(function.id().declaration()).expect("declaration label"),
        i32::try_from(action.root().index()).expect("root label"),
        0x1000_0000 + kind,
    ]
}

impl Command {
    pub(super) fn enable_trace(&mut self) {
        assert_eq!(
            self.observation.call(&mut self.store, 0x2000_0001).expect("enable without fault"),
            0
        );
    }

    pub(super) fn trace(&mut self) -> Vec<i32> {
        let count =
            self.observation.call(&mut self.store, 0x4000_0000).expect("actual trace count");
        assert!((0..=4096).contains(&count));
        (1..=count)
            .map(|index| {
                self.observation
                    .call(&mut self.store, 0x4000_0000 + index)
                    .expect("actual trace word")
            })
            .collect()
    }
}

#[test]
fn command_consumed_environment_payload_has_exact_sealed_root_and_value_drop_trace() {
    let mut command = Command::new("environment-consume", Input::Present(b"on".to_vec()));
    let functions =
        command.program.modules().flat_map(VerifiedModule::functions).collect::<Vec<_>>();
    assert_eq!(functions.len(), 2);
    let consumer = functions
        .iter()
        .copied()
        .find(|function| function.parameters().len() == 1)
        .expect("source consumer has one owned String parameter");
    let layouts = command.program.linear32_layouts();
    assert_eq!(
        layouts
            .type_by_id(consumer.parameters().next().expect("parameter").ty())
            .expect("parameter layout")
            .category(),
        TypeCategory::String
    );
    let blocks = consumer.blocks().collect::<Vec<_>>();
    assert_eq!(blocks.len(), 1, "consumer source is straight-line");
    assert_eq!(blocks[0].terminator().kind(), VerifiedTerminatorKind::Return);
    // The test reads the sealed IR facts, independently of the encoder's cleanup plan.
    // Only explicit normal drops execute on this successful straight-line source path.
    let mut drops = blocks[0]
        .instructions()
        .filter(|instruction| instruction.kind() == K::DropPlace)
        .flat_map(zryna_ir::data_ownership_v1::VerifiedInstruction::derived_drop_actions)
        .collect::<Vec<_>>();
    drops.extend(blocks[0].terminator().derived_drop_actions());
    assert_eq!(
        drops.len(),
        4,
        "parameter, copied String, literal and joined String each own one drop"
    );
    let mut expected = Vec::new();
    for action in &drops {
        let place =
            consumer.places().find(|place| place.id() == action.root()).expect("consumer root");
        assert_eq!(
            layouts.type_by_id(place.ty()).expect("root layout").category(),
            TypeCategory::String
        );
        assert_eq!(action.moved_projections().len(), 0);
        expected.extend(words(consumer, action, 1));
    }
    let entry = functions
        .iter()
        .copied()
        .find(|function| function.public_export().is_some())
        .expect("main");
    let found = entry
        .blocks()
        .flat_map(|block| {
            block
                .instructions()
                .filter(|instruction| instruction.kind() == K::DropPlace)
                .flat_map(zryna_ir::data_ownership_v1::VerifiedInstruction::derived_drop_actions)
                .chain(block.terminator().derived_drop_actions())
        })
        .filter(|action| action.active_variant() == Some(0))
        .collect::<Vec<_>>();
    assert_eq!(found.len(), 1, "Found retains its enum owner after transferring its payload");
    assert_eq!(found[0].initialized_projections().len(), 0, "no retained initialized payload");
    let shell =
        entry.places().find(|place| place.id() == found[0].root()).expect("retained enum root");
    assert_eq!(
        layouts.type_by_id(shell.ty()).expect("shell layout").category(),
        TypeCategory::Enum
    );
    let moved = found[0].moved_projections().collect::<Vec<_>>();
    assert_eq!(moved.len(), 1, "the exact Found String payload was transferred to consume");
    let payload =
        entry.places().find(|place| place.id() == moved[0]).expect("sealed moved payload");
    assert_eq!(
        payload.kind(),
        zryna_ir::data_ownership_v1::VerifiedPlaceKind::EnumPayload {
            base: found[0].root(),
            variant: 0,
        }
    );
    assert_eq!(
        layouts.type_by_id(payload.ty()).expect("payload layout").category(),
        TypeCategory::String
    );
    expected.extend(words(entry, &found[0], 3));
    command.enable_trace();
    assert_eq!(command.run.call(&mut command.store, ()).expect("consumed Found run"), 0);
    assert_eq!(
        command.trace(),
        expected,
        "each String drops once; the caller drops only its enum shell"
    );
    assert_eq!(command.canonical_state(1), 0);
    assert_eq!(command.canonical_state(3), 0);
}

#[test]
fn command_language_allocation_failure_drops_live_prefix_in_reverse_before_drain_and_reset() {
    let mut command = Command::new("environment-live-prefix", Input::FailLanguage);
    let entry = command
        .program
        .modules()
        .flat_map(VerifiedModule::functions)
        .find(|function| function.public_export().is_some())
        .expect("main");
    let lookup = entry
        .blocks()
        .flat_map(zryna_ir::data_ownership_v1::VerifiedBlock::instructions)
        .find(|instruction| instruction.kind() == K::EnvironmentLookup)
        .expect("source lookup");
    let actions = lookup.derived_drop_actions().collect::<Vec<_>>();
    assert_eq!(actions.iter().map(|action| action.root().index()).collect::<Vec<_>>(), [3, 1]);
    let expected = actions.iter().flat_map(|action| words(entry, action, 1)).collect::<Vec<_>>();
    command.enable_trace();
    let error = command.run.call(&mut command.store, ()).expect_err("real L allocation exhaustion");
    assert_eq!(command.trace(), expected);
    assert_eq!(
        error.downcast_ref::<wasmtime::Trap>(),
        Some(&wasmtime::Trap::UnreachableCodeReached)
    );
    let backtrace =
        error.downcast_ref::<wasmtime::WasmBacktrace>().expect("pinned engine backtrace");
    let frame = backtrace.frames().first().expect("controlled Run frame");
    assert!(wasmtime::Module::same(frame.module(), &command.module));
    let offset = u64::try_from(frame.module_offset().expect("actual opcode address"))
        .expect("bounded offset");
    assert_eq!(
        command
            .sites
            .iter()
            .find(|site| site.function_index == frame.func_index() && site.module_offset == offset)
            .map(|site| site.identity),
        Some(zryna_ir::data_ownership_v1::VerifiedTrapIdentity::AllocationV1)
    );
    assert_eq!(command.canonical_state(1), 0, "C live records drained");
    assert_eq!(command.canonical_state(0), 15_728_640, "C arena rewound");
    assert_eq!(command.canonical_state(3), 0, "fatal C state unchanged");
    let arena = command.storage.get_global(&mut command.store, "arena").expect("L arena");
    assert_eq!(
        arena.get(&mut command.store).i32(),
        Some(65_536),
        "L arena reset before qualified trap"
    );
}
