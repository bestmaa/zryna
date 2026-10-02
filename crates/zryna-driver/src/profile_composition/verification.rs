use std::collections::{BTreeMap, BTreeSet};

use zryna_diagnostics::Diagnostic;

use super::{
    FORBIDDEN, INVALID, PROFILE, RESOURCE, Report, UNSUPPORTED, ValidatedComposition,
    authority::Authorities,
    error,
    graph::{self, Graph},
    model::{Claim, Input, Requirement, Summary},
    policy::Policy,
    quota,
};

pub(super) fn verify(
    input: &Input,
    authorities: &Authorities,
    claim: &Claim,
) -> Result<ValidatedComposition, Vec<Diagnostic>> {
    let graph = graph::validate(input)?;
    let authorities = authorities.binding(&graph.ids())?;
    authorities.validate_command_requirements(&graph.input)?;
    let policy = Policy::load()?;
    select(&graph, &policy, &authorities)?;
    profiles(&graph, &authorities)?;
    let (summaries, witnesses) = derive(&graph, &authorities)?;
    restrictions(&graph, &policy, &summaries, &witnesses)?;
    if claim.binding != graph.binding(&authorities)?
        || claim.summaries != summaries
        || claim.witnesses != witnesses
    {
        return Err(vec![error(
            INVALID,
            "stale binding or forged transitive summary, witness or quota",
        )]);
    }
    Ok(ValidatedComposition { input: graph.input, authorities, summaries, witnesses })
}

fn select(
    graph: &Graph,
    policy: &Policy,
    authorities: &super::authority::Binding,
) -> Result<(), Vec<Diagnostic>> {
    let mut targets = BTreeSet::new();
    let mut report = Report::default();
    for selection in &graph.input.selections {
        if !targets.insert(Policy::target(selection.row)) {
            return Err(vec![error(INVALID, "duplicate selected output target")]);
        }
        if let Err(diagnostic) = policy.selection(selection)
            && !report.push(diagnostic)
        {
            return report.finish();
        }
        if let Some(world) = &selection.world {
            let Some(audit) = authorities.wit() else {
                return Err(vec![error(UNSUPPORTED, "WIT selection lacks sealed world audit")]);
            };
            let Some(resolved) = audit.worlds().iter().find(|item| item.identity() == world) else {
                return Err(vec![error(
                    UNSUPPORTED,
                    "selected WIT world lacks exact audit authority",
                )]);
            };
            if selection
                .approved
                .iter()
                .any(|requirement| !resolved.resolved_imports().contains(&requirement.interface))
            {
                return Err(vec![error(
                    UNSUPPORTED,
                    "approved interface is absent from the audited WIT world",
                )]);
            }
        }
        for node in &graph.input.instances {
            let incompatible = !node.rows.contains(&selection.row)
                || node.requirements.iter().any(|requirement| {
                    !policy.known(requirement)
                        || (Policy::ceiling_allows(selection.row, requirement.capability)
                            && !policy.admits(selection.row, requirement))
                });
            if incompatible
                && !report.push(error(
                    UNSUPPORTED,
                    format!(
                        "row {:?}, policy {}: instance {} has an incompatible target/interface",
                        selection.row, selection.policy_version, node.id
                    ),
                ))
            {
                return report.finish();
            }
        }
    }
    report.finish()
}

fn profiles(graph: &Graph, authorities: &super::authority::Binding) -> Result<(), Vec<Diagnostic>> {
    let mut report = Report::default();
    for node in &graph.input.instances {
        if !authorities.has_language(&node.id, graph.input.language)
            && !report.push(error(
                PROFILE,
                format!(
                    "selected {:?}, instance {} lacks matching sealed program authority",
                    graph.input.language, node.id
                ),
            ))
        {
            return report.finish();
        }
    }
    report.finish()
}

type Derived = (BTreeMap<String, Summary>, BTreeMap<Requirement, Vec<String>>);

// Independently traverse each closure. No producer summary, quota, or claimed path enters this walk.
fn derive(
    graph: &Graph,
    authorities: &super::authority::Binding,
) -> Result<Derived, Vec<Diagnostic>> {
    let mut summaries = BTreeMap::new();
    let mut witnesses: BTreeMap<Requirement, Vec<String>> = BTreeMap::new();
    let root_paths = graph.paths(&graph.input.root);
    for node in &graph.input.instances {
        let paths = graph.paths(&node.id);
        let closure: Vec<_> =
            graph.input.instances.iter().filter(|child| paths.contains_key(&child.id)).collect();
        let requirements =
            closure.iter().flat_map(|child| child.requirements.iter().cloned()).collect();
        let mut quota = quota::aggregate(closure.iter().copied())?;
        authorities.command_quota(closure.iter().map(|node| node.id.as_str()), &mut quota)?;
        summaries.insert(node.id.clone(), Summary { requirements, quota });
        for requirement in &node.requirements {
            let path = &root_paths[&node.id];
            let replace =
                witnesses.get(requirement).is_none_or(|old| (path.len(), path) < (old.len(), old));
            if replace {
                witnesses.insert(requirement.clone(), path.clone());
            }
        }
    }
    Ok((summaries, witnesses))
}

fn restrictions(
    graph: &Graph,
    policy: &Policy,
    summaries: &BTreeMap<String, Summary>,
    witnesses: &BTreeMap<Requirement, Vec<String>>,
) -> Result<(), Vec<Diagnostic>> {
    let mut report = Report::default();
    for selection in &graph.input.selections {
        for node in &graph.input.instances {
            for requirement in &summaries[&node.id].requirements {
                let denied = !node.restrictions.contains(&requirement.capability)
                    || !Policy::ceiling_allows(selection.row, requirement.capability)
                    || !selection.approved.contains(requirement);
                if denied {
                    // A restriction witness must pass through the rejecting node, not a different branch.
                    let witness = restriction_witness(graph, &node.id, requirement);
                    if !report.push(error(FORBIDDEN, format!(
                        "root {}, row {:?}, policy {}, rejecting instance {}, capability {} interface {}: witness {}",
                        graph.input.root, selection.row, selection.policy_version, node.id,
                        requirement.capability.name(), requirement.interface, witness.join(" -> ")))) {
                        return report.finish();
                    }
                }
            }
        }
        let maximum = policy.limits(selection.row);
        for (metric, amount) in summaries[&graph.input.root].quota.iter().enumerate() {
            let limit = maximum[metric].min(selection.ceilings[metric]);
            if *amount > limit && !report.push(error(RESOURCE, format!(
                "root {}, row {:?}, policy {}: quota metric {} limit {} exceeded at {}; observed {}",
                graph.input.root, selection.row, selection.policy_version, metric, limit, limit + 1, amount))) {
                return report.finish();
            }
        }
    }
    // The maps are retained only after all restrictions and quotas pass.
    debug_assert!(
        witnesses.keys().all(|key| summaries[&graph.input.root].requirements.contains(key))
    );
    report.finish()
}

fn restriction_witness(graph: &Graph, rejecting: &str, requirement: &Requirement) -> Vec<String> {
    let root_paths = graph.paths(&graph.input.root);
    let local_paths = graph.paths(rejecting);
    let tail = graph
        .input
        .instances
        .iter()
        .filter(|node| node.requirements.contains(requirement))
        .filter_map(|node| local_paths.get(&node.id))
        .min_by_key(|path| (path.len(), *path))
        .expect("transitive requirement has a reachable declaration");
    let mut path = root_paths[rejecting].clone();
    path.extend(tail.iter().skip(1).cloned());
    path
}
