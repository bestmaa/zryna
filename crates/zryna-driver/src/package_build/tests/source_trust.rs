//! Existing source-only denial boundaries; these tests do not prove host isolation or trust modes.

use std::collections::BTreeMap;

use super::*;

// Include empty directories as well as bytes so unexpected stages and cache repairs are visible.
fn tree(root: &Path) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
    fn collect(root: &Path, directory: &Path, entries: &mut BTreeMap<PathBuf, Option<Vec<u8>>>) {
        for entry in fs::read_dir(directory).expect("fixture directory") {
            let entry = entry.expect("fixture entry");
            let path = entry.path();
            let kind = entry.file_type().expect("fixture kind");
            assert!(kind.is_dir() || kind.is_file(), "fixture must be link-free");
            let bytes = kind.is_file().then(|| fs::read(&path).expect("fixture bytes"));
            entries.insert(path.strip_prefix(root).expect("relative path").to_path_buf(), bytes);
            if kind.is_dir() {
                collect(root, &path, entries);
            }
        }
    }
    let mut entries = BTreeMap::new();
    collect(root, root, &mut entries);
    entries
}

fn build_request<'a>(
    resolution: &'a PackageResolutionSuccess,
    cache: &'a ArtifactCacheRoot,
    output: &'a ArtifactOutputRoot,
) -> PackageBuildRequest<'a> {
    PackageBuildRequest {
        resolution,
        configuration: configuration(one_output_for_tests()),
        mode: PackageBuildMode::Frozen,
        cache_root: cache,
        output_root: output,
    }
}

#[test]
fn manifest_execution_extensions_reject_without_changing_existing_state() {
    // These are forbidden extensions of the closed manifest, not proposed recipe formats.
    for field in ["scripts", "nativeRecipe", "buildDependencies", "executionPolicy"] {
        for locator in ["packages/app", "packages/library"] {
            for mode in [PackageLockMode::Update, PackageLockMode::Frozen] {
                let (sources, _resolution) = fixture();
                let (_cache_project, cache, _output_project, output) = roots("manifest-denial");
                fs::write(cache.path().join("retain"), b"existing cache").expect("cache sentinel");
                fs::write(output.path().join("retain"), b"existing output")
                    .expect("output sentinel");
                let path = sources.path().join(locator).join("zryna.package.json");
                let mut manifest: Value =
                    serde_json::from_slice(&fs::read(&path).expect("manifest"))
                        .expect("manifest JSON");
                manifest[field] = json!({"requested": true});
                fs::write(&path, canonical(&manifest)).expect("forbidden extension");
                let before_sources = tree(sources.path());
                let before_cache = tree(cache.path());
                let before_output = tree(output.path());
                let error = resolve_package(&PackageResolutionRequest {
                    source_root: sources.path().to_path_buf(),
                    package: "packages/app".to_owned(),
                    git_cache: None,
                    mode,
                })
                .expect_err("execution extension must not produce a source capability");
                assert_eq!(error.code(), "ZRYNA-P4003", "{field} in {locator}: {mode:?}");
                assert_eq!(tree(sources.path()), before_sources);
                assert_eq!(tree(cache.path()), before_cache);
                assert_eq!(tree(output.path()), before_output);
            }
        }
    }
}

#[test]
fn source_and_adjacent_checksum_substitution_cannot_replace_the_frozen_lock() {
    for locator in ["packages/app", "packages/library"] {
        let (sources, resolution) = fixture();
        let package = sources.path().join(locator);
        let replacement = b"export function substituted(): i32 { return 99; }\n";
        fs::write(package.join("src/main.zry"), replacement).expect("substitute source");
        let manifest_path = package.join("zryna.package.json");
        let mut manifest: Value =
            serde_json::from_slice(&fs::read(&manifest_path).expect("manifest"))
                .expect("manifest JSON");
        manifest["files"][0]["sha256"] = json!(format!("{:x}", Sha256::digest(replacement)));
        manifest["files"][0]["size"] = json!(replacement.len());
        fs::write(&manifest_path, canonical(&manifest)).expect("adjacent checksum substitution");
        let before = tree(sources.path());
        let error = resolve_package(&PackageResolutionRequest {
            source_root: sources.path().to_path_buf(),
            package: "packages/app".to_owned(),
            git_cache: None,
            mode: PackageLockMode::Frozen,
        })
        .expect_err("self-consistent changed source must not override the existing lock");
        assert_eq!(error.code(), "ZRYNA-P4010");
        assert_eq!(tree(sources.path()), before);
        assert_eq!(
            fs::read(resolution.lock_path()).expect("retained lock"),
            resolution.graph().lock_bytes()
        );
    }
}

struct DependencyOutputCompiler {
    calls: Vec<String>,
}

impl PureSourceCompiler for DependencyOutputCompiler {
    fn compile(
        &mut self,
        unit: PackageCompilationUnit<'_>,
    ) -> Result<PackageCompilationResult, PackageBuildError> {
        self.calls.push(unit.package_id.to_owned());
        assert!(!unit.is_root, "root compilation must never be reached");
        Ok(PackageCompilationResult {
            dependency_bytes: vec![],
            outputs: vec![CompiledOutput {
                path: "javascript/app.mjs".to_owned(),
                bytes: b"dependency-supplied root output".to_vec(),
            }],
        })
    }

    fn authenticate_cached_outputs(
        &mut self,
        _authentication: CachedTargetAuthentication<'_>,
    ) -> Result<(), PackageBuildError> {
        panic!("empty cache must not request authentication");
    }
}

