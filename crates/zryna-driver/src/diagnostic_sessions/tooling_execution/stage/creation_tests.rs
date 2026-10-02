//! Filesystem ownership fault fixtures; these do not authenticate compiler materials.

use std::{
    cell::Cell,
    env, fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use sha2::{Digest, Sha256};
use zryna_diagnostics::Diagnostic;

use super::super::capture::CapturedFile;
use super::{MODULES, ROOT, ToolingStage, creation, stage_changed, with_cleanup};

thread_local! {
    static FAILURE: Cell<Option<&'static str>> = const { Cell::new(None) };
}
static NEXT: AtomicU64 = AtomicU64::new(0);

pub(super) fn checkpoint(point: &'static str) -> Result<(), Diagnostic> {
    FAILURE.with(|failure| {
        if failure.get() == Some(point) {
            failure.set(None);
            Err(stage_changed())
        } else {
            Ok(())
        }
    })
}

struct Fault;

impl Fault {
    fn arm(point: &'static str) -> Self {
        FAILURE.with(|failure| {
            assert!(failure.get().is_none());
            failure.set(Some(point));
        });
        Self
    }
}

impl Drop for Fault {
    fn drop(&mut self) {
        FAILURE.with(|failure| failure.set(None));
    }
}

struct Parent(PathBuf);

impl Parent {
    fn new() -> Self {
        let path = env::temp_dir().join(format!(
            "tooling-stage-owner-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).expect("exclusive fixture parent");
        Self(path)
    }

    fn stage(&self) -> ToolingStage {
        creation::create_root_in(&self.0).expect("retained fixture stage")
    }
}

impl Drop for Parent {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).expect("remove only the fixture-owned retained parent");
    }
}

fn fixture_file() -> CapturedFile {
    let bytes = b"inert ownership fixture\n".to_vec();
    CapturedFile { sha256: Sha256::digest(&bytes).into(), bytes }
}

#[test]
fn pending_leaf_faults_keep_original_authority_and_abort_explicitly() {
    for point in [
        "file-created",
        "file-prefix-written",
        "file-written",
        "file-flushed",
        "file-synced",
        "file-identity",
        "file-metadata",
        "file-readback",
        "file-readonly-retained",
    ] {
        let parent = Parent::new();
        let mut stage = parent.stage();
        let path = stage.physical_path().to_path_buf();
        let _fault = Fault::arm(point);
        creation::stage_file(&mut stage, ROOT, "worker.mjs", &fixture_file())
            .expect_err("injected creation boundary failure");
        let leaf = stage.files.get("worker").expect("original leaf registered before failure");
        assert!(leaf.identity.is_none(), "unsealed leaf cannot enter execution");
        assert!(leaf.original.metadata().expect("retained original metadata").is_file());
        if point == "file-prefix-written" {
            assert_eq!(fs::read(path.join("worker.mjs")).expect("partial bytes"), b"i");
        }
        assert!(stage.revalidate().is_err());
        stage.abort().expect("explicit owned partial cleanup");
        assert!(!path.exists(), "owned stage removed after {point}");
    }
}

#[test]
fn directory_open_gap_retains_uncertain_entry_without_unlink() {
    let parent = Parent::new();
    let mut stage = parent.stage();
    let path = stage.physical_path().to_path_buf();
    let _fault = Fault::arm("directory-created");
    creation::create_directory(&mut stage, ROOT, "node_modules", MODULES)
        .expect_err("failure before original directory open");
    assert!(stage.directories.get(MODULES).expect("pending directory").original.is_none());
    let error = stage.abort().expect_err("uncertain identity is not pathname authority");
    assert!(error.message().contains("cleanup incomplete"));
    assert!(path.join("node_modules").is_dir(), "uncertain entry retained");
}

#[test]
fn retained_directory_open_identity_and_metadata_faults_clean_explicitly() {
    for point in ["directory-opened", "directory-identity", "directory-metadata"] {
        let parent = Parent::new();
        let mut stage = parent.stage();
        let path = stage.physical_path().to_path_buf();
        let _fault = Fault::arm(point);
        creation::create_directory(&mut stage, ROOT, "node_modules", MODULES)
            .expect_err("post-open directory fault");
        assert!(stage.directories.get(MODULES).expect("pending directory").original.is_some());
        stage.abort().expect("cleanup using retained original directory");
        assert!(!path.exists());
    }
}

#[test]
fn root_creation_guard_preserves_primary_and_uncertain_cleanup() {
    let parent = Parent::new();
    let _fault = Fault::arm("root-created");
    let error = creation::create_root_in(&parent.0).expect_err("pre-open root fault");
    assert_eq!(error.code(), "ZRYNA-D3001");
    assert!(error.message().contains("cleanup [ZRYNA-D3001]"));
    assert!(error.message().contains("cleanup incomplete"));
    assert_eq!(fs::read_dir(&parent.0).expect("owned parent inventory").count(), 1);
}

#[test]
fn root_original_is_retained_before_fallible_identity_work() {
    let parent = Parent::new();
    let _fault = Fault::arm("root-opened");
    let error = creation::create_root_in(&parent.0).expect_err("post-open root fault");
    assert!(!error.message().contains("cleanup incomplete"));
    assert_eq!(fs::read_dir(&parent.0).expect("owned parent inventory").count(), 0);
}

#[test]
fn partial_leaf_replacement_and_foreign_sibling_survive_abort() {
    let parent = Parent::new();
    let mut stage = parent.stage();
    let path = stage.physical_path().to_path_buf();
    let _fault = Fault::arm("file-prefix-written");
    creation::stage_file(&mut stage, ROOT, "worker.mjs", &fixture_file())
        .expect_err("partial original retained");
    fs::rename(path.join("worker.mjs"), path.join("retained-original"))
        .expect("move fixture-owned original");
    fs::write(path.join("worker.mjs"), b"foreign replacement").expect("foreign leaf fixture");
    fs::write(path.join("foreign-sibling"), b"foreign sibling").expect("foreign sibling fixture");
    let error = stage.abort().expect_err("replacement cannot be unlinked as owned");
    assert!(error.message().contains("file worker:"));
    assert!(error.message().contains("directory"));
    assert_eq!(
        fs::read(path.join("worker.mjs")).expect("replacement retained"),
        b"foreign replacement"
    );
    assert_eq!(
        fs::read(path.join("foreign-sibling")).expect("sibling retained"),
        b"foreign sibling"
    );
    assert_eq!(fs::read(path.join("retained-original")).expect("original retained"), b"i");
}

#[cfg(unix)]
#[test]
fn parent_replacement_never_authorizes_foreign_directory_cleanup() {
    let parent = Parent::new();
    let mut stage = parent.stage();
    let path = stage.physical_path().to_path_buf();
    creation::create_directory(&mut stage, ROOT, "node_modules", MODULES).expect("owned child");
    fs::rename(path.join("node_modules"), path.join("retained-original"))
        .expect("relocate fixture-owned child");
    fs::create_dir(path.join("node_modules")).expect("foreign replacement directory");
    fs::write(path.join("node_modules/foreign"), b"retain").expect("foreign content");
    stage.abort().expect_err("ancestor mismatch rejects cleanup authority");
    assert_eq!(fs::read(path.join("node_modules/foreign")).expect("retained"), b"retain");
    assert!(path.join("retained-original").is_dir());
}

#[test]
fn invalid_creation_key_rejects_before_filesystem_mutation() {
    creation::create_root_in(std::path::Path::new("."))
        .expect_err("relative stage parent is never an ambient root");
    let parent = Parent::new();
    let mut stage = parent.stage();
    let path = stage.physical_path().to_path_buf();
    creation::stage_file(&mut stage, ROOT, "other.mjs", &fixture_file()).expect_err("unknown leaf");
    creation::create_directory(&mut stage, ROOT, "other", MODULES).expect_err("unknown child");
    assert_eq!(fs::read_dir(&path).expect("unchanged root").count(), 0);
    stage.abort().expect("empty owned root cleanup");
}

#[test]
fn cleanup_failure_does_not_replace_primary_diagnostic() {
    let primary =
        Diagnostic::error("ZRYNA-TEST-PRIMARY", None, "primary failure", "primary guidance");
    let cleanup = Diagnostic::error(
        "ZRYNA-TEST-CLEANUP",
        None,
        "independent cleanup failure",
        "cleanup guidance",
    );
    let combined = with_cleanup(primary, Err(cleanup));
    assert_eq!(combined.code(), "ZRYNA-TEST-PRIMARY");
    assert_eq!(combined.guidance(), "primary guidance");
    assert!(combined.message().contains("primary failure"));
    assert!(combined.message().contains("ZRYNA-TEST-CLEANUP"));
    assert!(combined.message().contains("independent cleanup failure"));
}
