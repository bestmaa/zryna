use super::{capture, verify_bodies};

const OTHER_ID: &str = "other-c-v0@0";
const OTHER_HEADER: &[u8] = b"#include <stdint.h>\nstruct other_handle;\nint32_t other_open(int32_t seed, struct other_handle **out);\nint32_t other_read(struct other_handle *handle, int32_t *out);\nvoid other_close(struct other_handle *handle);\n";

fn rename(value: &serde_json::Value) -> serde_json::Value {
    serde_json::from_str(
        &serde_json::to_string(value)
            .expect("independent records")
            .replace("fixture-c-v0@0", OTHER_ID)
            .replace("fixture_handle", "other_handle")
            .replace("fixture_open", "other_open")
            .replace("fixture_read", "other_read")
            .replace("fixture_close", "other_close"),
    )
    .expect("independent exact other-library contract")
}

fn two_libraries(handle: &str) -> capture::Capture {
    let mut document = capture::document();
    let mut library = document["libraries"][0].clone();
    library["kinds"] = serde_json::json!(["fixture-c-v0@0/fixture_handle"]);
    library["allocators"]
        .as_array_mut()
        .expect("allocator list")
        .retain(|allocator| allocator["category"] == "handle");
    let library = rename(&library);
    let operations: Vec<_> = document["operations"]
        .as_array()
        .expect("operations")
        .iter()
        .filter(|operation| {
            matches!(
                operation["symbol"].as_str(),
                Some("fixture_open" | "fixture_read" | "fixture_close")
            )
        })
        .map(rename)
        .collect();
    document["libraries"].as_array_mut().expect("libraries").push(library);
    document["operations"].as_array_mut().expect("operations").extend(operations);
    let other = capture::HANDLE
        .replace("readSeed", "otherSeed")
        .replace("fixture-c-v0@0", OTHER_ID)
        .replace("fixture_handle", "other_handle")
        .replace("fixture_open", "other_open")
        .replace("fixture_read", "other_read")
        .replace("fixture_close", "other_close");
    capture::captured_headers(
        &[
            ("buffer", capture::BUFFER),
            ("handle", handle),
            ("other", &other),
            ("scalar", capture::SCALAR),
        ],
        false,
        document,
        &[("fixture-c-v0@0", capture::HEADER), (OTHER_ID, OTHER_HEADER)],
    )
}

#[test]
fn native_c_body_v0_same_handle_carrier_from_another_library_never_substitutes_for_nominal_owner() {
    let valid = two_libraries(capture::HANDLE);
    let bodies = verify_bodies(&valid.sources, &valid.declarations)
        .expect("two separately captured nominal contracts");
    assert_eq!(bodies.functions().len(), 7);
    let bad = capture::HANDLE.replace(
        "Ffi.rawCall(\"fixture-c-v0@0/fixture_read\", handle, value)",
        "Ffi.rawCall(\"other-c-v0@0/other_read\", handle, value)",
    );
    // Keep the original read declaration's separate source binding genuinely present.
    let handle = format!("{bad}\n{}", capture::HANDLE.replace("readSeed", "keptBinding"));
    let capture = two_libraries(&handle);
    assert_eq!(
        verify_bodies(&capture.sources, &capture.declarations)
            .expect_err("same Handle type is not nominal authority")
            .detail(),
        "owner-nominal-identity"
    );
}

fn second_allocator(release: &str) -> capture::Capture {
    let mut document = capture::document();
    let allocator = document["libraries"][0]["allocators"]
        .as_array()
        .expect("allocators")
        .iter()
        .find(|allocator| allocator["category"] == "handle")
        .expect("original handle allocator")
        .clone();
    let replace = |value: &serde_json::Value| {
        serde_json::from_str::<serde_json::Value>(
            &serde_json::to_string(value)
                .expect("independent original contract")
                .replace("fixture_open", "second_open")
                .replace("fixture_close", "second_close"),
        )
        .expect("same kind, different allocator and release")
    };
    let operations: Vec<_> = document["operations"]
        .as_array()
        .expect("operations")
        .iter()
        .filter(|operation| {
            matches!(operation["symbol"].as_str(), Some("fixture_open" | "fixture_close"))
        })
        .map(replace)
        .collect();
    let allocators = document["libraries"][0]["allocators"].as_array_mut().expect("allocators");
    allocators.push(replace(&allocator));
    allocators.sort_by(|left, right| left["id"].as_str().cmp(&right["id"].as_str()));
    document["operations"].as_array_mut().expect("operations").extend(operations);
    let extra = format!(
        r#"
function second(seed: i32): i32 {{
  const out: FfiHandleOut = Ffi.outHandle("fixture-c-v0@0/fixture_handle");
  const status: i32 = Ffi.rawCall("fixture-c-v0@0/second_open", seed, out);
  if (status !== 0) {{ return Ffi.foreignError("fixture-c-v0@0/second_open", status); }}
  const handle: FfiHandle = Ffi.takeHandle(out);
  Ffi.release("fixture-c-v0@0/{release}", handle);
  return 0;
}}
function keptSecondBinding(seed: i32): i32 {{
  const out: FfiHandleOut = Ffi.outHandle("fixture-c-v0@0/fixture_handle");
  const status: i32 = Ffi.rawCall("fixture-c-v0@0/second_open", seed, out);
  if (status !== 0) {{ return Ffi.foreignError("fixture-c-v0@0/second_open", status); }}
  const handle: FfiHandle = Ffi.takeHandle(out);
  Ffi.release("fixture-c-v0@0/second_close", handle);
  return 0;
}}
"#
    );
    let header = [capture::HEADER,
        b"\nint32_t second_open(int32_t seed, struct fixture_handle **out);\nvoid second_close(struct fixture_handle *handle);\n"].concat();
    capture::captured_headers(
        &[
            ("buffer", capture::BUFFER),
            ("handle", &format!("{}{extra}", capture::HANDLE)),
            ("scalar", capture::SCALAR),
        ],
        false,
        document,
        &[("fixture-c-v0@0", &header)],
    )
}

#[test]
fn native_c_body_v0_same_library_and_kind_still_require_the_exact_creating_allocator_release() {
    let valid = second_allocator("second_close");
    let bodies = verify_bodies(&valid.sources, &valid.declarations)
        .expect("two allowed allocators of one kind");
    let second = bodies
        .functions()
        .iter()
        .find(|function| function.name() == "second")
        .expect("second allocator body");
    assert_eq!(second.owner_origins()[0].kind(), "fixture-c-v0@0/fixture_handle");
    assert_eq!(second.owner_origins()[0].allocator(), "fixture-c-v0@0/second_open");
    let bad = second_allocator("fixture_close");
    assert_eq!(
        verify_bodies(&bad.sources, &bad.declarations)
            .expect_err("same kind is not matching allocator authority")
            .detail(),
        "owner-nominal-identity"
    );
}
