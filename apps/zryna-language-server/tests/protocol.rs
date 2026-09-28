//! Protocol coverage for revision, validation, and lifecycle behavior.

use std::{
    io::{self, Write},
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

use serde_json::{Value, json};
use zryna_driver::diagnostic_sessions::{
    DiagnosticRevision, DiagnosticSession, DiagnosticSessionError, ToolingCompiler,
    admit_single_function_fixture,
};
use zryna_language_server::{
    MAX_OUTSTANDING_REQUESTS, Outgoing, RevisionCompiler, Server, read_frame, write_frame,
};
use zryna_source::{NormalizedSourcePath, SourceMap};

struct EmptyCompiler;

struct SemanticCompiler;

struct ControlFlowCompiler;

impl RevisionCompiler for EmptyCompiler {
    type Error = DiagnosticSessionError;

    fn admit(
        &mut self,
        session: &mut DiagnosticSession,
        sources: SourceMap,
    ) -> Result<DiagnosticRevision, Self::Error> {
        session.admit_diagnostics(sources, &[])
    }

    fn admit_control_flow(
        &mut self,
        session: &mut DiagnosticSession,
        sources: SourceMap,
        _entrypoint: &NormalizedSourcePath,
    ) -> Result<DiagnosticRevision, Self::Error> {
        session.admit_diagnostics(sources, &[])
    }
}

impl RevisionCompiler for SemanticCompiler {
    type Error = DiagnosticSessionError;

    fn admit(
        &mut self,
        session: &mut DiagnosticSession,
        sources: SourceMap,
    ) -> Result<DiagnosticRevision, Self::Error> {
        admit_single_function_fixture(session, sources)
    }

    fn admit_control_flow(
        &mut self,
        session: &mut DiagnosticSession,
        sources: SourceMap,
        _entrypoint: &NormalizedSourcePath,
    ) -> Result<DiagnosticRevision, Self::Error> {
        admit_single_function_fixture(session, sources)
    }
}

impl RevisionCompiler for ControlFlowCompiler {
    type Error = DiagnosticSessionError;

    fn admit(
        &mut self,
        _session: &mut DiagnosticSession,
        _sources: SourceMap,
    ) -> Result<DiagnosticRevision, Self::Error> {
        panic!("scalar admission selected for M2")
    }

    fn admit_control_flow(
        &mut self,
        session: &mut DiagnosticSession,
        sources: SourceMap,
        entrypoint: &NormalizedSourcePath,
    ) -> Result<DiagnosticRevision, Self::Error> {
        assert_eq!(entrypoint.as_str(), "src/main.zry");
        session.admit_diagnostics(sources, &[])
    }
}

fn request(server: &mut Server<impl RevisionCompiler>, value: impl serde::Serialize) -> Vec<Value> {
    let bytes = serde_json::to_vec(&value).unwrap_or_else(|error| panic!("fixture: {error}"));
    let outgoing = server.handle_bytes(&bytes);
    emit(server, outgoing)
}

fn queued(
    server: &mut Server<impl RevisionCompiler>,
    value: impl serde::Serialize,
) -> Vec<Outgoing> {
    let bytes = serde_json::to_vec(&value).unwrap_or_else(|error| panic!("fixture: {error}"));
    server.handle_bytes(&bytes)
}

fn raw_request(server: &mut Server<impl RevisionCompiler>, bytes: &[u8]) -> Vec<Value> {
    let outgoing = server.handle_bytes(bytes);
    emit(server, outgoing)
}

fn emit(server: &mut Server<impl RevisionCompiler>, outgoing: Vec<Outgoing>) -> Vec<Value> {
    outgoing
        .into_iter()
        .map(|message| {
            let value = message.value().clone();
            server
                .write_outgoing(&mut Vec::new(), message)
                .unwrap_or_else(|error| panic!("emit fixture: {error}"));
            value
        })
        .collect()
}

fn initialized() -> Server<EmptyCompiler> {
    let mut server = Server::new(EmptyCompiler).unwrap_or_else(|error| panic!("session: {error}"));
    let output = request(
        &mut server,
        json!({
            "jsonrpc":"2.0","id":1,"method":"initialize","params":{
                "rootUri":"file:///workspace",
                "capabilities":{"general":{"positionEncodings":["utf-8","utf-16"]}}
            }
        }),
    );
    assert_eq!(output.len(), 1);
    assert_eq!(output[0]["result"]["capabilities"]["positionEncoding"], "utf-8");
    assert!(
        request(&mut server, json!({"jsonrpc":"2.0","method":"initialized","params":{}}))
            .is_empty()
    );
    server
}

fn semantic_initialized() -> Server<SemanticCompiler> {
    let mut server =
        Server::new(SemanticCompiler).unwrap_or_else(|error| panic!("session: {error}"));
    let output = request(
        &mut server,
        json!({
            "jsonrpc":"2.0","id":1,"method":"initialize","params":{
                "rootUri":"file:///workspace",
                "capabilities":{"general":{"positionEncodings":["utf-8"]}}
            }
        }),
    );
    assert_eq!(output.len(), 1);
    let _ = request(&mut server, json!({"jsonrpc":"2.0","method":"initialized","params":{}}));
    server
}

fn control_flow_initialized() -> Server<ControlFlowCompiler> {
    let mut server = Server::new(ControlFlowCompiler).expect("session");
    let output = request(
        &mut server,
        json!({
            "jsonrpc":"2.0","id":1,"method":"initialize","params":{
                "rootUri":"file:///workspace",
                "capabilities":{"general":{"positionEncodings":["utf-16"]}},
                "initializationOptions":{"zrynaProfile":"control-flow-v1"}
            }
        }),
    );
    assert_eq!(
        output[0]["result"]["capabilities"]["experimental"]["zrynaAnalysisProfile"],
        "control-flow-v1"
    );
    assert_eq!(
        output[0]["result"]["capabilities"]["experimental"]["zrynaFormattingProfile"],
        "control-flow-format-v1"
    );
    assert_eq!(output[0]["result"]["capabilities"]["definitionProvider"], false);
    server
}

#[test]
fn explicit_m2_selects_entrypoint_and_rejects_unavailable_definition() {
    let mut server = control_flow_initialized();
    let published =
        open(&mut server, 1, "export function f(): i32 { let x: i32 = 1; return x; }\n");
    assert_eq!(published[0]["method"], "zryna/publishDiagnostics");
    assert_eq!(published[1]["params"]["version"], 1);
    assert_eq!(definition(&mut server, 2, 0, 46)[0]["error"]["code"], -32601);
    let changed =
        change(&mut server, 2, "export function f(): i32 { const x: i32 = 2; return x; }\n");
    assert_eq!(changed[0]["params"]["revision"], 2);
}

#[test]
fn invalid_profile_cannot_initialize_or_admit_source() {
    let mut server = Server::new(EmptyCompiler).expect("session");
    for options in [
        json!({"zrynaProfile":"unsupported-v1"}),
        json!({"zrynaProfile":"control-flow-v1","other":true}),
        json!(1),
    ] {
        let rejected = request(
            &mut server,
            json!({"jsonrpc":"2.0","id":1,"method":"initialize",
            "params":{"rootUri":"file:///workspace","capabilities":{},"initializationOptions":options}}),
        );
        assert_eq!(rejected[0]["error"]["code"], -32602);
    }
    let unopened = open(&mut server, 1, "export function f(): i32 { return 1; }\n");
    assert!(unopened.is_empty());
}

static NEXT_M3_WORKSPACE: AtomicU64 = AtomicU64::new(0);

#[test]
#[allow(clippy::too_many_lines)]
fn m3_saved_import_and_open_entry_format_then_reject_changed_dependency() {
    let workspace = std::env::temp_dir().join(format!(
        "zryna-lsp-m3-{}-{}",
        std::process::id(),
        NEXT_M3_WORKSPACE.fetch_add(1, Ordering::Relaxed),
    ));
    std::fs::create_dir(&workspace).expect("workspace");
    std::fs::write(workspace.join("main.zry"), "invalid saved entry").expect("saved entry");
    std::fs::write(
        workspace.join("math.zry"),
        "export function double(value: i32): i32 { return value + value; }\n",
    )
    .expect("saved import");
    let root_uri = if cfg!(windows) {
        let mut uri = format!("file:///{}", workspace.to_string_lossy().replace('\\', "/"));
        let drive = uri.as_bytes()[8];
        let alternate = if drive.is_ascii_uppercase() {
            drive.to_ascii_lowercase()
        } else {
            drive.to_ascii_uppercase()
        };
        uri.replace_range(8..9, &char::from(alternate).to_string());
        uri
    } else {
        format!("file://{}", workspace.to_string_lossy())
    };
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = Command::new("node").args(["-p", "process.execPath"]).output().expect("node path");
    assert!(output.status.success());
    let node = PathBuf::from(String::from_utf8(output.stdout).expect("UTF-8").trim());
    let compiler =
        ToolingCompiler::discover(&repository.canonicalize().expect("repository"), &node)
            .expect("pinned compiler");
    let mut server = Server::new(compiler).expect("server");
    server.configure_workspace_root(&workspace).expect("trusted workspace root");
    let rejected = request(
        &mut server,
        json!({"jsonrpc":"2.0","id":0,"method":"initialize",
        "params":{"rootUri":"file:///different","capabilities":{},
        "initializationOptions":{"zrynaProfile":"data-ownership-v1"}}}),
    );
    assert_eq!(rejected[0]["error"]["code"], -32602);
    let initialized = request(
        &mut server,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize",
        "params":{"rootUri":root_uri,"capabilities":{},
        "initializationOptions":{"zrynaProfile":"data-ownership-v1"}}}),
    );
    assert_eq!(
        initialized[0]["result"]["capabilities"]["experimental"]["zrynaFormattingProfile"],
        "data-ownership-format-v1"
    );
    let uri = format!("{root_uri}/main.zry");
    let opened = request(
        &mut server,
        json!({"jsonrpc":"2.0","method":"textDocument/didOpen",
        "params":{"textDocument":{"uri":uri,"languageId":"zryna","version":1,
        "text":"import{double}from\"./math.zry\"; export function score(value:i32):i32{return double(value);}"}}}),
    );
    assert_eq!(opened[0]["method"], "zryna/publishDiagnostics");
    assert_eq!(opened[0]["params"]["report"]["diagnostics"], json!([]));
    assert_eq!(opened[1]["params"]["version"], 1);

    let queued_output = queued(
        &mut server,
        json!({"jsonrpc":"2.0","id":7,
        "method":"textDocument/formatting","params":{"textDocument":{"uri":uri},
        "options":{"tabSize":2,"insertSpaces":true}}}),
    );
    assert!(queued_output.is_empty());
    let pending = server.finish_pending();
    let formatted = emit(&mut server, pending);
    assert_eq!(formatted.len(), 1);
    assert!(
        formatted[0]["result"][0]["newText"]
            .as_str()
            .expect("text")
            .contains("return double(value);")
    );

    let math_uri = format!("{root_uri}/math.zry");
    let opened_dependency = request(
        &mut server,
        json!({"jsonrpc":"2.0",
        "method":"textDocument/didOpen","params":{"textDocument":{
        "uri":math_uri,"languageId":"zryna","version":1,
        "text":"export function double(value:i32):i32{return value+value+1;}"}}}),
    );
    assert_eq!(opened_dependency[0]["params"]["documents"].as_array().expect("documents").len(), 2);
    assert_eq!(opened_dependency[0]["params"]["report"]["diagnostics"], json!([]));
    let queued_output = queued(
        &mut server,
        json!({"jsonrpc":"2.0","id":10,
        "method":"textDocument/formatting","params":{"textDocument":{"uri":math_uri},
        "options":{"tabSize":2,"insertSpaces":true}}}),
    );
    assert!(queued_output.is_empty());
    let pending = server.finish_pending();
    let dependency_edit = emit(&mut server, pending);
    assert!(
        dependency_edit[0]["result"][0]["newText"]
            .as_str()
            .expect("text")
            .contains("value + value + 1")
    );
    let closed_dependency = request(
        &mut server,
        json!({"jsonrpc":"2.0",
        "method":"textDocument/didClose","params":{"textDocument":{"uri":math_uri}}}),
    );
    assert_eq!(closed_dependency[0]["params"]["uri"], math_uri);

    let queued_output = queued(
        &mut server,
        json!({"jsonrpc":"2.0","id":11,
        "method":"textDocument/formatting","params":{"textDocument":{"uri":uri},
        "options":{"tabSize":2,"insertSpaces":true}}}),
    );
    assert!(queued_output.is_empty());
    let prepared = server.finish_pending().pop().expect("prepared formatting");
    assert!(prepared.value().get("result").is_some());

    std::fs::write(
        workspace.join("math.zry"),
        "export function double(value: i32): i32 { return value * 2; }\n",
    )
    .expect("changed dependency");
    let mut bytes = Vec::new();
    server.write_outgoing(&mut bytes, prepared).expect("emission");
    let frame = read_frame(&mut io::Cursor::new(bytes)).expect("frame").expect("response");
    let emitted: Value = serde_json::from_slice(&frame).expect("JSON");
    assert_eq!(emitted["error"]["data"]["code"], "ZRYNA-D4002");
    let queued_output = queued(
        &mut server,
        json!({"jsonrpc":"2.0","id":8,
        "method":"textDocument/formatting","params":{"textDocument":{"uri":uri},
        "options":{"tabSize":2,"insertSpaces":true}}}),
    );
    assert!(queued_output.is_empty());
    let pending = server.finish_pending();
    let stale = emit(&mut server, pending);
    assert_eq!(stale[0]["error"]["data"]["code"], "ZRYNA-D4002");

    let rejected = request(
        &mut server,
        json!({"jsonrpc":"2.0","method":"textDocument/didChange",
        "params":{"textDocument":{"uri":uri,"version":2},
        "contentChanges":[{"text":"export function score(): i32 {"}]}}),
    );
    assert_eq!(rejected[0]["method"], "zryna/publishDiagnostics");
    assert_ne!(rejected[0]["params"]["report"]["diagnostics"], json!([]));
    let queued_output = queued(
        &mut server,
        json!({"jsonrpc":"2.0","id":9,
        "method":"textDocument/formatting","params":{"textDocument":{"uri":uri},
        "options":{"tabSize":2,"insertSpaces":true}}}),
    );
    assert!(queued_output.is_empty());
    let pending = server.finish_pending();
    let unavailable = emit(&mut server, pending);
    assert_eq!(unavailable[0]["error"]["data"]["code"], "ZRYNA-D4001");
    drop(server);
    std::fs::remove_dir_all(workspace).expect("workspace cleanup");
}

