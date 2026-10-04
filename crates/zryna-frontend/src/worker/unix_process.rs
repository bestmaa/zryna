//! Unix worker process groups and shared process-creation exclusion.

use super::{
    ManagedProcess, SpawnSpec, SpawnedWorker, WorkerError, WorkerFailure, WorkerInput, WorkerOutput,
};
use process_wrap::std::{ChildWrapper, CommandWrap, ProcessGroup};
use std::process::{Command, ExitStatus, Stdio};

struct UnixProcess {
    child: Box<dyn ChildWrapper>,
    group_id: u32,
}

impl ManagedProcess for UnixProcess {
    fn start_kill(&mut self) -> std::io::Result<()> {
        self.child.start_kill()
    }

    fn try_wait(&mut self) -> std::io::Result<Option<ExitStatus>> {
        self.child.try_wait()
    }

    fn cleanup_confirmed(&self) -> std::io::Result<bool> {
        use nix::{errno::Errno, sys::signal, unistd::Pid};

        let raw_group_id = i32::try_from(self.group_id).map_err(std::io::Error::other)?;
        match signal::killpg(Pid::from_raw(raw_group_id), None) {
            Ok(()) => Ok(false),
            Err(Errno::ESRCH) => Ok(true),
            Err(error) => Err(std::io::Error::from(error)),
        }
    }
}

pub(super) fn spawn_worker(spec: &impl SpawnSpec) -> Result<SpawnedWorker, WorkerError> {
    let mut native_command = Command::new(spec.executable());
    native_command
        .args(spec.arguments())
        .current_dir(spec.current_dir())
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut command = CommandWrap::from(native_command);
    command.wrap(ProcessGroup::leader());
    let mut child = zryna_process::spawn(|| command.spawn())
        .map_err(|_| WorkerError::new(WorkerFailure::Spawn))?;
    let group_id = child.id();
    let stdin = child
        .stdin()
        .take()
        .map(|stream| Box::new(stream) as WorkerInput)
        .ok_or_else(|| WorkerError::new(WorkerFailure::ProcessIo))?;
    let stdout = child
        .stdout()
        .take()
        .map(|stream| Box::new(stream) as WorkerOutput)
        .ok_or_else(|| WorkerError::new(WorkerFailure::ProcessIo))?;
    let stderr = child
        .stderr()
        .take()
        .map(|stream| Box::new(stream) as WorkerOutput)
        .ok_or_else(|| WorkerError::new(WorkerFailure::ProcessIo))?;
    Ok(SpawnedWorker { process: Box::new(UnixProcess { child, group_id }), stdin, stdout, stderr })
}
