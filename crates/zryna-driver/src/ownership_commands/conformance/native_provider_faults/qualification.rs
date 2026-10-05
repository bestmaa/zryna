//! Retained evidence for cold publication and real source-mutation attempts.

use super::*;

fn destination(label: &str) -> Option<std::path::PathBuf> {
    let parent = std::path::PathBuf::from(std::env::var_os("ZRYNA_M3_NATIVE_FAULT_EVIDENCE")?);
    assert!(parent.is_absolute() && parent.is_dir());
    let path = parent.join(label);
    fs::create_dir(&path).expect("create-only qualification evidence");
    Some(path)
}

fn save_bundle(path: &Path, bundle: &PublishedOwnershipBundle) {
    for (relative, bytes) in inventory(bundle.path()) {
        let file = path.join(relative);
        fs::create_dir_all(file.parent().expect("bundle parent")).expect("evidence parent");
        fs::write(file, bytes).expect("complete evidence bundle");
    }
}

fn write(path: &Path, value: &Value) {
    fs::write(path.join("control.json"), serde_json::to_vec_pretty(value).expect("control JSON"))
        .expect("retained qualification control");
}

pub(super) fn retain_collision(
    request: &DataOwnershipBuildRequest,
    target: TargetSelection,
    bundle: &PublishedOwnershipBundle,
    checkpoints: &Checkpoints,
    failure: &CommandFailure,
) {
    let Some(path) = destination(&format!("{}-cold-collision", target_name(target))) else {
        return;
    };
    save_bundle(&path.join("bundle"), bundle);
    fs::write(
        path.join("main.zry"),
        fs::read(request.workspace_root.join("main.zry")).expect("unchanged source bytes"),
    )
    .expect("retained source");
    write(
        &path,
        &serde_json::json!({
            "control":"cold-collision","target":target_name(target),
            "platform":std::env::consts::OS,"native_first":true,
            "execution_checkpoints":checkpoints.execution.get(),
            "publication_checkpoints":*checkpoints.publication.borrow(),
            "failure_kind":format!("{:?}",failure.kind()),"diagnostics":failure.diagnostics(),
            "output_entries":fs::read_dir(request.workspace_root.join(".zryna/out")).expect("output inventory").count(),
            "results":bundle.results(),"public_activation":false,
        }),
    );
}

pub(super) fn retain_mutation(
    request: &DataOwnershipBuildRequest,
    mutation: Mutation,
    checkpoints: &Checkpoints,
    failure: Option<&CommandFailure>,
    bundle: Option<&PublishedOwnershipBundle>,
) {
    let path = destination(&format!("mutation-{mutation:?}"));
    if let Some(bundle) = bundle {
        if let Some(path) = &path {
            save_bundle(&path.join("bundle"), bundle);
        }
        fs::remove_dir_all(bundle.path()).expect("test-owned prevented-mutation bundle cleanup");
    }
    assert_no_artifacts(&request.workspace_root);
    let Some(path) = path else { return };
    #[cfg(windows)]
    let attempt = checkpoints.attempt.borrow().clone().expect("actual mutation attempt");
    #[cfg(not(windows))]
    let attempt = serde_json::json!({"action":"changed","io_error":null});
    write(
        &path,
        &serde_json::json!({
            "control":"source-mutation","checkpoint":format!("{mutation:?}"),
            "platform":std::env::consts::OS,"attempt":attempt,
            "mutated":checkpoints.mutated.get(),
            "execution_checkpoints":checkpoints.execution.get(),
            "publication_checkpoints":*checkpoints.publication.borrow(),
            "failure_kind":failure.map(|f|format!("{:?}",f.kind())),
            "diagnostics":failure.map(CommandFailure::diagnostics).unwrap_or_default(),
            "results":bundle.map(PublishedOwnershipBundle::results).unwrap_or_default(),
            "output_entries":fs::read_dir(request.workspace_root.join(".zryna/out")).expect("clean output inventory").count(),
            "public_activation":false,
        }),
    );
}

