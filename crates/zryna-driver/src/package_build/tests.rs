mod graph;
mod hostile;
mod invalidation;
mod source_trust;

use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};

use super::*;
use crate::{PackageLockMode, PackageResolutionRequest, resolve_package};

static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);
const ZERO: &str = "0000000000000000000000000000000000000000000000000000000000000000";
const COMPILER: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
const PROFILE: &str = "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";
const RUNTIME: &str = "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";
const POLICY: &str = "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";

struct TemporaryRoot(PathBuf);

impl TemporaryRoot {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "zryna-build-{label}-{}-{}",
            std::process::id(),
            NEXT_ROOT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).expect("temporary root");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TemporaryRoot {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).expect("temporary root cleanup");
    }
}

fn canonical(value: &Value) -> Vec<u8> {
    let mut bytes = serde_json::to_vec(value).expect("canonical JSON");
    bytes.push(b'\n');
    bytes
}

fn source(locator: &str) -> Value {
    json!({ "kind": "local", "locator": locator, "revision": "" })
}

fn write_package(root: &Path, locator: &str, name: &str, body: &[u8], dependencies: &[Value]) {
    let directory = root.join(locator);
    fs::create_dir_all(directory.join("src")).expect("package directory");
    fs::write(directory.join("src/main.zry"), body).expect("source");
    let manifest = json!({
        "compatibility": {
            "compiler": "0.1.0",
            "profile": "control-flow-v1",
            "targets": ["javascript"]
        },
        "dependencies": dependencies,
        "files": [{
            "path": "src/main.zry",
            "sha256": format!("{:x}", Sha256::digest(body)),
            "size": body.len()
        }],
        "format": "zryna.package.v1",
        "name": name,
        "source": source(locator),
        "version": "1.0.0"
    });
    fs::write(directory.join("zryna.package.json"), canonical(&manifest)).expect("manifest");
}

fn resolved(root: &TemporaryRoot, mode: PackageLockMode) -> PackageResolutionSuccess {
    resolve_package(&PackageResolutionRequest {
        source_root: root.path().to_path_buf(),
        package: "packages/app".to_owned(),
        git_cache: None,
        mode,
    })
    .expect("resolved package graph")
}

fn fixture() -> (TemporaryRoot, PackageResolutionSuccess) {
    let root = TemporaryRoot::new("sources");
    write_package(
        root.path(),
        "packages/app",
        "app",
        b"export function main(): i32 { return 1; }\n",
        &[json!({
            "alias": "math",
            "name": "library",
            "source": source("packages/library"),
            "version": "1.0.0"
        })],
    );
    write_package(
        root.path(),
        "packages/library",
        "library",
        b"export function value(): i32 { return 1; }\n",
        &[],
    );
    resolved(&root, PackageLockMode::Update);
    let frozen = resolved(&root, PackageLockMode::Frozen);
    (root, frozen)
}

fn configuration(outputs: Vec<BuildOutput>) -> BuildPlanConfiguration {
    let targets = vec![BuildTarget {
        id: BuildTargetId::JavaScript,
        triple: "ecmascript2023-unknown-unknown".to_owned(),
        abi: "zryna-js-scalar-v1".to_owned(),
        features: vec![],
        runtime: BuildRuntimeIdentity {
            name: "zryna-js-runtime".to_owned(),
            version: "1".to_owned(),
            sha256: RUNTIME.to_owned(),
        },
        composition: BuildCompositionIdentity {
            contract: "zryna.cross-target-profiles.v1".to_owned(),
            row: "U-JS".to_owned(),
            host_policy_sha256: ZERO.to_owned(),
            approved_request_sha256: ZERO.to_owned(),
        },
    }];
    BuildPlanConfiguration {
        compiler: BuildCompilerIdentity {
            tool: "zryna".to_owned(),
            version: "0.1.0".to_owned(),
            sha256: COMPILER.to_owned(),
            protocol: "syntax-v3".to_owned(),
        },
        profile: BuildProfileIdentity {
            id: "control-flow-v1".to_owned(),
            configuration_sha256: PROFILE.to_owned(),
        },
        execution_policy: ExecutionPolicyIdentity {
            id: "source-build-policy".to_owned(),
            version: "0".to_owned(),
            configuration_sha256: POLICY.to_owned(),
        },
        host: BuildHostIdentity {
            triple: "x86_64-unknown-linux-gnu".to_owned(),
            environment: vec![
                BuildEnvironmentEntry { name: "LC_ALL".to_owned(), value: "C".to_owned() },
                BuildEnvironmentEntry {
                    name: "SOURCE_DATE_EPOCH".to_owned(),
                    value: "0".to_owned(),
                },
                BuildEnvironmentEntry { name: "TZ".to_owned(), value: "UTC".to_owned() },
            ],
        },
        targets: targets.clone(),
        host_tools: vec![BuildHostToolIdentity {
            name: "zryna".to_owned(),
            version: "0.1.0".to_owned(),
            sha256: COMPILER.to_owned(),
            runs_on: "x86_64-unknown-linux-gnu".to_owned(),
            targets: vec![BuildTargetId::JavaScript],
        }],
        outputs,
    }
}

