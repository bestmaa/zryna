use super::Fixture;
use zryna_source::{SourceFileInput, SourceMap};

#[test]
fn stale_source_map_or_source_bytes_cannot_replace_the_issuing_source() {
    let fixture = Fixture::new();
    for replacement in ["MODE", "HOME"] {
        let text =
            include_str!("../../../../../tests/wasi-command-source-fixtures/environment-match.zry")
                .replace("MODE", replacement);
        let sources = SourceMap::build(vec![SourceFileInput { path: "src/main.zry".into(), text }])
            .expect("independent source map");
        let errors = super::super::verify(
            fixture.seed(),
            &sources,
            &fixture.source,
            fixture.linear.clone(),
            fixture.linux.clone(),
        )
        .expect_err("stale authority");
        assert!(errors.iter().any(|error| error.code() == "ZRYNA-I3003"), "{errors:?}");
    }
    fixture.check(fixture.seed()).expect("recovery");
}

#[test]
fn exact_command_issuing_identity_is_distinct_and_clone_preserves_it() {
    let fixture = Fixture::new();
    let first = fixture.check(fixture.seed()).expect("first");
    let second = fixture.check(fixture.seed()).expect("second");
    assert_ne!(first.identity(), second.identity());
    assert_eq!(first.identity(), first.clone().identity());
    assert_eq!(first.identity().source_map(), fixture.sources.identity());
    assert_eq!(
        first.runtime_contract(),
        crate::data_ownership_v1::RuntimeContractIdentity::CommandH1V1
    );
    assert_eq!(first.memory_partition(), second.memory_partition());
    assert_eq!(super::super::MemoryPartition::MEMORY_BYTES, 256 * 65_536);
    assert_eq!(
        super::super::MemoryPartition::CANONICAL_END
            - super::super::MemoryPartition::CANONICAL_START,
        1_048_576
    );
}
