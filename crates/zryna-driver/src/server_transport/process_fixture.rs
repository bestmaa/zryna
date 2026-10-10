//! Test-only fixture group/job ownership; the production Unix worker owns its own group.

use std::{
    io::{Read, Write},
    process::ExitStatus,
};

pub(super) struct Spawned {
    pub(super) child: Child,
    pub(super) input: Box<dyn Write + Send>,
    pub(super) output: Box<dyn Read + Send>,
}

#[cfg(unix)]
pub(super) struct Child {
    process: Box<dyn process_wrap::std::ChildWrapper>,
    reaped: bool,
    termination_requested: bool,
}
#[cfg(windows)]
pub(super) struct Child {
    process: windows_spawn::Child,
    job: windows_spawn::Job,
    reaped: bool,
    termination_requested: bool,
}

impl Child {
    pub(super) fn try_wait(&mut self) -> std::io::Result<Option<ExitStatus>> {
        let result = self.process.try_wait()?;
        if result.is_some() {
            self.reaped = true;
        }
        Ok(result)
    }
    pub(super) fn kill(&mut self) -> std::io::Result<()> {
        if self.termination_requested || (cfg!(unix) && self.reaped) {
            return Ok(());
        }
        // A reaped Unix leader's saved PGID can be reused. Never signal that number again.
        #[cfg(unix)]
        let result = self.process.start_kill();
        #[cfg(windows)]
        let result = self.job.terminate(1);
        if result.is_ok() {
            self.termination_requested = true;
        }
        result
    }
    pub(super) fn wait(&mut self) -> std::io::Result<ExitStatus> {
        let status = self.process.wait()?;
        self.reaped = true;
        Ok(status)
    }
}
impl Drop for Child {
    fn drop(&mut self) {
        let _ = self.kill();
        let _ = self.wait();
    }
}

#[cfg(unix)]
pub(super) fn spawn(arguments: &[&str], marker: &str) -> std::io::Result<Spawned> {
    use process_wrap::std::{CommandWrap, ProcessGroup};
    use std::process::{Command, Stdio};
    let mut command = Command::new(std::env::current_exe()?);
    command
        .args(arguments)
        .env(marker, "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    let mut command = CommandWrap::from(command);
    command.wrap(ProcessGroup::leader());
    let mut child = Child {
        process: zryna_process::spawn(|| command.spawn())?,
        reaped: false,
        termination_requested: false,
    };
    let input =
        child.process.stdin().take().ok_or_else(|| std::io::Error::other("lost fixture stdin"))?;
    let output = child
        .process
        .stdout()
        .take()
        .ok_or_else(|| std::io::Error::other("lost fixture stdout"))?;
    Ok(Spawned { child, input: Box::new(input), output: Box::new(output) })
}

#[cfg(windows)]
pub(super) fn spawn(arguments: &[&str], marker: &str) -> std::io::Result<Spawned> {
    use windows_spawn::{Command, Job, SpawnOptions, Stdio};
    let job = Job::create()?;
    job.set_kill_on_close(true)?;
    let mut command = Command::new(std::env::current_exe()?);
    command
        .args(arguments)
        .env(marker, "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    let process = zryna_process::spawn(|| command.spawn_with(SpawnOptions::new().job(&job)))?;
    let mut child = Child { process, job, reaped: false, termination_requested: false };
    let input =
        child.process.stdin.take().ok_or_else(|| std::io::Error::other("lost fixture stdin"))?;
    let output =
        child.process.stdout.take().ok_or_else(|| std::io::Error::other("lost fixture stdout"))?;
    Ok(Spawned { child, input: Box::new(input), output: Box::new(output) })
}