#[cfg(windows)]
pub(super) fn attempt(
    request: &DataOwnershipBuildRequest,
    checkpoints: &Checkpoints,
    mutation: Option<Mutation>,
) {
    assert!(
        checkpoints.attempt.borrow().is_none(),
        "one actual attempt at the selected checkpoint"
    );
    let source = request.workspace_root.join(&request.entrypoint);
    let before = fs::read(&source).expect("retained source before attempt");
    let parent = source.parent().expect("source parent");
    let original = same_file::Handle::from_path(parent).expect("original parent identity");
    let result = if matches!(mutation, Some(Mutation::SourceDirectory)) {
        fs::rename(parent, request.workspace_root.join("replaced-sources"))
    } else {
        fs::write(&source, "export function score(): i32 { return 99; }\n")
    };
    let error = match result {
        Ok(()) => {
            if matches!(mutation, Some(Mutation::SourceDirectory)) {
                fs::create_dir(parent).expect("replacement source directory");
                fs::write(&source, &before).expect("identical replacement bytes");
                assert_ne!(
                    original,
                    same_file::Handle::from_path(parent).expect("replacement identity")
                );
            }
            checkpoints.mutated.set(true);
            None
        }
        Err(error) => {
            // The attempted operation itself must establish OS denial; arbitrary I/O failures reject.
            assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
            assert_eq!(error.raw_os_error(), Some(32), "expected Windows sharing violation");
            assert_eq!(fs::read(&source).expect("source after denied attempt"), before);
            assert_eq!(
                original,
                same_file::Handle::from_path(parent).expect("parent after denied attempt")
            );
            Some(serde_json::json!({"kind":"PermissionDenied","raw_os_error":error.raw_os_error()}))
        }
    };
    let after = fs::read(&source).expect("source after actual attempt");
    *checkpoints.attempt.borrow_mut() = Some(serde_json::json!({
        "action":if checkpoints.mutated.get(){"changed"}else{"prevented"},
        "io_error":error,"before_sha256":format!("{:x}",Sha256::digest(before)),
        "after_sha256":format!("{:x}",Sha256::digest(after)),
        "parent_replaced":original != same_file::Handle::from_path(parent).expect("parent after attempt"),
    }));
}

#[cfg(windows)]
#[test]
fn windows_source_mutation_attempts_are_prevented_or_detected_and_cleanup() {
    let _guard = route_guard();
    let registry = registry();
    for mutation in
        [Mutation::Execution, Mutation::Manifest, Mutation::Commit, Mutation::SourceDirectory]
    {
        let workspace = fixture_workspace();
        install(workspace.root(), &registry, "vec");
        let mut request = request(workspace.root(), TargetSelection::JavaScript);
        if matches!(mutation, Mutation::SourceDirectory) {
            fs::create_dir(workspace.root().join("sources")).expect("source-directory fixture");
            fs::rename(
                workspace.root().join("main.zry"),
                workspace.root().join("sources/main.zry"),
            )
            .expect("source below retained parent");
            request.entrypoint = "sources/main.zry".to_owned();
        }
        assert!(!workspace.root().join(".zryna").exists());
        let checkpoints = Checkpoints::default();
        let result = native_run(&request, None, Some(mutation), &checkpoints);
        assert!(checkpoints.attempt.borrow().is_some(), "selected checkpoint actually reached");
        if checkpoints.mutated.get() {
            let failure = result.expect_err("actual source mutation rejects");
            assert_eq!(failure.kind(), CommandFailureKind::Source);
            assert_eq!(failure.diagnostics()[0].code(), "ZRYNA-D3004");
            retain_mutation(&request, mutation, &checkpoints, Some(&failure), None);
        } else {
            let bundle = result.expect("OS denied mutation; immutable source still publishes");
            assert_observation(&bundle, TargetSelection::JavaScript, None);
            assert_eq!(checkpoints.execution.get(), 3);
            retain_mutation(&request, mutation, &checkpoints, None, Some(&bundle));
        }
        println!(
            "retained Windows source guard {mutation:?}: {:?}; cleanup complete",
            checkpoints.attempt.borrow()
        );
    }
}