struct RecordingCompiler {
    calls: Vec<String>,
    output_paths: Vec<String>,
    authenticated_cache: Option<Vec<CompiledOutput>>,
    cache_authentications: usize,
    fail: bool,
}

impl RecordingCompiler {
    fn new(output_paths: Vec<String>) -> Self {
        Self {
            calls: vec![],
            output_paths,
            authenticated_cache: None,
            cache_authentications: 0,
            fail: false,
        }
    }

    fn with_authenticated_cache(mut self, outputs: Vec<CompiledOutput>) -> Self {
        self.authenticated_cache = Some(outputs);
        self
    }
}

impl PureSourceCompiler for RecordingCompiler {
    fn compile(
        &mut self,
        unit: PackageCompilationUnit<'_>,
    ) -> Result<PackageCompilationResult, PackageBuildError> {
        self.calls.push(unit.package_id.to_owned());
        if self.fail {
            return Err(PackageBuildError::compiler("injected compiler failure"));
        }
        let mut digest = Sha256::new();
        digest.update(unit.package_id.as_bytes());
        for source in unit.sources {
            digest.update(source.path().as_bytes());
            digest.update(source.bytes());
        }
        for dependency in unit.dependencies {
            digest.update(dependency.package_id.as_bytes());
            digest.update(dependency.bytes);
        }
        let dependency_bytes = digest.finalize().to_vec();
        let outputs = if unit.is_root {
            self.output_paths
                .iter()
                .map(|path| CompiledOutput {
                    path: path.clone(),
                    bytes: [unit.target.id.as_str().as_bytes(), &dependency_bytes].concat(),
                })
                .collect()
        } else {
            vec![]
        };
        Ok(PackageCompilationResult { dependency_bytes, outputs })
    }

    fn authenticate_cached_outputs(
        &mut self,
        authentication: CachedTargetAuthentication<'_>,
    ) -> Result<(), PackageBuildError> {
        self.cache_authentications += 1;
        if self.authenticated_cache.as_deref() == Some(authentication.outputs) {
            Ok(())
        } else {
            Err(PackageBuildError::compiler(
                "cached outputs lack the test compiler's trusted result authority",
            ))
        }
    }
}

fn roots(label: &str) -> (TemporaryRoot, ArtifactCacheRoot, TemporaryRoot, ArtifactOutputRoot) {
    let cache_project = TemporaryRoot::new(&format!("{label}-cache"));
    let cache = ArtifactCacheRoot::prepare_for_project(cache_project.path()).expect("cache");
    let output_project = TemporaryRoot::new(&format!("{label}-output"));
    let output = ArtifactOutputRoot::prepare_for_workspace(output_project.path()).expect("output");
    (cache_project, cache, output_project, output)
}

fn plan_key(
    resolution: &PackageResolutionSuccess,
    configuration: BuildPlanConfiguration,
    cache: &ArtifactCacheRoot,
    output: &ArtifactOutputRoot,
) -> Result<String, PackageBuildError> {
    plan::prepare(&PackageBuildRequest {
        resolution,
        configuration,
        mode: PackageBuildMode::Frozen,
        cache_root: cache,
        output_root: output,
    })
    .map(|prepared| prepared.identity.cache_key().to_owned())
}

