//! Bounded test-only acquisition of exact pinned worker responses.

use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

pub fn responses(version: u32, texts: &[String]) -> Vec<Value> {
    request_responses(
        version,
        &texts
            .iter()
            .map(|text| {
                json!({
                    "schema_version": version, "files": [{"path": "src/main.zry", "text": text}],
                })
            })
            .collect::<Vec<_>>(),
    )
}

pub fn project_response(version: u32, files: &[(String, String)]) -> Value {
    request_responses(version, &[json!({
        "schema_version": version,
        "files": files.iter().map(|(path, text)| json!({"path": path, "text": text})).collect::<Vec<_>>(),
    })]).remove(0)
}

fn request_responses(version: u32, params: &[Value]) -> Vec<Value> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let node = std::env::var_os("ZRYNA_TEST_NODE").unwrap_or_else(|| "node".into());
    let worker =
        if version == 2 { "worker.mjs".to_owned() } else { format!("worker-v{version}.mjs") };
    let mut child = Command::new(node)
        .arg(format!("src/{worker}"))
        .current_dir(root.join("adapters/typescript-6"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("pinned worker");
    let requests = params
        .iter()
        .enumerate()
        .map(|(index, params)| {
            json!({"id": index + 1, "method": "analyze", "params": params}).to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    assert!(requests.len() < 4 * 1024 * 1024, "bounded test request set");
    let mut stdin = child.stdin.take().expect("stdin");
    let stdout = child.stdout.take().expect("stdout");
    let stderr = child.stderr.take().expect("stderr");
    let writer = thread::spawn(move || stdin.write_all(requests.as_bytes()));
    let output_limit = if version == 2 {
        zryna_syntax::v2::MAX_RESPONSE_BYTES
    } else {
        zryna_syntax::v4::MAX_RESPONSE_BYTES
    };
    let output = thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.take(output_limit as u64 + 1).read_to_end(&mut bytes).expect("stdout read");
        bytes
    });
    let errors = thread::spawn(move || {
        let mut bytes = Vec::new();
        stderr.take(4097).read_to_end(&mut bytes).expect("stderr read");
        bytes
    });
    let deadline = Instant::now() + Duration::from_secs(30);
    let status = loop {
        if let Some(status) = child.try_wait().expect("worker wait") {
            break status;
        }
        if Instant::now() >= deadline {
            child.kill().expect("bounded worker termination");
            child.wait().expect("worker reaped");
            panic!("pinned worker exceeded the test deadline");
        }
        thread::sleep(Duration::from_millis(10));
    };
    writer.join().expect("writer thread").expect("worker requests");
    let output = output.join().expect("stdout thread");
    let errors = errors.join().expect("stderr thread");
    assert!(status.success(), "worker exit: {status}");
    assert!(errors.is_empty(), "worker stderr: {}", String::from_utf8_lossy(&errors));
    assert!(output.len() <= output_limit, "bounded worker output");
    let responses = output
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice::<Value>(line).expect("worker JSON"))
        .collect::<Vec<_>>();
    assert_eq!(responses.len(), params.len(), "complete worker response set");
    responses
        .into_iter()
        .enumerate()
        .map(|(index, mut response)| {
            assert_eq!(response["id"], json!(index + 1), "request correlation");
            response.as_object_mut().expect("response object").remove("id");
            if let Some(error) = response.get_mut("error") {
                let message = error["message"].as_str().expect("worker diagnostic").to_owned();
                if let Some((prefix, location)) = message.split_once(" at file ") {
                    let (file, location) = location.split_once(" bytes ").expect("file location");
                    let (start, end) = location.split_once("..").expect("range");
                    let digits = end.bytes().take_while(u8::is_ascii_digit).count();
                    error["span"] = json!({"file": file.parse::<u32>().expect("file"), "start": start.parse::<u32>().expect("start"),
                    "end": end[..digits].parse::<u32>().expect("end")});
                    error["message"] = json!(format!("{prefix}{}", &end[digits..]));
                } else {
                    error["span"] = Value::Null;
                }
            }
            response
        })
        .collect()
}
