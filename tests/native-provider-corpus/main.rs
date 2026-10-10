//! Independent downstream comparisons through genuine providers and existing verifier APIs.
#![forbid(unsafe_code)]

mod closures;
mod compare;
mod corpus;
mod m1;
mod m2;
mod m3;
mod providers;
mod runtime;

use serde_json::{Value, json};
use std::{collections::BTreeSet, fs, panic::AssertUnwindSafe, path::PathBuf};

pub struct Context {
    root: PathBuf,
    node: PathBuf,
    output: PathBuf,
    cases: Vec<Value>,
    ids: BTreeSet<String>,
}

impl Context {
    fn case(
        &mut self,
        id: String,
        profile: &str,
        source: &str,
        check: impl FnOnce(&Self) -> Value,
    ) {
        assert!(self.ids.insert(id.clone()), "unique case identity: {id}");
        eprintln!("checking {id}");
        let checked = std::panic::catch_unwind(AssertUnwindSafe(|| check(self)));
        let record = match checked {
            Ok(details) => json!({
                "id": id, "profile": profile, "source": source, "status": "pass",
                "syntax": details["syntax"], "semantic": details["semantic"],
                "artifacts": details["artifacts"], "runtime": details["runtime"],
                "details": details,
            }),
            Err(error) => {
                let error = error
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| error.downcast_ref::<&str>().map(|text| (*text).to_owned()))
                    .unwrap_or_else(|| "non-string assertion failure".to_owned());
                json!({"id":id,"profile":profile,"source":source,"status":"fail",
                    "syntax":"incomplete","semantic":"incomplete","artifacts":"incomplete",
                    "runtime":"incomplete","details":{"error":error.chars().take(8192).collect::<String>()}})
            }
        };
        self.cases.push(record);
    }

    fn blocked(&mut self, id: &str, profile: &str, reason: &str) {
        assert!(self.ids.insert(id.to_owned()));
        self.cases.push(json!({"id":id,"profile":profile,"source":"",
            "status":"blocked","syntax":"not-applicable","semantic":"not-applicable",
            "artifacts":"not-applicable","runtime":"blocked","details":{"reason":reason}}));
    }
}

fn main() {
    std::panic::set_hook(Box::new(|information| {
        eprintln!("{}", information.to_string().chars().take(8192).collect::<String>());
    }));
    let args = std::env::args_os().skip(1).map(PathBuf::from).collect::<Vec<_>>();
    assert_eq!(args.len(), 3, "usage: corpus ROOT NODE OUTPUT_ROOT");
    assert!(args.iter().all(|path| path.is_absolute()), "absolute input paths");
    let [root, node, output]: [PathBuf; 3] = args.try_into().expect("three paths");
    assert!(root.is_dir() && node.is_file());
    assert!(!output.starts_with(&root), "runtime must be outside the source checkout");
    assert!(!output.exists(), "create-only owned runtime directory");
    fs::create_dir(&output).expect("owned runtime directory");
    let mut context = Context { root, node, output, cases: vec![], ids: BTreeSet::new() };
    m1::run(&mut context);
    m2::run(&mut context);
    m3::run(&mut context);
    context.blocked("production-manifest-parity", "m1-m3", "this corpus has not exercised exhaustive production-manifest parity; defaults select the bootstrap worker, and the separate feature-gated CLI smoke provides only bounded build evidence");
    context.blocked(
        "m2-native-execution",
        "m2",
        "verified ControlFlowV1 native invocation preparation is crate-private",
    );
    context.blocked("m3-native-fault-observation", "m3", "canonical fault runner and typed cleanup trace decoder are private; public run_native_invocation accepts no fault selection");
    context.blocked("ordinary-no-node-cli", "m1-m3", "this comparison intentionally uses the pinned bootstrap worker; ordinary native CLI activation is a separate acceptance obligation");
    let unavailable = context
        .cases
        .iter()
        .filter(|case| case["details"]["native"]["status"] == "platform-unavailable")
        .map(|case| {
            (
                case["id"].as_str().expect("runtime ID").to_owned(),
                case["profile"].as_str().expect("runtime profile").to_owned(),
            )
        })
        .collect::<Vec<_>>();
    for (id, profile) in unavailable {
        context.blocked(&format!("native-platform:{id}"), &profile,
            "portable provider observations executed; actual native linking and execution require Linux x86-64");
    }
    let disposition = |status: &str| {
        context
            .cases
            .iter()
            .filter(|case| case["status"] == status)
            .map(|case| case["id"].clone())
            .collect::<Vec<_>>()
    };
    let failed = disposition("fail");
    let receipt = json!({"schema_version":1,"public_activation":false,
        "passed":disposition("pass"),"failed":failed,"ignored":[],"blocked":disposition("blocked"),
        "cases":context.cases,"runtime_root":context.output,
        "limits":{"frozen_parser_428":"existing independent suite, not executed here",
            "native_execution":"Linux x86-64 only","source_inventory":"M1 registry, all 14 M2 files, all 95 M3 files, all 25 contextual M3 registry fixtures"}});
    println!("{}", serde_json::to_string(&receipt).expect("typed receipt"));
    if !receipt["failed"].as_array().expect("failed IDs").is_empty() {
        std::process::exit(1);
    }
}