#[test]
fn isolated_cold_and_warm_builds_share_plan_and_artifact_bytes() {
    let (_sources, resolution) = fixture();
    let (_cache_project, cache, _first_project, first_output) = roots("replay");
    let config = configuration(vec![BuildOutput {
        path: "javascript/app.mjs".to_owned(),
        target: BuildTargetId::JavaScript,
    }]);
    let mut cold_compiler = RecordingCompiler::new(vec!["javascript/app.mjs".to_owned()]);
    let cold = execute_package_build(
        &PackageBuildRequest {
            resolution: &resolution,
            configuration: config.clone(),
            mode: PackageBuildMode::Frozen,
            cache_root: &cache,
            output_root: &first_output,
        },
        &mut cold_compiler,
    )
    .expect("cold build");
    assert_eq!(cold.targets()[0].cache, TargetCacheOutcome::Miss);
    assert_eq!(cold_compiler.calls.last().map(String::as_str), Some(resolution.graph().root()));
    let cold_manifest: Value =
        serde_json::from_slice(&fs::read(cold.manifest_path()).expect("test fixture"))
            .expect("test fixture");
    assert_eq!(
        cold_manifest["plan"]["cacheKey"],
        Value::String(cold.plan().cache_key().to_owned())
    );
    assert_eq!(
        cold_manifest["targets"][0]["targetCacheKey"],
        Value::String(cold.targets()[0].target_cache_key.clone())
    );

    let cold_artifact =
        cold.manifest_path().parent().expect("test fixture").join("javascript/app.mjs");
    let cold_bytes = fs::read(&cold_artifact).expect("test fixture");
    let second_project = TemporaryRoot::new("replay-output-two");
    let second_output =
        ArtifactOutputRoot::prepare_for_workspace(second_project.path()).expect("output");
    let mut warm_compiler = RecordingCompiler::new(vec!["javascript/app.mjs".to_owned()])
        .with_authenticated_cache(vec![CompiledOutput {
            path: "javascript/app.mjs".to_owned(),
            bytes: cold_bytes.clone(),
        }]);
    let warm = execute_package_build(
        &PackageBuildRequest {
            resolution: &resolution,
            configuration: config.clone(),
            mode: PackageBuildMode::Frozen,
            cache_root: &cache,
            output_root: &second_output,
        },
        &mut warm_compiler,
    )
    .expect("warm build");
    assert!(warm_compiler.calls.is_empty());
    assert_eq!(warm_compiler.cache_authentications, 1);
    assert_eq!(warm.targets()[0].cache, TargetCacheOutcome::Hit);
    assert_eq!(cold.plan(), warm.plan());
    let warm_artifact =
        warm.manifest_path().parent().expect("test fixture").join("javascript/app.mjs");
    assert_eq!(cold_bytes, fs::read(warm_artifact).expect("test fixture"));

    let (_isolated_cache_project, isolated_cache, _isolated_project, isolated_output) =
        roots("replay-isolated");
    let mut isolated_compiler = RecordingCompiler::new(vec!["javascript/app.mjs".to_owned()]);
    let isolated = execute_package_build(
        &PackageBuildRequest {
            resolution: &resolution,
            configuration: config,
            mode: PackageBuildMode::Frozen,
            cache_root: &isolated_cache,
            output_root: &isolated_output,
        },
        &mut isolated_compiler,
    )
    .expect("isolated cold build");
    assert_eq!(isolated.targets()[0].cache, TargetCacheOutcome::Miss);
    assert_eq!(cold.plan(), isolated.plan());
    let isolated_artifact =
        isolated.manifest_path().parent().expect("test fixture").join("javascript/app.mjs");
    assert_eq!(
        fs::read(cold.manifest_path().parent().expect("test fixture").join("javascript/app.mjs"))
            .expect("test fixture"),
        fs::read(isolated_artifact).expect("test fixture")
    );
}

#[test]
fn every_plan_identity_change_selects_a_new_cache_key() {
    let (_sources, resolution) = fixture();
    let (_cache_project, cache, _first_project, first_output) = roots("identity");
    let outputs = vec![BuildOutput {
        path: "javascript/app.mjs".to_owned(),
        target: BuildTargetId::JavaScript,
    }];
    let mut compiler = RecordingCompiler::new(vec!["javascript/app.mjs".to_owned()]);
    let first = execute_package_build(
        &PackageBuildRequest {
            resolution: &resolution,
            configuration: configuration(outputs.clone()),
            mode: PackageBuildMode::Frozen,
            cache_root: &cache,
            output_root: &first_output,
        },
        &mut compiler,
    )
    .expect("test fixture");
    let second_project = TemporaryRoot::new("identity-output-two");
    let second_output =
        ArtifactOutputRoot::prepare_for_workspace(second_project.path()).expect("test fixture");
    let mut changed = configuration(outputs);
    changed.execution_policy.configuration_sha256 = ZERO.to_owned();
    let second = execute_package_build(
        &PackageBuildRequest {
            resolution: &resolution,
            configuration: changed,
            mode: PackageBuildMode::Frozen,
            cache_root: &cache,
            output_root: &second_output,
        },
        &mut compiler,
    )
    .expect("test fixture");
    assert_ne!(first.plan().cache_key(), second.plan().cache_key());
    assert_eq!(second.targets()[0].cache, TargetCacheOutcome::Miss);
}

