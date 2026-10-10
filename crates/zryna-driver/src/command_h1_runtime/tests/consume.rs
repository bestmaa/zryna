//! Found payloads enter ordinary ownership operations as real initialized language Strings.

use super::*;

#[test]
fn actual_found_payload_moves_into_helper_and_clones_concatenates_then_drops() -> std::io::Result<()>
{
    let source = SourceFixture::new("environment-consume");
    for value in [None, Some(""), Some("on"), Some("é🙂"), Some("🙂".repeat(256).as_str())] {
        let input = PrivateInput::new("MODE", value)?;
        let policy = CommandH1HostPolicy::environment("MODE").expect("exact key approval");
        let run = source
            .prepare(Some(&input), &policy)
            .expect("real owned result source")
            .execute(&policy)
            .expect("real payload ownership operations");
        returned(
            &run,
            if value.is_some() { CommandH1RunReturn::Ok } else { CommandH1RunReturn::Err },
        );
        drop(run);
        input.assert_released()?;
    }
    Ok(())
}
