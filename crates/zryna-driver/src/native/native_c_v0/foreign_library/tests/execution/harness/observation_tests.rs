use super::*;

#[test]
fn linked_observation_retains_real_scalar_objects_and_never_runs_client() {
    let fixture = Fixture::new();
    let requirements = linked_requirements(&capture::reference(), "imported");
    let foreign = fixture.compile_observed(&library_source()).expect("real observed C compilation");
    let library = accept(&requirements, foreign.bytes(), &["free", "malloc"]);
    let marker = fixture.acquired_path();
    let client = format!(
        "#include <stdio.h>\nint main(void) {{ FILE *f=fopen(\"{}\",\"w\"); if(f)fclose(f); abort(); }}",
        marker.display()
    );
    let observation = fixture
        .observe_link(&requirements, &library, &foreign, &client)
        .expect("compile/link only");
    fixture.empty();
    assert!(!marker.exists(), "client side effect proves the observer did not run it");
    observation.check_elf_rejection_controls();
    foreign.check_failure_retention_controls(
        &fixture.observation_root().expect("independent observed fixture prerequisite"),
    );
    fixture.empty();
    let report = observation.report();
    assert_eq!(report["execution_authorized"], false);
    assert_eq!(report["target_executed"], false);
    assert_eq!(
        report["missing_prerequisites"]
            .as_array()
            .expect("independent observed fixture prerequisite")
            .len(),
        3
    );
    assert!(
        report["observed_linker_order"]
            .as_array()
            .expect("independent observed fixture prerequisite")
            .len()
            > 3
    );
    let directory = std::env::var_os("ZRYNA_LINKED_OUTPUT_EVIDENCE_DIR").map_or_else(
        || fixture.0.join("observed-scalar"),
        |root| PathBuf::from(root).join("scalar"),
    );
    observation.export(&directory).expect("create-only actual artifacts after stage cleanup");
    assert!(observation.export(&directory).is_err(), "no evidence overwrite");
    eprintln!("417-linked-output-observation {report}");
}

#[test]
fn linked_observation_reissued_authority_rejects_before_new_compile_or_link() {
    let fixture = Fixture::new();
    let requirements = linked_requirements(&capture::reference(), "imported");
    let foreign = fixture
        .compile_observed(&library_source())
        .expect("independent observed fixture prerequisite");
    let library = accept(&requirements, foreign.bytes(), &["free", "malloc"]);
    let other = linked_requirements(&capture::reference(), "imported");
    assert_eq!(requirements.object_sha256(), other.object_sha256());
    let failure = fixture
        .observe_link(&other, &library, &foreign, "int main(void){return 0;}")
        .expect_err("expected observational rejection");
    assert!(failure.invocations.is_empty());
    assert_eq!(failure.diagnostics[0].code(), "ZRYNA-C4102");
    fixture.empty();
}

#[test]
fn linked_observation_real_compile_and_link_failures_cleanup_then_retry() {
    let fixture = Fixture::new();
    let failure =
        fixture.compile_observed("not valid C").expect_err("expected observational rejection");
    assert_eq!(failure.invocations.len(), 1);
    fixture.empty();
    let requirements = linked_requirements(&capture::reference(), "imported");
    let foreign = fixture
        .compile_observed(&library_source())
        .expect("independent observed fixture prerequisite");
    let library = accept(&requirements, foreign.bytes(), &["free", "malloc"]);
    let failure = fixture
        .observe_link(
            &requirements,
            &library,
            &foreign,
            "extern void missing(void); int main(void){missing();return 0;}",
        )
        .expect_err("expected observational rejection");
    let report = failure.report();
    assert_eq!(report["invocations"][0]["success"], false);
    assert!(
        !report["invocations"][0]["stderr_bytes"]
            .as_array()
            .expect("independent observed fixture prerequisite")
            .is_empty()
    );
    fixture.empty();
    fixture
        .observe_link(&requirements, &library, &foreign, "int main(void){return 0;}")
        .expect("independent observed fixture prerequisite");
    fixture.empty();
    eprintln!("417-linked-output-failure {report}");
}