#[test]
fn compiler_profile_grant_runtime_and_environment_each_invalidate_the_plan() {
    let (_sources, resolution) = fixture();
    let (_cache_project, cache, _output_project, output) = roots("dimensions");
    let outputs = one_output_for_tests();
    let baseline = plan_key(&resolution, configuration(outputs.clone()), &cache, &output)
        .expect("test fixture");
    let mut changes = Vec::new();

    let mut compiler = configuration(outputs.clone());
    compiler.compiler.sha256 = ZERO.to_owned();
    compiler.host_tools[0].sha256 = ZERO.to_owned();
    changes.push(compiler);
    let mut profile = configuration(outputs.clone());
    profile.profile.configuration_sha256 = ZERO.to_owned();
    changes.push(profile);
    let mut policy = configuration(outputs.clone());
    policy.execution_policy.configuration_sha256 = ZERO.to_owned();
    changes.push(policy);
    let mut environment = configuration(outputs.clone());
    environment.host.environment[1].value = "1".to_owned();
    changes.push(environment);
    let mut runtime = configuration(outputs.clone());
    runtime.targets[0].runtime.sha256 = ZERO.to_owned();
    changes.push(runtime);
    let mut grant = configuration(outputs);
    grant.targets[0].composition.approved_request_sha256 = COMPILER.to_owned();
    changes.push(grant);

    for changed in changes {
        let key = plan_key(&resolution, changed, &cache, &output).expect("valid changed plan");
        assert_ne!(key, baseline);
    }
}

fn one_output_for_tests() -> Vec<BuildOutput> {
    vec![BuildOutput { path: "javascript/app.mjs".to_owned(), target: BuildTargetId::JavaScript }]
}

#[test]
fn plan_and_target_keys_match_independent_domain_hashes() {
    let (_sources, resolution) = fixture();
    let (_cache_project, cache, _output_project, output) = roots("oracle");
    let prepared = plan::prepare(&PackageBuildRequest {
        resolution: &resolution,
        configuration: configuration(one_output_for_tests()),
        mode: PackageBuildMode::Frozen,
        cache_root: &cache,
        output_root: &output,
    })
    .expect("test fixture");
    let mut value: Value = serde_json::from_slice(prepared.identity.bytes()).expect("test fixture");
    let cache_key =
        value.as_object_mut().expect("test fixture").remove("cacheKey").expect("test fixture");
    let mut projection = serde_json::to_vec(&value).expect("test fixture");
    projection.push(b'\n');
    let mut plan_hash = Sha256::new();
    plan_hash.update(b"ZRYNA-RESOLVED-BUILD-PLAN-V0\0plan\0");
    plan_hash.update(&projection);
    assert_eq!(cache_key, Value::String(format!("{:x}", plan_hash.finalize())));

    let mut target = serde_json::to_vec(&json!({
        "planKey": prepared.identity.cache_key(),
        "target": "javascript"
    }))
    .expect("test fixture");
    target.push(b'\n');
    let mut target_hash = Sha256::new();
    target_hash.update(b"ZRYNA-RESOLVED-BUILD-PLAN-V0\0target\0");
    target_hash.update(target);
    let expected_target = format!("{:x}", target_hash.finalize());
    assert_eq!(
        prepared.identity.target_cache_key(&BuildTargetId::JavaScript),
        Some(expected_target.as_str())
    );
}

#[test]
fn exact_output_bound_is_accepted_and_first_extra_rejects() {
    let (_sources, resolution) = fixture();
    let (_cache_project, cache, _output_project, output) = roots("bounds");
    let paths = (0..16).map(|index| format!("javascript/out-{index:02}.mjs")).collect::<Vec<_>>();
    let outputs = paths
        .iter()
        .map(|path| BuildOutput { path: path.clone(), target: BuildTargetId::JavaScript })
        .collect::<Vec<_>>();
    let mut compiler = RecordingCompiler::new(paths);
    execute_package_build(
        &PackageBuildRequest {
            resolution: &resolution,
            configuration: configuration(outputs.clone()),
            mode: PackageBuildMode::Frozen,
            cache_root: &cache,
            output_root: &output,
        },
        &mut compiler,
    )
    .expect("exact output bound");

    let extra_project = TemporaryRoot::new("bounds-extra");
    let extra_output =
        ArtifactOutputRoot::prepare_for_workspace(extra_project.path()).expect("test fixture");
    let mut extra = outputs;
    extra.push(BuildOutput {
        path: "javascript/out-16.mjs".to_owned(),
        target: BuildTargetId::JavaScript,
    });
    let error = execute_package_build(
        &PackageBuildRequest {
            resolution: &resolution,
            configuration: configuration(extra),
            mode: PackageBuildMode::Frozen,
            cache_root: &cache,
            output_root: &extra_output,
        },
        &mut RecordingCompiler::new(vec![]),
    )
    .expect_err("first extra output");
    assert_eq!(error.code(), "ZRYNA-B4101");
}
