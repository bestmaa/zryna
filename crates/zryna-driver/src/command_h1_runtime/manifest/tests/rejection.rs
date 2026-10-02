use super::*;

fn granted() -> io::Result<(PrivateInput, ExecutedCommandH1)> {
    let source = SourceFixture::new("environment-match");
    let input = PrivateInput::new("MODE", Some("private-input-only"))?;
    let policy = CommandH1HostPolicy::environment("MODE").expect("root approval");
    let run = source
        .prepare(Some(&input), &policy)
        .expect("real preparation")
        .execute(&policy)
        .expect("actual granted run");
    Ok((input, run))
}

fn denied() -> io::Result<(PrivateInput, ExecutedCommandH1)> {
    let source = SourceFixture::new("environment-match");
    let input = PrivateInput::new("MODE", Some("private-input-only"))?;
    let policy = CommandH1HostPolicy::environment("MODE").expect("root approval");
    let run = source
        .prepare(Some(&input), &policy)
        .expect("real preparation")
        .execute_before_call(&policy, &|| policy.revoke())
        .expect("actual revoked callback run");
    Ok((input, run))
}

#[test]
fn unknown_fields_at_every_record_level_and_secret_input_fields_reject() -> io::Result<()> {
    let (_input, run) = denied()?;
    let original = bytes(&run);
    for pointer in [
        "",
        "/source",
        "/source/scalarEntry",
        "/source/requirements/0",
        "/composition",
        "/composition/approved/0",
        "/component",
        "/grants",
        "/grants/requested/0",
        "/grants/effective/0",
        "/input",
        "/limits",
        "/execution",
        "/execution/denial",
    ] {
        rejected(&original, |value| {
            value
                .pointer_mut(pointer)
                .expect("actual record exists")
                .as_object_mut()
                .expect("closed object")
                .insert("unknown".into(), true.into());
        });
    }
    for field in ["value", "inputPath", "valueHash", "valueSha256"] {
        rejected(&original, |value| {
            value["input"][field] = "untrusted-input-field".into();
        });
    }
    Ok(())
}

#[test]
fn duplicate_fields_at_every_nested_level_reject_even_when_values_match() -> io::Result<()> {
    let (_input, run) = denied()?;
    let original = bytes(&run);
    let text = std::str::from_utf8(&original).expect("JSON UTF-8");
    for (key, value) in [
        ("schema", format!("\"{COMMAND_H1_MANIFEST_SCHEMA}\"")),
        ("profile", "\"command-h1-v1\"".into()),
        ("abiVersion", "1".into()),
        ("capability", "\"environment\"".into()),
        ("root", "\"command-source\"".into()),
        ("kind", "\"wasi-command-component-v1\"".into()),
        ("hostPolicy", format!("\"{HOST_POLICY}\"")),
        ("utf8ByteCount", "18".into()),
        ("fuel", envelope::FUEL.to_string()),
        ("runReturn", "\"absent\"".into()),
        ("reason", "\"permission-denied\"".into()),
    ] {
        let field = format!("\"{key}\":{value}");
        assert!(text.contains(&field), "actual serialized field required for duplicate mutant");
        let mutant = text.replacen(&field, &format!("{field},{field}"), 1);
        rejected_bytes(&original, mutant.as_bytes());
    }
    Ok(())
}

#[test]
fn alternate_record_shapes_nullable_optional_fields_and_unit_enum_objects_reject() -> io::Result<()>
{
    let (_input, run) = granted()?;
    let original = bytes(&run);
    for pointer in [
        "/source",
        "/source/scalarEntry",
        "/source/requirements/0",
        "/composition",
        "/component",
        "/grants",
        "/input",
        "/limits",
        "/execution",
    ] {
        rejected(&original, |value| {
            let field = value.pointer_mut(pointer).expect("record");
            *field = Value::Array(field.as_object().expect("object").values().cloned().collect());
        });
    }
    for field in ["trapCategory", "trapIdentity", "denial"] {
        rejected(&original, |value| {
            value["execution"][field] = Value::Null;
        });
    }
    rejected(&original, |value| {
        value["input"]["kind"] = serde_json::json!({"present": null});
    });
    rejected(&original, |value| {
        value["execution"]["kind"] = serde_json::json!({"run-returned": null});
    });
    rejected(&original, |value| {
        value["source"]["scalarEntry"]["result"] = serde_json::json!({"bool": null});
    });
    rejected(&original, |value| {
        value["teardown"] = serde_json::json!({"confirmed": null});
    });
    Ok(())
}