fn open<Compiler: RevisionCompiler>(
    server: &mut Server<Compiler>,
    version: i64,
    text: &str,
) -> Vec<Value> {
    request(
        server,
        json!({
            "jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{
                "uri":"file:///workspace/src/main.zry","languageId":"zryna","version":version,"text":text
            }}
        }),
    )
}

fn change<Compiler: RevisionCompiler>(
    server: &mut Server<Compiler>,
    version: i64,
    text: &str,
) -> Vec<Value> {
    request(
        server,
        json!({
            "jsonrpc":"2.0","method":"textDocument/didChange","params":{
                "textDocument":{"uri":"file:///workspace/src/main.zry","version":version},
                "contentChanges":[{"text":text}]
            }
        }),
    )
}

fn definition<Compiler: RevisionCompiler>(
    server: &mut Server<Compiler>,
    id: i64,
    line: u32,
    character: u32,
) -> Vec<Value> {
    request(
        server,
        json!({
            "jsonrpc":"2.0","id":id,"method":"textDocument/definition","params":{
                "textDocument":{"uri":"file:///workspace/src/main.zry"},
                "position":{"line":line,"character":character}
            }
        }),
    )
}

#[test]
fn framing_is_exact_bounded_and_recovers_between_frames() {
    let first = br#"{"jsonrpc":"2.0"}"#;
    let second = br#"{"jsonrpc":"2.0","method":"exit"}"#;
    let mut wire = Vec::new();
    write_frame(&mut wire, first).unwrap_or_else(|error| panic!("frame: {error}"));
    write_frame(&mut wire, second).unwrap_or_else(|error| panic!("frame: {error}"));
    let mut input = io::BufReader::new(wire.as_slice());
    assert_eq!(read_frame(&mut input).expect("first frame"), Some(first.to_vec()));
    assert_eq!(read_frame(&mut input).expect("second frame"), Some(second.to_vec()));
    assert_eq!(read_frame(&mut input).expect("clean eof"), None);

    let mut duplicate =
        io::BufReader::new(b"Content-Length: 0\r\nContent-Length: 0\r\n\r\n".as_slice());
    assert!(read_frame(&mut duplicate).is_err());

    let oversized =
        format!("Content-Length: {}\r\n\r\n", zryna_language_server::MAX_LSP_MESSAGE_BYTES + 1);
    assert!(read_frame(&mut io::BufReader::new(oversized.as_bytes())).is_err());
    let mut exact = vec![0_u8; zryna_language_server::MAX_LSP_MESSAGE_BYTES];
    write_frame(&mut io::sink(), &exact).expect("exact frame limit");
    exact.push(0);
    assert!(write_frame(&mut io::sink(), &exact).is_err());
}

