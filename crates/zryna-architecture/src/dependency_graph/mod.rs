use crate::contract::MemberContract;
use crate::contract::MemberKind;
use crate::contract::WorkspaceContract;
use crate::diagnostics::ValidationDiagnostics;
use crate::diagnostics::architecture_error;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::path::Path;

pub(super) type InternalDependencyGraph = BTreeMap<String, BTreeSet<String>>;

pub(super) fn validate_dependency_graph(
    contract: &WorkspaceContract,
    resolved_graph: Option<&InternalDependencyGraph>,
    diagnostics: &mut ValidationDiagnostics,
) {
    let members: BTreeMap<&str, &MemberContract> =
        contract.members.iter().map(|member| (member.id.as_str(), member)).collect();
    for member in &contract.members {
        if diagnostics.is_halted() {
            return;
        }
        let mut unique = BTreeSet::new();
        for dependency in &member.dependencies {
            if diagnostics.is_halted() {
                return;
            }
            if !unique.insert(dependency.as_str()) {
                diagnostics.push(architecture_error(
                    "ZRYNA-A1101",
                    Some(Path::new(&member.root)),
                    format!("'{}' declares duplicate dependency '{dependency}'", member.id),
                    "declare each internal component edge exactly once",
                ));
            }
            let Some(target) = members.get(dependency.as_str()) else {
                diagnostics.push(architecture_error(
                    "ZRYNA-A1101",
                    Some(Path::new(&member.root)),
                    format!("'{}' depends on unknown member '{dependency}'", member.id),
                    "register the dependency or remove the edge",
                ));
                continue;
            };
            let _ = target;
        }
    }
    let effective_graph = resolved_graph.cloned().unwrap_or_else(|| {
        contract
            .members
            .iter()
            .map(|member| (member.id.clone(), member.dependencies.iter().cloned().collect()))
            .collect()
    });
    for member in &contract.members {
        if diagnostics.is_halted() {
            return;
        }
        let dependencies = effective_graph.get(&member.id).cloned().unwrap_or_default();
        for dependency in dependencies {
            let Some(target) = members.get(dependency.as_str()) else {
                diagnostics.push(architecture_error(
                    "ZRYNA-A1101",
                    Some(Path::new(&member.root)),
                    format!("resolved graph contains unknown member '{dependency}'"),
                    "register the dependency or remove the edge",
                ));
                continue;
            };
            if !allowed_layer_edge(member.kind, target.kind) {
                diagnostics.push(architecture_error(
                    "ZRYNA-A1102",
                    Some(Path::new(&member.root)),
                    format!("forbidden dependency direction: '{}' -> '{dependency}'", member.id),
                    "depend only toward lower-level contracts or route orchestration through zryna-driver",
                ));
            }
        }
    }
    for member in &contract.members {
        if diagnostics.is_halted() {
            return;
        }
        let mut visiting = BTreeSet::new();
        let mut visited = BTreeSet::new();
        if has_graph_cycle(member.id.as_str(), &effective_graph, &mut visiting, &mut visited) {
            diagnostics.push(architecture_error(
                "ZRYNA-A1103",
                Some(Path::new(&member.root)),
                format!("dependency cycle reaches '{}'", member.id),
                "break the cycle by moving shared contracts into a lower foundation member",
            ));
        }
    }
}
pub(super) const fn allowed_layer_edge(source: MemberKind, target: MemberKind) -> bool {
    use MemberKind::{Application, Backend, Compiler, Foundation, Frontend, Orchestrator};
    match source {
        Foundation | Frontend => matches!(target, Foundation),
        Compiler => matches!(target, Foundation | Compiler),
        Backend => matches!(target, Foundation | Compiler),
        Orchestrator => !matches!(target, Application | Orchestrator),
        Application => matches!(target, Foundation | Orchestrator),
    }
}

fn has_graph_cycle<'a>(
    id: &'a str,
    graph: &'a InternalDependencyGraph,
    visiting: &mut BTreeSet<&'a str>,
    visited: &mut BTreeSet<&'a str>,
) -> bool {
    if visiting.contains(id) {
        return true;
    }
    if visited.contains(id) {
        return false;
    }
    visiting.insert(id);
    if let Some(dependencies) = graph.get(id) {
        for dependency in dependencies {
            if has_graph_cycle(dependency, graph, visiting, visited) {
                return true;
            }
        }
    }
    visiting.remove(id);
    visited.insert(id);
    false
}
