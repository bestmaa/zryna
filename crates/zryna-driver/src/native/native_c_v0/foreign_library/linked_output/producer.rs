use super::*;
use object::{Object as _, ObjectKind};

pub(crate) fn compile_object(
    root: &ArtifactOutputRoot,
    source: &[u8],
    tools: &LinuxX8664LinkToolchain,
) -> Result<CompiledObject, Failure> {
    compile(root, source, tools, true, &[])
}

pub(super) fn compile_stack_protected_object(
    root: &ArtifactOutputRoot,
    source: &[u8],
    tools: &LinuxX8664LinkToolchain,
) -> Result<CompiledObject, Failure> {
    compile(root, source, tools, true, &["-fstack-protector-all"])
}

fn compile(
    root: &ArtifactOutputRoot,
    source: &[u8],
    tools: &LinuxX8664LinkToolchain,
    fixture_warnings: bool,
    control_flags: &[&str],
) -> Result<CompiledObject, Failure> {
    if source.len() > zryna_backend_native::MAX_NATIVE_OBJECT_BYTES {
        return Err(rejected().into());
    }
    let stages = staging::stages(root, 1)?;
    let mut invocation = None;
    let result = (|| -> Result<CompiledObject, Failure> {
        let stage = &stages[0];
        stage.write_input(&stage.harness, source)?;
        let object = stage.capability_file_path("runtime.o")?;
        let mut args = [
            "-std=c11",
            "-O0",
            "-g0",
            "-fno-ident",
            "-fno-pie",
            "-fno-pic",
            "-fcf-protection=none",
            "-fno-stack-protector",
            "-fno-unwind-tables",
            "-fno-asynchronous-unwind-tables",
        ]
        .into_iter()
        .map(OsString::from)
        .collect::<Vec<_>>();
        args.extend(control_flags.iter().map(OsString::from));
        if fixture_warnings {
            args.extend(["-Wall", "-Wextra", "-Werror", "-pedantic"].map(OsString::from));
        }
        args.push("-c".into());
        args.extend([
            "-o".into(),
            object.into_os_string(),
            stage.capability_file_path("invocation.c")?.into_os_string(),
        ]);
        let actual = process::invoke(stage, tools, args)?;
        invocation = Some(actual.clone());
        if !actual.status.success() || !actual.stdout.is_empty() || !actual.stderr.is_empty() {
            return Err(rejected().into());
        }
        let bytes = staging::bytes(
            &stage.capability_file_path("runtime.o")?,
            zryna_backend_native::MAX_NATIVE_OBJECT_BYTES,
        )?;
        let file = object::File::parse(bytes.as_slice()).map_err(|_| rejected())?;
        if file.kind() != ObjectKind::Relocatable
            || file.architecture() != object::Architecture::X86_64
            || file.endianness() != object::Endianness::Little
            || !file.is_64()
        {
            return Err(rejected().into());
        }
        Ok(CompiledObject { source: source.to_vec(), bytes, invocation: actual })
    })()
    .map_err(|mut failure| {
        failure.invocations.extend(invocation.iter().cloned());
        failure
    });
    staging::finish(&stages, result, invocation.as_slice())
}

pub(crate) fn observe(
    root: &ArtifactOutputRoot,
    requirements: &HandleLinkRequirements,
    library: &CapturedForeignLibrary,
    foreign: &CompiledObject,
    client_source: &[u8],
    runtime: Option<&CompiledObject>,
    wrap: bool,
) -> Result<Observation, Failure> {
    check_foreign(requirements, library, foreign)?;
    if runtime.map(|value| value.source.as_slice()) != requirements.private_runtime_source()
        || runtime.is_some_and(|value| value.tools() != foreign.tools())
    {
        return Err(rejected().into());
    }
    if let Some(runtime) = runtime {
        runtime_object::check(requirements, &runtime.bytes)?;
    }
    // Match the existing fixture client policy; oracle helpers need not all be referenced.
    // Foreign and private-runtime compilation retain their strict warning policy.
    let mut completed = vec![foreign.invocation.clone()];
    completed.extend(runtime.map(|value| value.invocation.clone()));
    let attach = |mut failure: Failure| {
        failure.invocations.extend(completed.iter().cloned());
        failure
    };
    let client = compile(root, client_source, foreign.tools(), false, &[]).map_err(attach)?;
    completed.push(client.invocation.clone());
    let stages =
        staging::stages(root, if runtime.is_some() { 3 } else { 2 }).map_err(|mut failure| {
            failure.invocations.extend(completed.iter().cloned());
            failure
        })?;
    let mut invocation = None;
    let result = (|| -> Result<Observation, Failure> {
        let stage = &stages[0];
        stage.write_input(&stage.object, requirements.object().bytes())?;
        stage.write_input(&stage.directory.join("runtime.o"), library.bytes())?;
        stages[1].write_input(&stages[1].directory.join("runtime.o"), &client.bytes)?;
        if let Some(runtime) = runtime {
            stages[2].write_input(&stages[2].directory.join("runtime.o"), &runtime.bytes)?;
        }
        let mut args = [
            "-no-pie",
            "-Wl,--build-id=none",
            "-Wl,--fatal-warnings",
            "-Wl,--no-undefined",
            "-Wl,-z,noexecstack,-z,relro,-z,now",
            "-Wl,-t,-t",
        ]
        .into_iter()
        .map(OsString::from)
        .collect::<Vec<_>>();
        if wrap {
            args.extend(["-Wl,--wrap=malloc".into(), "-Wl,--wrap=free".into()]);
        }
        args.extend([
            "-o".into(),
            stage.capability_file_path("invocation.elf")?.into_os_string(),
            stages[1].capability_file_path("runtime.o")?.into_os_string(),
            stage.capability_file_path("program.o")?.into_os_string(),
            stage.capability_file_path("runtime.o")?.into_os_string(),
        ]);
        if runtime.is_some() {
            args.push(stages[2].capability_file_path("runtime.o")?.into_os_string());
        }
        let link = process::invoke(stage, foreign.tools(), args)?;
        invocation = Some(link.clone());
        if !link.status.success() || !link.stderr.is_empty() {
            return Err(rejected().into());
        }
        let trace = process::trace(&link.stdout)?;
        let executable = stage.capability_file_path("invocation.elf")?;
        let final_elf = staging::bytes(&executable, native::MAX_NATIVE_EXECUTABLE_BYTES)?;
        let (_, audited) = native::audit_staged_executable(&executable, "zryna_c_v0_i_dispatch")?;
        if final_elf != *audited {
            return Err(rejected().into());
        }
        let dependencies = elf::read(&final_elf)?;
        // There is intentionally no executable mode, Run phase, loader lookup or permission seal.
        Ok(Observation {
            requirements: requirements.clone(),
            library: library.clone(),
            foreign: foreign.clone(),
            client: client.clone(),
            runtime: runtime.cloned(),
            link,
            final_elf,
            dependencies,
            trace,
        })
    })()
    .map_err(|mut failure| {
        failure.invocations.extend(invocation.iter().cloned());
        failure.invocations.extend(completed.iter().cloned());
        failure
    });
    completed.extend(invocation);
    staging::finish(&stages, result, &completed)
}

/// Binding rejection precedes any new client or private-runtime compiler invocation.
pub(crate) fn check_foreign(
    requirements: &HandleLinkRequirements,
    library: &CapturedForeignLibrary,
    foreign: &CompiledObject,
) -> Result<(), Failure> {
    library.check_binding(requirements)?;
    if foreign.bytes != library.bytes() || !foreign.invocation.status.success() {
        return Err(rejected().into());
    }
    Ok(())
}