#[test]
fn open_change_and_close_publish_revision_bound_diagnostics() {
    let mut server = initialized();
    let first = open(&mut server, 1, "export function a(x: i32): i32 { return x; }\n");
    assert_eq!(first.len(), 2);
    assert_eq!(first[0]["method"], "zryna/publishDiagnostics");
    assert_eq!(first[0]["params"]["revision"], 1);
    assert_eq!(first[0]["params"]["documents"][0]["version"], 1);
    assert_eq!(first[1]["method"], "textDocument/publishDiagnostics");

    let second = change(&mut server, 2, "export function b(x: i32): i32 { return x; }\n");
    assert_eq!(second[0]["params"]["revision"], 2);
    assert_ne!(first[0]["params"]["snapshot"], second[0]["params"]["snapshot"]);

    let stale = change(&mut server, 2, "same version");
    assert_eq!(stale[0]["method"], "window/logMessage");
    let closed = request(
        &mut server,
        json!({
            "jsonrpc":"2.0","method":"textDocument/didClose","params":{
                "textDocument":{"uri":"file:///workspace/src/main.zry"}
            }
        }),
    );
    assert_eq!(closed[0]["params"]["diagnostics"], json!([]));
}

#[test]
fn queued_definition_is_cancelled_and_next_request_recovers() {
    let mut server = semantic_initialized();
    let _ = open(&mut server, 1, "export function a(x: i32): i32 { return x; }\n");
    assert!(definition(&mut server, 7, 0, 40).is_empty());
    assert!(
        request(
            &mut server,
            json!({
                "jsonrpc":"2.0","method":"$/cancelRequest","params":{"id":7}
            })
        )
        .is_empty()
    );
    let pending = server.finish_pending();
    let cancelled = emit(&mut server, pending);
    assert_eq!(cancelled[0]["error"]["code"], -32800);

    assert!(definition(&mut server, 8, 0, 40).is_empty());
    let pending = server.finish_pending();
    let recovered = emit(&mut server, pending);
    assert_eq!(recovered[0]["result"]["range"]["start"]["character"], 18);
}

