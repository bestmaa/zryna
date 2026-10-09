use super::cleanup::words;
use super::{Command, Input};
use zryna_ir::data_ownership_v1::{
    VerifiedDropActionKind as D, VerifiedInstructionKind as K, VerifiedModule,
};

struct Case {
    name: &'static str,
    kind: D,
    preparation: &'static [K],
    completed_fault: i32,
    prefix: &'static [i32],
    survivor: &'static [i32],
}

const CASES: [Case; 3] = [
    Case {
        name: "clone-aggregate",
        kind: D::GenericCloneInitializedPrefix,
        preparation: &[K::StringFromUtf8, K::StringFromUtf8, K::StructConstruct],
        completed_fault: 3,
        prefix: &[2, 1],
        survivor: &[2, 1, 1],
    },
    Case {
        name: "clone-vector",
        kind: D::GenericCloneInitializedPrefix,
        preparation: &[K::StringFromUtf8, K::StringFromUtf8, K::VecConstruct],
        completed_fault: 3,
        prefix: &[2, 1],
        survivor: &[2, 1, 1],
    },
    Case {
        name: "clone-generic",
        kind: D::GenericCloneInitializedPrefix,
        preparation: &[
            K::StringFromUtf8,
            K::StringFromUtf8,
            K::VecConstruct,
            K::StringFromUtf8,
            K::StructConstruct,
        ],
        completed_fault: 4,
        prefix: &[2, 1, 2],
        survivor: &[2, 1, 2, 1, 1],
    },
];

impl Case {
    fn expected(&self, command: &Command, prefix_values: &[i32]) -> Vec<i32> {
        let function = command
            .program
            .modules()
            .flat_map(VerifiedModule::functions)
            .find(|function| function.public_export().is_some())
            .expect("source main");
        assert_eq!(function.id().module(), 0, "one source module");
        let blocks = function.blocks().collect::<Vec<_>>();
        assert_eq!(blocks.len(), 1, "closed straight-line source setup");
        assert_eq!(
            blocks[0].terminator().kind(),
            zryna_ir::data_ownership_v1::VerifiedTerminatorKind::Return
        );
        let preparation = blocks[0]
            .instructions()
            .take_while(|instruction| {
                !matches!(instruction.kind(), K::ClonePlace | K::VecClone | K::GenericClonePlace)
            })
            .filter_map(|instruction| match instruction.kind() {
                kind @ (K::StringFromUtf8 | K::StructConstruct | K::VecConstruct) => Some(kind),
                K::InitializePlace
                | K::MoveFromPlace
                | K::GenericMoveFromPlace
                | K::CopyFromPlace => None,
                kind => panic!("unexpected operation before the source clone: {kind:?}"),
            })
            .collect::<Vec<_>>();
        assert_eq!(preparation, self.preparation, "exact source construction probe inventory");
        let clone = function
            .blocks()
            .flat_map(zryna_ir::data_ownership_v1::VerifiedBlock::instructions)
            .find(|instruction| {
                matches!(instruction.kind(), K::ClonePlace | K::VecClone | K::GenericClonePlace)
            })
            .expect("source clone has a sealed dynamic prefix");
        let actions = match clone.kind() {
            K::ClonePlace => {
                clone.aggregate_clone_element_failure_drop_actions().collect::<Vec<_>>()
            }
            K::VecClone => clone.vec_clone_element_failure_drop_actions().collect::<Vec<_>>(),
            K::GenericClonePlace => {
                clone.generic_clone_prefix_failure_drop_actions().collect::<Vec<_>>()
            }
            _ => unreachable!(),
        };
        assert_eq!(
            actions[0].kind(),
            self.kind,
            "{} source reaches the claimed prefix kind",
            self.name
        );
        assert_eq!(&actions[1..], clone.derived_drop_actions().collect::<Vec<_>>());
        assert_eq!(actions.len(), 2, "unpublished destination and one pre-existing source owner");
        assert_eq!(actions[1].kind(), D::Place);
        assert_eq!(actions[1].initialized_projections().len(), 0, "source owner is complete");
        assert_eq!(actions[1].moved_projections().len(), 0, "source children remain live");
        assert_eq!(clone.place_operands().next(), Some(actions[1].root()));
        let result = clone.result().expect("unpublished clone result");
        let destination = function
            .places()
            .find(|place| {
                place.kind() == zryna_ir::data_ownership_v1::VerifiedPlaceKind::Temporary(result)
            })
            .expect("sealed temporary");
        assert_eq!(actions[0].root(), destination.id());
        assert_ne!(actions[0].root(), actions[1].root());
        let mut expected = Vec::new();
        if !prefix_values.is_empty() {
            expected.extend(words(function, &actions[1], 2)[..2].iter().copied());
            expected.push(i32::try_from(actions[0].root().index()).expect("prefix root"));
            expected.extend(prefix_values.iter().map(|kind| 0x1000_0000 + kind));
        }
        expected.extend(words(function, &actions[1], self.survivor[0]));
        expected.extend(self.survivor[1..].iter().map(|kind| 0x1000_0000 + kind));
        expected
    }

    fn check(&self, fault: i32, prefix_values: &[i32]) {
        let mut command = Command::new(self.name, Input::Denied);
        let expected = self.expected(&command, prefix_values);
        // Code 2 also counts String and owner construction. The independently
        // asserted straight-line setup must finish before the clone-relative
        // fault enters its destination frame or owned children.
        let selected = i32::try_from(self.preparation.len()).expect("bounded preparation") + fault;
        assert_eq!(
            command
                .observation
                .call(&mut command.store, 0x2200_0000 + selected)
                .expect("closed clone fault selector"),
            0
        );
        assert!(command.run.call(&mut command.store, ()).is_err(), "checked clone failure");
        assert_eq!(
            command.trace(),
            expected,
            "{} fault {fault}: prefix before survivor, exactly once",
            self.name
        );
        assert_eq!(command.canonical_state(1), 0);
        assert_eq!(command.canonical_state(3), 0);
        // Same-instance private-core recovery checks stale state. Production
        // execution still consumes its Store and admits no retry.
        command.enable_trace();
        assert_eq!(
            command.run.call(&mut command.store, ()).expect("subsequent private-core operation"),
            0
        );
        let successful = command.trace();
        assert_eq!(
            successful.iter().filter(|word| **word == 0x2000_0000).count(),
            2,
            "success owns only the published copy and source cleanup roots"
        );
        assert_eq!(command.canonical_state(1), 0);
        assert_eq!(command.canonical_state(3), 0);
    }
}

#[test]
fn command_clone_prefixes_consume_exact_destination_once_before_recursive_unwind() {
    for case in &CASES {
        for (fault, prefix_values) in
            [(1, &[][..]), (2, &[2][..]), (case.completed_fault, case.prefix)]
        {
            case.check(fault, prefix_values);
        }
    }
}
