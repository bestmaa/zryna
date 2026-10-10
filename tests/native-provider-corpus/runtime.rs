use crate::{Context, compare::Artifacts, corpus};
use serde_json::{Value, json};
use std::{
    fs,
    io::Read,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
use zryna_abi::ScalarValue;

pub fn scalar(value: &Value) -> ScalarValue {
    match value["type"].as_str().expect("typed scalar") {
        "i32" => {
            ScalarValue::I32(i32::try_from(value["value"].as_i64().expect("integer")).expect("i32"))
        }
        "bool" => ScalarValue::Bool(value["value"].as_bool().expect("boolean")),
        _ => panic!("unsupported frozen scalar type"),
    }
}
pub fn typed_arguments(arguments: &Value) -> Vec<ScalarValue> {
    arguments.as_array().expect("typed arguments").iter().map(scalar).collect()
}
pub fn m3_arguments(arguments: &Value) -> Vec<ScalarValue> {
    arguments
        .as_array()
        .expect("M3 arguments")
        .iter()
        .map(|value| {
            ScalarValue::I32(
                i32::try_from(value.as_i64().expect("M3 integer argument")).expect("i32"),
            )
        })
        .collect()
}

pub fn portable_pair(
    context: &Context,
    label: &str,
    artifacts: [&Artifacts; 2],
    export: &str,
    arguments: &[ScalarValue],
    expected: &Value,
    ownership: bool,
) -> Value {
    let root = corpus::install(context, label, &[]);
    let mut observations = vec![];
    for (provider, artifacts) in ["bootstrap", "native"].into_iter().zip(artifacts) {
        fs::write(root.join(format!("{provider}.mjs")), artifacts.javascript.as_bytes())
            .expect("exact emitted JS bytes");
        fs::write(root.join(format!("{provider}.wasm")), &artifacts.wasm)
            .expect("exact emitted Wasm bytes");
        for target in ["javascript", "webassembly"] {
            let program = harness(provider, target, export, arguments, expected, ownership);
            let script = format!("{provider}-{target}-run.mjs");
            fs::write(root.join(&script), program.as_bytes())
                .expect("isolated scalar observation harness");
            let output = node(context, &root, &script);
            let value: Value =
                serde_json::from_slice(&output).expect("actual typed scalar observation");
            assert_eq!(&value, expected, "{label} {provider} {target} frozen scalar outcome");
            observations.push(
                json!({"provider":provider,"target":target,"status":"executed","outcome":value}),
            );
        }
    }
    json!(observations)
}

fn harness(
    provider: &str,
    target: &str,
    export: &str,
    arguments: &[ScalarValue],
    expected: &Value,
    ownership: bool,
) -> String {
    let export = serde_json::to_string(export).expect("quoted scalar export");
    let arguments = arguments
        .iter()
        .map(|value| match value {
            ScalarValue::I32(value) => value.to_string(),
            ScalarValue::Bool(value) if target == "webassembly" => u8::from(*value).to_string(),
            ScalarValue::Bool(value) => value.to_string(),
        })
        .collect::<Vec<_>>()
        .join(",");
    let load = if target == "javascript" {
        format!("const exports = await import('./{provider}.mjs');\n")
    } else {
        format!(
            "import {{readFileSync}} from 'node:fs';\nconst {{instance}} = await WebAssembly.instantiate(readFileSync('./{provider}.wasm'), {{}});\nconst exports = instance.exports;\n"
        )
    };
    let invoke = format!("let value = exports[{export}]({arguments});\n");
    let observe = if ownership {
        "const status = exports.$zryna$observation(0);\n"
    } else {
        "const status = 0;\n"
    };
    let validate = match expected["type"].as_str().expect("expected result type") {
        "i32" => {
            "if(status!==0 || typeof value!=='number' || !Number.isInteger(value) || value!==(value|0) || Object.is(value,-0)) process.exit(70);\nprocess.stdout.write(JSON.stringify({type:'i32',value}));\n"
        }
        "bool" if target == "webassembly" => {
            "if(status!==0 || (value!==0 && value!==1)) process.exit(70);\nprocess.stdout.write(JSON.stringify({type:'bool',value:value===1}));\n"
        }
        "bool" => {
            "if(status!==0 || typeof value!=='boolean') process.exit(70);\nprocess.stdout.write(JSON.stringify({type:'bool',value}));\n"
        }
        "trap" => {
            "if(status!==1 || value!==0) process.exit(70);\nprocess.stdout.write(JSON.stringify({type:'trap',value:'zryna.trap.bounds-v1'}));\n"
        }
        _ => panic!("unsupported frozen result"),
    };
    format!("{load}{invoke}{observe}{validate}")
}

fn node(context: &Context, root: &std::path::Path, script: &str) -> Vec<u8> {
    let mut child = Command::new(&context.node)
        .arg(script)
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("pinned Node observation");
    let stdout = child.stdout.take().expect("stdout");
    let stderr = child.stderr.take().expect("stderr");
    let output = thread::spawn(move || {
        let mut bytes = vec![];
        stdout.take(65_537).read_to_end(&mut bytes).expect("bounded scalar stdout");
        bytes
    });
    let errors = thread::spawn(move || {
        let mut bytes = vec![];
        stderr.take(16_385).read_to_end(&mut bytes).expect("bounded scalar stderr");
        bytes
    });
    let deadline = Instant::now() + Duration::from_secs(15);
    let status = loop {
        if let Some(status) = child.try_wait().expect("observation child status") {
            break status;
        }
        if Instant::now() >= deadline {
            child.kill().expect("terminate bounded observation");
            child.wait().expect("reap observation child");
            panic!("scalar observation exceeded 15 second deadline");
        }
        thread::sleep(Duration::from_millis(10));
    };
    let output = output.join().expect("stdout reader");
    let errors = errors.join().expect("stderr reader");
    assert!(status.success(), "observation status {status}: {}", String::from_utf8_lossy(&errors));
    assert!(errors.is_empty() && output.len() <= 65_536, "bounded clean observation frame");
    output
}