#[test]
fn same_length_change_suppresses_stale_definition() {
    let mut server = semantic_initialized();
    let _ = open(&mut server, 1, "export function a(x: i32): i32 { return x; }\n");
    assert!(definition(&mut server, 9, 0, 40).is_empty());
    let _ = change(&mut server, 2, "export function a(y: i32): i32 { return y; }\n");
    let pending = server.finish_pending();
    let stale = emit(&mut server, pending);
    assert_eq!(stale[0]["error"]["code"], -32801);
}

#[test]
fn unicode_positions_reject_split_scalars_and_recover() {
    let mut server = initialized();
    let text = "// é😀e\u{301}\r\nexport function identity(x: i32): i32 { return x; }\n";
    let _ = open(&mut server, 1, text);
    let split = definition(&mut server, 10, 0, 4);
    assert_eq!(split[0]["error"]["code"], -32602);
    assert!(definition(&mut server, 11, 1, 47).is_empty());
    let pending = server.finish_pending();
    assert_eq!(emit(&mut server, pending)[0]["error"]["code"], -32803);
}

#[test]
fn malformed_foreign_and_unsupported_messages_cannot_publish() {
    let mut server = initialized();
    let foreign = request(
        &mut server,
        json!({
            "jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{
                "uri":"file:///elsewhere/main.zry","languageId":"zryna","version":1,"text":"x"
            }}
        }),
    );
    assert_eq!(foreign[0]["method"], "window/logMessage");
    let traversal = request(
        &mut server,
        json!({
            "jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{
                "uri":"file:///workspace/../main.zry","languageId":"zryna","version":1,"text":"x"
            }}
        }),
    );
    assert_eq!(traversal[0]["method"], "window/logMessage");
    let malformed =
        raw_request(&mut server, br#"{"jsonrpc":"2.0","id":1,"id":2,"method":"initialize"}"#);
    assert_eq!(malformed[0]["error"]["code"], -32700);
    let nested_duplicate = raw_request(
        &mut server,
        br#"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///workspace/a.zry","uri":"file:///workspace/b.zry","languageId":"zryna","version":1,"text":"x"}}}"#,
    );
    assert_eq!(nested_duplicate[0]["error"]["code"], -32700);
    let nested = format!("{}0{}", "[".repeat(65), "]".repeat(65));
    let deep = format!("{{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"x\",\"params\":{nested} }}");
    let over_depth = raw_request(&mut server, deep.as_bytes());
    assert_eq!(over_depth[0]["error"]["code"], -32600);
    let unsupported = request(
        &mut server,
        json!({
            "jsonrpc":"2.0","id":"x","method":"textDocument/rename","params":{}
        }),
    );
    assert_eq!(unsupported[0]["error"]["code"], -32601);
    let recovered = open(&mut server, 1, "export function a(x: i32): i32 { return x; }\n");
    assert_eq!(recovered[0]["method"], "zryna/publishDiagnostics");
}

