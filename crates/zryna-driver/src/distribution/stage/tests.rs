//! Unit fixtures exercise stage copying and retention, not initial archive authentication.

use super::super::manifest::providers::{LEGACY_PROVIDERS, PROVIDERS};
use super::{InstallationTree, InstalledCompiler, NEXT_STAGE, ProviderStage};
use serde_json::json;
use sha2::{Digest as _, Sha256};
use std::{fs, path::PathBuf};

struct Case {
    root: PathBuf,
    compiler: Option<InstalledCompiler>,
}

impl Case {
    fn new() -> Self {
        Self::with_providers("refs/heads/main", &PROVIDERS)
    }

    fn with_providers(source_ref: &str, providers: &[&str]) -> Self {
        let root = loop {
            let path = std::env::temp_dir().join(format!(
                "zryna-provider-fixture-{}-{}",
                std::process::id(),
                NEXT_STAGE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            match fs::create_dir(&path) {
                Ok(()) => break path,
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => panic!("fixture root: {error}"),
            }
        };
        let mut files = Vec::new();
        for &path in providers {
            let destination = root.join(path);
            fs::create_dir_all(destination.parent().expect("provider parent"))
                .expect("provider directories");
            let data = format!("// immutable test bytes for {path}\n").into_bytes();
            fs::write(destination, &data).expect("provider file");
            files.push(json!({"path": path, "size": data.len(),
                "sha256": format!("{:x}", Sha256::digest(&data)), "role": "provider", "mode": 420,
                "material": "source", "licenses": ["LICENSE"]}));
        }
        let mut tree = InstallationTree::capture(&root).expect("fixture root capture");
        for &path in providers {
            tree.capture_file(path, 1024, true).expect("fixture file capture");
        }
        let distribution = serde_json::from_value(json!({
            "format": "zryna.distribution.v1", "version": env!("CARGO_PKG_VERSION"),
            "source": {"repository": "https://github.com/zryna/zryna", "ref": source_ref,
                "commit": "a".repeat(40), "tree": "b".repeat(40), "sourceDateEpoch": 0},
            "target": {"triple": "x86_64-unknown-linux-gnu", "archiveFormat": "tar-gzip",
                "platformBaseline": {"os": "linux", "distribution": "ubuntu", "version": "24.04", "architecture": "x86_64"}},
            "recipe": {"format": "zryna.distribution-recipe.v1", "sha256": "c".repeat(64)},
            "files": files,
        })).expect("private stage fixture fields");
        Self { root: root.clone(), compiler: Some(InstalledCompiler { root, tree, distribution }) }
    }

    fn compiler(&self) -> &InstalledCompiler {
        self.compiler.as_ref().expect("live fixture")
    }
}

impl Drop for Case {
    fn drop(&mut self) {
        self.compiler.take();
        fs::remove_dir_all(&self.root).expect("remove exclusively created fixture");
    }
}

#[test]
fn stage_contains_exactly_registered_provider_files_and_cleans_up() {
    let case = Case::new();
    let stage = ProviderStage::create(case.compiler()).expect("provider stage");
    assert_eq!(stage.files.len(), PROVIDERS.len());
    for path in PROVIDERS {
        let relative = path.strip_prefix("lib/zryna/bootstrap/").expect("provider prefix");
        assert_eq!(
            fs::read(stage.path.join(relative)).expect("stage bytes"),
            case.compiler().tree.bytes(path).expect("original bytes")
        );
    }
    assert!(stage.worker("../worker.mjs").is_err());
    assert!(stage.worker("limits-v3.mjs").is_err());
    stage.revalidate().expect("complete stage inventory");
    let path = stage.path.clone();
    drop(stage);
    assert!(!path.exists(), "ordinary verified stage cleanup");
}

#[test]
fn each_registered_staged_file_rejects_mutation_or_prevents_the_write() {
    let case = Case::new();
    for path in PROVIDERS {
        let stage = ProviderStage::create(case.compiler()).expect("provider stage");
        let relative = path.strip_prefix("lib/zryna/bootstrap/").expect("provider prefix");
        let data = case.compiler().tree.bytes(path).expect("source bytes");
        let changed = vec![b'x'; data.len()];
        match fs::write(stage.path.join(relative), changed) {
            Ok(()) => assert!(stage.revalidate().is_err(), "{path}"),
            Err(_) => stage.revalidate().expect("write prevented"),
        }
        let retained_path = stage.path.clone();
        drop(stage);
        if retained_path.exists() {
            fs::remove_dir_all(retained_path).expect("remove owned changed fixture stage");
        }
    }
}

#[test]
fn an_added_stage_module_is_rejected_and_never_removed_by_failed_admission() {
    let case = Case::new();
    let stage = ProviderStage::create(case.compiler()).expect("provider stage");
    let path = stage.path.clone();
    fs::write(path.join("unexpected.mjs"), b"export {};\n").expect("hostile extra module");
    assert!(stage.revalidate().is_err());
    drop(stage);
    assert!(
        path.join("unexpected.mjs").exists(),
        "failed admission does not delete unknown entries"
    );
    fs::remove_dir_all(path).expect("remove owned changed fixture stage");
}

#[test]
fn tagged_release_keeps_the_original_nine_file_stage() {
    let source_ref = format!("refs/tags/v{}", env!("CARGO_PKG_VERSION"));
    let case = Case::with_providers(&source_ref, &LEGACY_PROVIDERS);
    let stage = ProviderStage::create(case.compiler()).expect("legacy provider stage");
    assert_eq!(stage.files.len(), 9);
    assert!(!stage.path.join("v4").exists());
    for path in LEGACY_PROVIDERS {
        let relative = path.strip_prefix("lib/zryna/bootstrap/").expect("provider prefix");
        assert_eq!(
            fs::read(stage.path.join(relative)).expect("legacy bytes"),
            case.compiler().tree.bytes(path).expect("original legacy bytes")
        );
    }
    stage.revalidate().expect("legacy stage inventory");
    let path = stage.path.clone();
    drop(stage);
    assert!(!path.exists(), "legacy stage cleanup");
}

#[test]
fn unknown_or_mismatched_source_refs_never_select_a_stage() {
    for source_ref in ["refs/heads/feature", "refs/tags/v0.2.2"] {
        let case = Case::with_providers(source_ref, &LEGACY_PROVIDERS);
        assert!(ProviderStage::create(case.compiler()).is_err(), "{source_ref}");
    }
}
