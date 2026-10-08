use serde_json::json;

use super::super::Distribution;
use super::{LEGACY_PROVIDERS, PROVIDERS};

fn fixture(source_ref: &str, providers: &[&str]) -> Distribution {
    let mut files = providers
        .iter()
        .map(|path| (*path, "provider"))
        .chain([
            ("LICENSE", "license"),
            ("NOTICE", "notice"),
            ("README.md", "notice"),
            ("SUPPORT.md", "notice"),
            ("VERSION", "notice"),
            ("licenses/node-LICENSE", "license"),
            ("licenses/typescript-LICENSE.txt", "license"),
            ("licenses/typescript-ThirdPartyNoticeText.txt", "license"),
            ("licenses/typescript6-LICENSE.txt", "license"),
            ("licenses/rust/example-1.0.0/LICENSE", "license"),
            ("metadata/materials.json", "metadata"),
            ("metadata/architecture-receipt.json", "metadata"),
        ])
        .map(|(path, role)| {
            json!({
                "path": path, "size": 1, "sha256": "a".repeat(64), "role": role,
                "mode": 420, "material": "source", "licenses": ["LICENSE"],
            })
        })
        .collect::<Vec<_>>();
    let (triple, format, baseline, node) = if cfg!(windows) {
        (
            "x86_64-pc-windows-msvc",
            "zip",
            json!({
                "os": "windows", "product": "windows-server", "version": "2022",
                "architecture": "x86_64", "runtime": "operating-system-ucrt",
            }),
            "runtime/node/node.exe",
        )
    } else {
        (
            "x86_64-unknown-linux-gnu",
            "tar-gzip",
            json!({
                "os": "linux", "distribution": "ubuntu", "version": "24.04",
                "architecture": "x86_64",
            }),
            "runtime/node/bin/node",
        )
    };
    files.push(json!({"path": node, "size": 1, "sha256": "a".repeat(64),
        "role": "runtime", "mode": 493, "material": "source", "licenses": ["LICENSE"]}));
    files.sort_by(|left, right| left["path"].as_str().cmp(&right["path"].as_str()));
    serde_json::from_value(json!({
        "format": "zryna.distribution.v1", "version": env!("CARGO_PKG_VERSION"),
        "source": {"repository": "https://github.com/zryna/zryna", "ref": source_ref,
            "commit": "a".repeat(40), "tree": "b".repeat(40), "sourceDateEpoch": 0},
        "target": {"triple": triple, "archiveFormat": format, "platformBaseline": baseline},
        "recipe": {"format": "zryna.distribution-recipe.v1", "sha256": "c".repeat(64)},
        "files": files,
    }))
    .expect("syntactic distribution fixture")
}

#[test]
fn complete_manifests_preserve_legacy_and_require_every_main_module() {
    let tagged = format!("refs/tags/v{}", env!("CARGO_PKG_VERSION"));
    fixture(&tagged, &LEGACY_PROVIDERS).validate().expect("legacy manifest");
    assert!(fixture(&tagged, &PROVIDERS).validate().is_err(), "tagged role set stays exact");
    let main = fixture("refs/heads/main", &PROVIDERS);
    main.validate().expect("complete main manifest");
    for path in PROVIDERS.into_iter().filter(|path| !LEGACY_PROVIDERS.contains(path)) {
        let mut incomplete = fixture("refs/heads/main", &PROVIDERS);
        incomplete.files.retain(|file| file.path != path);
        assert!(incomplete.validate().is_err(), "missing {path}");
    }
}

#[test]
fn invalid_identities_and_unknown_module_paths_fail_closed() {
    for source_ref in ["refs/heads/feature", "refs/tags/v0.2.2"] {
        let invalid = fixture(source_ref, &LEGACY_PROVIDERS);
        assert!(invalid.validate().is_err(), "{source_ref}");
        assert!(invalid.provider_paths().is_err(), "{source_ref}");
    }
    let mut unknown = fixture("refs/heads/main", &PROVIDERS);
    let mut extra = unknown
        .files
        .iter()
        .find(|file| file.role == "provider")
        .expect("provider fixture")
        .clone();
    extra.path = "lib/zryna/bootstrap/v4/syntax/unregistered.mjs".to_owned();
    unknown.files.push(extra);
    unknown.files.sort_by(|left, right| left.path.cmp(&right.path));
    assert!(unknown.validate().is_err());
}