#[test]
fn cancelled_and_stale_ids_remain_reserved_until_response_emission() {
    let mut server = semantic_initialized();
    let source = "export function a(x: i32): i32 { return x; }\n";
    let changed = "export function a(y: i32): i32 { return y; }\n";
    let _ = open(&mut server, 1, source);

    assert!(definition(&mut server, 7, 0, 40).is_empty());
    assert!(request(
        &mut server,
        json!({"jsonrpc":"2.0","method":"$/cancelRequest","params":{"id":7}}),
    )
    .is_empty());
    let duplicate = queued(
        &mut server,
        json!({"jsonrpc":"2.0","id":7,"method":"textDocument/definition","params":{
            "textDocument":{"uri":"file:///workspace/src/main.zry"},
            "position":{"line":0,"character":40}
        }}),
    );
    assert_eq!(duplicate[0].value()["id"], Value::Null);
    assert_eq!(duplicate[0].value()["error"]["code"], -32600);
    let _ = emit(&mut server, duplicate);
    let pending = server.finish_pending();
    let cancelled = emit(&mut server, pending);
    assert_eq!(cancelled[0]["id"], 7);
    assert_eq!(cancelled[0]["error"]["code"], -32800);

    assert!(definition(&mut server, 9, 0, 40).is_empty());
    let _ = change(&mut server, 2, changed);
    let collision = queued(&mut server, json!({"jsonrpc":"2.0","id":9,"method":"shutdown"}));
    assert_eq!(collision[0].value()["id"], Value::Null);
    let _ = emit(&mut server, collision);
    assert!(definition(&mut server, 10, 0, 40).is_empty());
    let pending = server.finish_pending();
    let completed = emit(&mut server, pending);
    assert_eq!(completed[0]["id"], 9);
    assert_eq!(completed[0]["error"]["code"], -32801);
    assert_eq!(completed[1]["id"], 10);

    let shutdown = request(&mut server, json!({"jsonrpc":"2.0","id":9,"method":"shutdown"}));
    assert_eq!(shutdown[0]["id"], 9);
    assert!(shutdown[0]["result"].is_null());
}