#[test]
fn changed_world_imports_exports_pins_profile_hash_or_artifact_path_reject() {
    let original = bytes(&pure("pure-entry"));
    for (pointer, value) in [
        ("/schema", "zryna.wasi-command-manifest.v2"),
        ("/source/profile", "data-ownership-v1"),
        ("/composition/world", "zryna:capability-profiles/server@0.1.0"),
        ("/composition/row", "WitServer"),
        ("/component/world", "zryna:capability-profiles/command@0.1.1"),
        ("/component/wasiVersion", "0.2.13"),
        ("/component/path", "component/candidate.wasm"),
        ("/component/kind", "webassembly-component"),
        (
            "/component/witClosureDigest",
            "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
        ),
        (
            "/component/worldSha256",
            "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
        ),
        ("/source/sha256", "FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF"),
        (
            "/source/programBinding",
            "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
        ),
    ] {
        rejected(&original, |document| {
            *document.pointer_mut(pointer).expect("field") = value.into();
        });
    }
    for pointer in [
        "/component/packages",
        "/component/explicitImports",
        "/component/resolvedImports",
        "/component/exports",
    ] {
        rejected(&original, |value| {
            value.pointer_mut(pointer).expect("list").as_array_mut().expect("array").pop();
        });
        rejected(&original, |value| {
            let entries = value.pointer_mut(pointer).expect("list").as_array_mut().expect("array");
            if entries.len() > 1 {
                entries.reverse();
            } else {
                entries.push(entries[0].clone());
            }
        });
    }
    rejected(&original, |value| {
        value["component"]["witFileCount"] = 33.into();
    });
    rejected(&original, |value| {
        value["source"]["verifierRevision"] = 2.into();
    });
    rejected(&original, |value| {
        value["source"]["scalarEntry"]["result"] = "i32".into();
    });
}

#[test]
fn independently_changed_limits_grants_quota_and_input_combinations_reject() -> io::Result<()> {
    let (_input, run) = granted()?;
    let original = bytes(&run);
    let document = value(&run);
    for (name, amount) in document["limits"].as_object().expect("limits") {
        rejected(&original, |value| {
            value["limits"][name] = match amount {
                Value::Bool(value) => (!value).into(),
                Value::Number(value) => (value.as_u64().expect("unsigned limit") + 1).into(),
                _ => panic!("closed scalar limit"),
            };
        });
    }
    for pointer in
        ["/source/requirements", "/composition/approved", "/grants/requested", "/grants/effective"]
    {
        rejected(&original, |value| {
            *value.pointer_mut(pointer).expect("set") = Value::Array(Vec::new());
        });
    }
    for pointer in ["/composition/staticQuota", "/grants/staticQuota", "/grants/registryCeilings"] {
        rejected(&original, |value| {
            value.pointer_mut(pointer).expect("quota")[2] = 2.into();
        });
    }
    rejected(&original, |value| {
        value["input"]["utf8ByteCount"] = 1025.into();
    });
    rejected(&original, |value| {
        value["input"]["kind"] = "missing".into();
    });
    rejected(&original, |value| {
        value["input"]["kind"] = "none".into();
    });
    Ok(())
}

#[test]
fn returned_denied_and_trapped_combinations_reject_fabricated_result_or_denial_metadata()
-> io::Result<()> {
    let returned = bytes(&pure("pure-entry"));
    rejected(&returned, |value| {
        value["execution"]["runReturn"] = "absent".into();
    });
    rejected(&returned, |value| {
        value["execution"]["trapCategory"] = "host-process-failure".into();
    });
    let (_input, run) = denied()?;
    let denied = bytes(&run);
    rejected(&denied, |value| {
        value["execution"]["runReturn"] = "err".into();
    });
    rejected(&denied, |value| {
        value["execution"]["trapIdentity"] = "zryna.trap.bounds-v1".into();
    });
    for (field, data) in [
        ("interface", "wasi:filesystem/types@0.2.12"),
        ("operation", "get-arguments"),
        ("reason", "missing"),
        ("policyRevision", "zryna.command-h1.host.v2"),
    ] {
        rejected(&denied, |value| {
            value["execution"]["denial"][field] = data.into();
        });
    }
    let trapped = bytes(&pure("language-bounds"));
    rejected(&trapped, |value| {
        value["execution"]["runReturn"] = "ok".into();
    });
    rejected(&trapped, |value| {
        value["execution"]["trapIdentity"] = "arbitrary-exception-text".into();
    });
    rejected(&trapped, |value| {
        value["execution"].as_object_mut().expect("execution").remove("trapIdentity");
    });
    rejected(&trapped, |value| {
        value["execution"]["trapCategory"] = "host-process-failure".into();
    });
    rejected(&trapped, |value| {
        value["execution"]["trapCategory"] = "interface-violation".into();
    });
    Ok(())
}

#[test]
fn strict_utf8_surrogates_trailing_records_and_exact_manifest_byte_bound_reject_and_recover() {
    let original = bytes(&pure("pure-entry"));
    for suffix in [b"null".as_slice(), b"{}", b"true"] {
        let mut mutant = original.clone();
        mutant.extend_from_slice(suffix);
        rejected_bytes(&original, &mutant);
    }
    rejected_bytes(&original, &[0xff]);
    let text = std::str::from_utf8(&original).expect("UTF-8");
    let surrogate = text.replacen("src/main.zry", r"src/\ud800.zry", 1);
    rejected_bytes(&original, surrogate.as_bytes());
    let mut exact = original.clone();
    exact.resize(MAX_COMMAND_H1_MANIFEST_BYTES, b' ');
    decode_command_h1_manifest(&exact).expect("exact byte bound with JSON trivia");
    exact.push(b' ');
    rejected_bytes(&original, &exact);
}
