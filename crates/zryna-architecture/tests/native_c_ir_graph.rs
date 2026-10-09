//! Independent static registration/manifest fixtures. No Cargo subprocess is launched here.

use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::PathBuf,
};

type Graph = BTreeMap<String, BTreeSet<String>>;
const MEMBER: &str = "zryna-native-c-ir";

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn registry() -> Value {
    serde_json::from_str(
        &fs::read_to_string(root().join("zryna.workspace.json")).expect("actual registry"),
    )
    .expect("registry JSON")
}
fn graph(document: &Value) -> Graph {
    document["members"]
        .as_array()
        .expect("members")
        .iter()
        .map(|member| {
            (
                member["id"].as_str().expect("member id").into(),
                member["dependencies"]
                    .as_array()
                    .expect("dependencies")
                    .iter()
                    .map(|d| d.as_str().expect("edge").into())
                    .collect(),
            )
        })
        .collect()
}
fn cycle(
    id: &str,
    graph: &Graph,
    visiting: &mut BTreeSet<String>,
    visited: &mut BTreeSet<String>,
) -> bool {
    if visited.contains(id) {
        return false;
    }
    if !visiting.insert(id.into()) {
        return true;
    }
    for edge in graph.get(id).expect("every edge registered") {
        if cycle(edge, graph, visiting, visited) {
            return true;
        }
    }
    visiting.remove(id);
    visited.insert(id.into());
    false
}
fn acyclic(graph: &Graph) -> bool {
    graph.keys().all(|id| !cycle(id, graph, &mut BTreeSet::new(), &mut BTreeSet::new()))
}

#[test]
fn registered_native_c_ir_has_exact_real_manifest_edges_and_compiler_kind() {
    let document = registry();
    let graph = graph(&document);
    let member = document["members"]
        .as_array()
        .expect("members")
        .iter()
        .find(|m| m["id"] == MEMBER)
        .expect("new registered compiler component");
    assert_eq!(member["kind"], "compiler");
    assert_eq!(member["root"], "crates/zryna-native-c-ir");
    let expected: BTreeSet<String> = [
        "zryna-diagnostics",
        "zryna-ir",
        "zryna-layout",
        "zryna-ownership-runtime-abi",
        "zryna-semantics",
        "zryna-source",
        "zryna-syntax",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    assert_eq!(graph[MEMBER], expected);
    let manifest: toml::Value = toml::from_str(
        &fs::read_to_string(root().join("crates/zryna-native-c-ir/Cargo.toml"))
            .expect("actual manifest"),
    )
    .expect("TOML");
    let actual: BTreeSet<String> = manifest["dependencies"]
        .as_table()
        .expect("normal dependencies")
        .keys()
        .filter(|id| id.starts_with("zryna-"))
        .cloned()
        .collect();
    assert_eq!(actual, expected);
    assert!(manifest.get("build-dependencies").is_none());
    assert!(
        manifest["dev-dependencies"]
            .as_table()
            .expect("fixture dependencies")
            .keys()
            .all(|id| !id.starts_with("zryna-"))
    );
    assert!(acyclic(&graph));
}

#[test]
fn base_ir_and_semantics_backedges_are_detected_as_real_graph_cycles() {
    let document = registry();
    let baseline = graph(&document);
    for source in ["zryna-ir", "zryna-semantics"] {
        assert!(!baseline[source].contains(MEMBER));
        let mut hostile = baseline.clone();
        hostile.get_mut(source).expect("registered source").insert(MEMBER.into());
        assert!(!acyclic(&hostile), "{source} backedge must close the actual graph cycle");
    }
}

#[test]
fn cargo_workspace_lock_and_test_directories_register_the_same_component() {
    let document = registry();
    let cargo: toml::Value =
        toml::from_str(&fs::read_to_string(root().join("Cargo.toml")).expect("workspace Cargo"))
            .expect("TOML");
    let members = cargo["workspace"]["members"].as_array().expect("Cargo members");
    assert_eq!(
        members.iter().filter(|m| m.as_str() == Some("crates/zryna-native-c-ir")).count(),
        1
    );
    let lock: toml::Value =
        toml::from_str(&fs::read_to_string(root().join("Cargo.lock")).expect("local lock entry"))
            .expect("lock TOML");
    let packages = lock["package"].as_array().expect("packages");
    let entries =
        packages.iter().filter(|p| p["name"].as_str() == Some(MEMBER)).collect::<Vec<_>>();
    assert_eq!(entries.len(), 1);
    assert!(entries[0].get("source").is_none());
    for id in [MEMBER, "zryna-architecture"] {
        let member = document["members"]
            .as_array()
            .expect("members")
            .iter()
            .find(|m| m["id"] == id)
            .expect("registered test owner");
        assert!(
            member["allowedEntries"]
                .as_array()
                .expect("entries")
                .iter()
                .any(|entry| entry == "tests")
        );
    }
}

#[test]
fn native_c_mir_edge_matches_normal_manifest_and_lock_without_frontend_edges() {
    let document = registry();
    let graph = graph(&document);
    assert!(acyclic(&graph));
    assert!(graph["zryna-native-mir"].contains(MEMBER));
    assert!(!graph[MEMBER].contains("zryna-native-mir"));
    let manifest: toml::Value = toml::from_str(
        &fs::read_to_string(root().join("crates/zryna-native-mir/Cargo.toml"))
            .expect("actual native MIR manifest"),
    )
    .expect("MIR TOML document");
    let expected: BTreeSet<String> = [
        "zryna-abi",
        "zryna-diagnostics",
        "zryna-ir",
        "zryna-layout",
        "zryna-native-c-ir",
        "zryna-ownership-runtime-abi",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    let actual = manifest["dependencies"]
        .as_table()
        .expect("normal dependencies")
        .keys()
        .filter(|name| name.starts_with("zryna-"))
        .cloned()
        .collect::<BTreeSet<_>>();
    assert_eq!(actual, expected);
    assert!(actual.iter().all(|name| graph["zryna-native-mir"].contains(name)));
    let lock: toml::Value =
        toml::from_str(&fs::read_to_string(root().join("Cargo.lock")).expect("actual lock"))
            .expect("lock TOML document");
    let entry = lock["package"]
        .as_array()
        .expect("packages")
        .iter()
        .find(|package| package["name"].as_str() == Some("zryna-native-mir"))
        .expect("MIR local package");
    assert!(entry.get("source").is_none());
    assert_eq!(
        entry["dependencies"]
            .as_array()
            .expect("lock dependencies")
            .iter()
            .filter(|dependency| dependency.as_str() == Some(MEMBER))
            .count(),
        1
    );
}
