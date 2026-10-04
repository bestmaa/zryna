//! A bounded native candidate worker for the existing v2/v3/v4 process handshake.

use std::{
    error::Error,
    fs,
    io::{self, BufRead, Read, Write},
    path::Path,
};

use serde::Deserialize;
use serde_json::{Value, json};
use zryna_frontend::{AnalyzeRequest, MAX_WORKER_REQUEST_BYTES, native_lexer, native_parser};
use zryna_source::{SourceFileInput, SourceMap};

pub const PROVIDER: &str = "zryna-native-activation-harness";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    id: u32,
    method: String,
    #[serde(default)]
    params: Option<AnalyzeRequest>,
}

fn request(reader: &mut impl BufRead) -> Result<Option<Request>> {
    let mut bytes = Vec::new();
    reader.take((MAX_WORKER_REQUEST_BYTES + 1) as u64).read_until(b'\n', &mut bytes)?;
    if bytes.is_empty() {
        return Ok(None);
    }
    if bytes.len() > MAX_WORKER_REQUEST_BYTES || bytes.last() != Some(&b'\n') {
        return Err("request framing/budget rejected".into());
    }
    Ok(Some(serde_json::from_slice(&bytes)?))
}

fn respond(value: &Value) -> Result<()> {
    let mut stdout = io::stdout().lock();
    serde_json::to_writer(&mut stdout, value)?;
    stdout.write_all(b"\n")?;
    stdout.flush()?;
    Ok(())
}

fn candidate(protocol: u32, request: AnalyzeRequest) -> Result<Value> {
    if request.schema_version != protocol {
        return Err("request protocol rejected".into());
    }
    let sources = SourceMap::build(
        request
            .files
            .into_iter()
            .map(|file| SourceFileInput { path: file.path, text: file.text })
            .collect(),
    )
    .map_err(|_| "source map rejected")?;
    let lexed = native_lexer::lex(&sources)?;
    Ok(match protocol {
        2 => serde_json::to_value(native_parser::parse_v2_recovering_candidate(&sources, &lexed)?)?,
        3 => serde_json::to_value(native_parser::v3::parse_v3_straight_line_candidate(
            &sources, &lexed,
        )?)?,
        4 => serde_json::to_value(native_parser::v4::parse_v4_candidate(&sources, &lexed)?)?,
        _ => return Err("unsupported exact protocol".into()),
    })
}

pub fn run(args: &[String]) -> Result<()> {
    if args.len() != 3 {
        return Err("worker requires protocol, fault, marker".into());
    }
    let protocol: u32 = args[0].parse()?;
    if !(2..=4).contains(&protocol) {
        return Err("unsupported protocol".into());
    }
    let fault = args[1].as_str();
    let mut stdin = io::stdin().lock();
    let first = request(&mut stdin)?.ok_or("missing handshake")?;
    if first.id != 1 || first.method != "handshake" || first.params.is_some() {
        return Err("exact handshake required first".into());
    }
    let mut caps = json!({"module_resolution": false, "semantic_diagnostics": false});
    if protocol >= 3 {
        caps["control_flow_v1"] = json!(true);
    }
    if protocol == 4 {
        caps["data_ownership_syntax_v1"] = json!(true);
    }
    let mut info = json!({"provider": PROVIDER, "provider_version": VERSION,
        "protocol_version": protocol, "capabilities": caps});
    match fault {
        "identity" => info["provider"] = json!("typescript-6"),
        "version" => info["provider_version"] = json!("stale"),
        "protocol" => info["protocol_version"] = json!(1),
        "module" => info["capabilities"]["module_resolution"] = json!(true),
        "semantic" => info["capabilities"]["semantic_diagnostics"] = json!(true),
        "control" => info["capabilities"]["control_flow_v1"] = json!(false),
        "ownership" => info["capabilities"]["data_ownership_syntax_v1"] = json!(false),
        "extra" => info["ambient_root"] = json!("/"),
        _ => {}
    }
    respond(&json!({"id": if fault == "id" { 99 } else { 1 }, "result": info}))?;
    // The parent must never send source text after any failed handshake.
    let Some(second) = request(&mut stdin)? else {
        return Ok(());
    };
    fs::write(Path::new(&args[2]), b"analysis received")?;
    if second.id != 2 || second.method != "analyze" {
        return Err("exact analysis required second".into());
    }
    let result = candidate(protocol, second.params.ok_or("missing analysis params")?);
    let mut raw = match result {
        Ok(raw) => raw,
        Err(error) => {
            return respond(&json!({"id": 2, "error": {
                "code": "ZRYNA-F1502", "message": error.to_string(),
            }}));
        }
    };
    match fault {
        "snapshot-version" => raw["schema_version"] = json!(1),
        "snapshot-path" => raw["files"][0]["path"] = json!("../foreign.zry"),
        _ => {}
    }
    respond(&json!({"id": 2, "result": raw}))?;
    if fault == "frame" {
        respond(&json!({"id": 3, "result": raw}))?;
    }
    if request(&mut stdin)?.is_some() {
        return Err("extra request rejected".into());
    }
    Ok(())
}
