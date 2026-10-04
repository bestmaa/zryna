//! Actual private runtime compilation, retaining its original rendered source and stage.
use super::*;
use crate::native::NativeStage;
use std::fs;

pub(super) fn compile(
    fixture: &Fixture,
    requirements: &HandleLinkRequirements,
    sanitized: bool,
) -> (NativeStage, Vec<u8>) {
    let runtime = requirements.private_runtime_source().expect("retained actual private source");
    assert_eq!(
        &sha(runtime),
        requirements
            .private_runtime_source_sha256()
            .expect("independent byte fixture prerequisite")
    );
    let stage = fixture.stage();
    stage.write_input(&stage.runtime_source, runtime).expect("retained private source snapshot");
    let runtime_object =
        stage.capability_file_path("runtime.o").expect("independent byte fixture prerequisite");
    let mut args: Vec<OsString> = [
        "-std=c11",
        "-O0",
        "-g0",
        "-fno-ident",
        "-fno-pie",
        "-fno-pic",
        "-fcf-protection=none",
        "-fno-unwind-tables",
        "-fno-asynchronous-unwind-tables",
        "-Wall",
        "-Wextra",
        "-Werror",
        "-pedantic",
        "-c",
    ]
    .into_iter()
    .map(Into::into)
    .collect();
    if sanitized {
        args.extend(SANITIZERS.map(Into::into));
    }
    args.extend([
        stage
            .capability_file_path("runtime.c")
            .expect("independent byte fixture prerequisite")
            .into_os_string(),
        "-o".into(),
        runtime_object.clone().into_os_string(),
    ]);
    let built = super::super::super::harness::compile(&stage, &args);
    assert!(built.status.success(), "{}", String::from_utf8_lossy(&built.stderr));
    assert!(built.stdout.is_empty() && built.stderr.is_empty());
    let runtime_bytes =
        fs::read(&runtime_object).expect("actual separately compiled private object");
    (stage, runtime_bytes)
}