#[test]
fn numeric_and_string_ids_are_distinct_connection_authorities() {
    let mut server = semantic_initialized();
    let _ = open(&mut server, 1, "export function a(x: i32): i32 { return x; }\n");
    assert!(definition(&mut server, 7, 0, 40).is_empty());
    assert!(
        queued(
            &mut server,
            json!({"jsonrpc":"2.0","id":"7","method":"textDocument/definition","params":{
                "textDocument":{"uri":"file:///workspace/src/main.zry"},
                "position":{"line":0,"character":40}
            }}),
        )
        .is_empty()
    );
    let pending = server.finish_pending();
    let completed = emit(&mut server, pending);
    assert_eq!(completed.len(), 2);
    assert_eq!(completed[0]["id"], 7);
    assert_eq!(completed[1]["id"], "7");
}

#[test]
fn immediate_responses_release_only_after_successful_flush_and_inventory_is_bounded() {
    struct FailedWrite;
    impl Write for FailedWrite {
        fn write(&mut self, _buffer: &[u8]) -> io::Result<usize> {
            Err(io::Error::other("intentional write failure"))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    struct FailedFlush;
    impl Write for FailedFlush {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            Ok(buffer.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::other("intentional flush failure"))
        }
    }

    let mut server = initialized();
    let flush_failure =
        queued(&mut server, json!({"jsonrpc":"2.0","id":19,"method":"unknown","params":{}}))
            .pop()
            .unwrap_or_else(|| panic!("immediate flush response"));
    assert!(server.write_outgoing(&mut FailedFlush, flush_failure).is_err());
    let duplicate =
        queued(&mut server, json!({"jsonrpc":"2.0","id":19,"method":"unknown","params":{}}));
    assert_eq!(duplicate[0].value()["id"], Value::Null);
    let _ = emit(&mut server, duplicate);

    let first =
        queued(&mut server, json!({"jsonrpc":"2.0","id":20,"method":"unknown","params":{}}))
            .pop()
            .unwrap_or_else(|| panic!("immediate error response"));
    assert!(server.write_outgoing(&mut FailedWrite, first).is_err());
    let duplicate =
        queued(&mut server, json!({"jsonrpc":"2.0","id":20,"method":"unknown","params":{}}));
    assert_eq!(duplicate[0].value()["id"], Value::Null);
    let _ = emit(&mut server, duplicate);

    let mut retained = Vec::new();
    for id in 21..(21 + MAX_OUTSTANDING_REQUESTS - 2) {
        let mut response =
            queued(&mut server, json!({"jsonrpc":"2.0","id":id,"method":"unknown","params":{}}));
        retained.push(response.pop().unwrap_or_else(|| panic!("bounded response")));
    }
    let over_limit =
        queued(&mut server, json!({"jsonrpc":"2.0","id":1000,"method":"unknown","params":{}}));
    assert_eq!(over_limit[0].value()["id"], Value::Null);
    let _ = emit(&mut server, over_limit);

    let released_id = 21 + MAX_OUTSTANDING_REQUESTS - 3;
    let released = retained.pop().unwrap_or_else(|| panic!("retained response"));
    assert_eq!(released.value()["id"], released_id);
    server
        .write_outgoing(&mut Vec::new(), released)
        .unwrap_or_else(|error| panic!("successful release: {error}"));
    let recovered = queued(
        &mut server,
        json!({"jsonrpc":"2.0","id":released_id,"method":"unknown","params":{}}),
    );
    assert_eq!(recovered[0].value()["id"], released_id);
}

#[test]
fn shutdown_requires_ordered_exit() {
    let mut server = initialized();
    let shutdown = request(&mut server, json!({"jsonrpc":"2.0","id":12,"method":"shutdown"}));
    assert!(shutdown[0]["result"].is_null());
    assert!(!server.should_exit());
    assert!(request(&mut server, json!({"jsonrpc":"2.0","method":"exit"})).is_empty());
    assert!(server.should_exit());
}

#[path = "protocol/formatting.rs"]
mod formatting;