#[test]
fn dependency_root_output_is_denied_before_cache_fill_or_publication() {
    let (sources, resolution) = fixture();
    let (_cache_project, cache, _output_project, output) = roots("dependency-output-denial");
    // A Windows cache miss may prepare this empty namespace. Provision it before
    // snapshotting so full-tree equality still rejects every entry and stage write.
    drop(
        cache
            .retained_build_namespace_with_substitution_hook(|| {})
            .expect("empty cache namespace"),
    );
    let before_sources = tree(sources.path());
    let before_cache = tree(cache.path());
    let before_output = tree(output.path());
    let mut compiler = DependencyOutputCompiler { calls: vec![] };
    let error = execute_package_build(&build_request(&resolution, &cache, &output), &mut compiler)
        .expect_err("dependency cannot publish even an exactly declared root output");
    assert_eq!(error.code(), "ZRYNA-B4104");
    assert_eq!(compiler.calls.len(), 1);
    assert_ne!(compiler.calls[0], resolution.graph().root());
    assert_eq!(tree(cache.path()), before_cache);
    assert_eq!(tree(output.path()), before_output);
    assert_eq!(tree(sources.path()), before_sources);
}

#[test]
fn build_uses_authenticated_bytes_after_disk_source_and_manifest_replacement() {
    let (sources, resolution) = fixture();
    let expected = resolution.clone();
    let (_cache_project, cache, _output_project, output) = roots("retained-source");
    let (_clean_cache_project, clean_cache, _clean_project, clean_output) =
        roots("before-replacement");
    let clean = execute_package_build(
        &build_request(&resolution, &clean_cache, &clean_output),
        &mut RecordingCompiler::new(vec!["javascript/app.mjs".to_owned()]),
    )
    .expect("baseline build from authenticated bytes");
    let expected_output = fs::read(
        clean.manifest_path().parent().expect("baseline bundle").join("javascript/app.mjs"),
    )
    .expect("baseline output");
    for locator in ["packages/app", "packages/library"] {
        let package = sources.path().join(locator);
        fs::write(package.join("src/main.zry"), b"replaced source bytes\n")
            .expect("replace source");
        fs::write(package.join("zryna.package.json"), b"invalid replacement manifest\n")
            .expect("replace manifest");
    }
    let before_sources = tree(sources.path());
    let mut compiler = RecordingCompiler::new(vec!["javascript/app.mjs".to_owned()]);
    let built = execute_package_build(&build_request(&resolution, &cache, &output), &mut compiler)
        .expect("build only the immutable authenticated closure");
    assert_eq!(compiler.calls.len(), 2);
    assert_eq!(built.targets()[0].cache, TargetCacheOutcome::Miss);
    assert_eq!(
        fs::read(built.manifest_path().parent().expect("bundle").join("javascript/app.mjs"))
            .expect("published output"),
        expected_output
    );
    assert_eq!(resolution, expected);
    assert_eq!(tree(sources.path()), before_sources);
}

#[test]
fn cache_transplanted_across_policy_identity_rejects_without_repair_or_publication() {
    let (sources, resolution) = fixture();
    let (_cache_project, cache, _cold_project, cold_output) = roots("policy-transplant");
    let cold = execute_package_build(
        &build_request(&resolution, &cache, &cold_output),
        &mut RecordingCompiler::new(vec!["javascript/app.mjs".to_owned()]),
    )
    .expect("cold entry");
    let next_project = TemporaryRoot::new("policy-transplant-output");
    let next_output =
        ArtifactOutputRoot::prepare_for_workspace(next_project.path()).expect("output");
    let mut request = build_request(&resolution, &cache, &next_output);
    // Opaque cache identity only: no executable mode or isolation claim is inferred here.
    request.configuration.execution_policy.configuration_sha256 = ZERO.to_owned();
    let prepared = plan::prepare(&request).expect("changed identity plan");
    assert_ne!(prepared.identity.cache_key(), cold.plan().cache_key());
    let old_key = &cold.targets()[0].target_cache_key;
    let new_key = prepared.identity.target_cache_key(&BuildTargetId::JavaScript).expect("new key");
    let namespace = cache.path().join("build-plan-v0");
    let transplanted = namespace.join(new_key);
    fs::create_dir(&transplanted).expect("transplanted root");
    for (path, bytes) in tree(&namespace.join(old_key)) {
        let destination = transplanted.join(path);
        if let Some(bytes) = bytes {
            fs::write(destination, bytes).expect("transplanted bytes");
        } else {
            fs::create_dir(destination).expect("transplanted directory");
        }
    }
    let before_sources = tree(sources.path());
    let before_cache = tree(cache.path());
    let before_cold_output = tree(cold_output.path());
    let before_output = tree(next_output.path());
    let mut compiler = RecordingCompiler::new(vec!["javascript/app.mjs".to_owned()]);
    let error = execute_package_build(&request, &mut compiler).expect_err("old-policy metadata");
    assert_eq!(error.code(), "ZRYNA-B4102");
    assert!(compiler.calls.is_empty());
    assert_eq!(compiler.cache_authentications, 0);
    assert_eq!(tree(cache.path()), before_cache);
    assert_eq!(tree(next_output.path()), before_output);
    assert_eq!(tree(cold_output.path()), before_cold_output);
    assert_eq!(tree(sources.path()), before_sources);
}
