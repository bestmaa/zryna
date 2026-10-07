use super::*;

pub(super) fn invoke(
    stage: &NativeStage,
    tools: &LinuxX8664LinkToolchain,
    arguments: Vec<OsString>,
) -> Result<Invocation, Failure> {
    native::revalidate_tool(&tools.driver, &tools.driver_identity)?;
    native::revalidate_tool(&tools.linker, &tools.linker_identity)?;
    stage.revalidate()?;
    let directory = stage.capability_directory_path();
    let limits = native::NativeProcessLimits::default();
    let output = native::run_bounded_process(
        &tools.driver,
        &arguments,
        &directory,
        limits.link_timeout(),
        limits.tool_output_bytes(),
        limits.tool_output_bytes(),
        native::ProcessPhase::Link,
        Some(&directory),
    )?;
    let actual = Invocation {
        tools: tools.clone(),
        arguments,
        directory,
        status: output.status,
        stdout: output.stdout,
        stderr: output.stderr,
    };
    validate(stage, actual)
}

pub(super) fn validate(stage: &NativeStage, actual: Invocation) -> Result<Invocation, Failure> {
    let tools = &actual.tools;
    let validation = (|| -> Result<(), Diagnostic> {
        native::revalidate_tool(&tools.driver, &tools.driver_identity)?;
        native::revalidate_tool(&tools.linker, &tools.linker_identity)?;
        stage.revalidate()
    })();
    validation
        .map_err(|error| Failure { diagnostics: vec![error], invocations: vec![actual.clone()] })?;
    Ok(actual)
}

pub(super) fn trace(bytes: &[u8]) -> Result<Vec<String>, Diagnostic> {
    if bytes.len() > native::MAX_NATIVE_TOOL_OUTPUT_BYTES || bytes.contains(&0) {
        return Err(rejected());
    }
    let text = std::str::from_utf8(bytes).map_err(|_| rejected())?;
    if text.is_empty() || !text.ends_with('\n') {
        return Err(rejected());
    }
    text.strip_suffix('\n')
        .ok_or_else(rejected)?
        .split('\n')
        .map(|line| {
            if line.is_empty() || line.contains('\r') {
                Err(rejected())
            } else {
                Ok(line.to_owned())
            }
        })
        .collect()
}
