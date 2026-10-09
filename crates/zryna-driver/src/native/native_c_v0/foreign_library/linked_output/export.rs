//! Create-only private evidence export. Serialized observations cannot recreate issuer authority.
use super::*;
use sha2::{Digest as _, Sha256};
use std::{
    fs, io::Write as _, os::unix::ffi::OsStrExt as _, os::unix::fs::OpenOptionsExt as _, path::Path,
};

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn invocation(value: &Invocation) -> serde_json::Value {
    serde_json::json!({
        "tools":format!("{:?}",value.tools),
        "driver_path_bytes":value.tools.driver.as_os_str().as_bytes(),
        "linker_path_bytes":value.tools.linker.as_os_str().as_bytes(),
        "tool_origin_scope":"actual discovered capability; metadata/version identity, not independently approved tool bytes",
        "arguments_bytes":value.arguments.iter().map(|arg|arg.as_os_str().as_bytes()).collect::<Vec<_>>(),
        "directory_bytes":value.directory.as_os_str().as_bytes(),
        "exit_status":format!("{}",value.status),"success":value.status.success(),
        "stdout_bytes":value.stdout,"stderr_bytes":value.stderr,
        "phase":"Link helper; compile or link fixture only; no Run phase",
        "environment":{"LANG":"C","LC_ALL":"C","TZ":"UTC","SOURCE_DATE_EPOCH":"0",
            "PATH":"/usr/bin:/bin","TMPDIR_bytes":value.directory.as_os_str().as_bytes()},
        "environment_cleared":true})
}

impl Observation {
    pub(crate) fn report(&self) -> serde_json::Value {
        serde_json::json!({"schema_version":1,"kind":"private-non-authorizing-compile-link-observation",
            "source_map_identity":format!("{:?}",self.requirements.object().program().source().source_map_identity()),
            "program_object_sha256":digest(self.requirements.object().bytes()),
            "library_id":self.library.library_id(),"foreign_object_sha256":digest(&self.foreign.bytes),
            "client_object_sha256":digest(&self.client.bytes),
            "private_runtime_object_sha256":self.runtime.as_ref().map(|value|digest(&value.bytes)),
            "final_elf_sha256":digest(&self.final_elf),"observed_linker_order":self.trace,
            "observed_ELF_requirements":self.dependencies,
            "foreign_compile":invocation(&self.foreign.invocation),
            "client_compile":invocation(&self.client.invocation),
            "runtime_compile":self.runtime.as_ref().map(|value|invocation(&value.invocation)),
            "link":invocation(&self.link),"missing_prerequisites":MISSING,
            "execution_authorized":false,"target_executed":false,"driver_executable_mode_applied":false,
            "complete_input_use_attestation":false,"loader_or_provider_lookup_performed":false,
            "note":"GCC may itself create executable mode bits. This observer grants no permissions and returns no executable capability. Trace and final metadata are observed requirements, not approved providers."})
    }

    pub(crate) fn export(&self, destination: &Path) -> std::io::Result<()> {
        use std::os::unix::fs::DirBuilderExt as _;
        let mut builder = fs::DirBuilder::new();
        builder.mode(0o700).create(destination)?;
        let mut artifacts: Vec<(&str, &[u8])> = vec![
            ("program.o", self.requirements.object().bytes()),
            ("foreign.o", &self.foreign.bytes),
            ("foreign.c", &self.foreign.source),
            ("client.o", &self.client.bytes),
            ("client.c", &self.client.source),
            ("final.elf", &self.final_elf),
            ("linker.trace", &self.link.stdout),
            ("program-header.h", self.requirements.object().header().as_bytes()),
        ];
        if let Some(runtime) = &self.runtime {
            artifacts.extend([
                ("private-runtime.o", runtime.bytes.as_slice()),
                ("private-runtime.c", runtime.source.as_slice()),
                (
                    "private-runtime-header.h",
                    self.requirements
                        .object()
                        .program()
                        .source()
                        .runtime_abi()
                        .native_linux_x86_64_header(),
                ),
            ]);
        }
        let authority = self
            .requirements
            .object()
            .program()
            .source()
            .private_authority()
            .body_authority()
            .declaration_authority();
        artifacts.extend([
            (
                "library-header.h",
                authority
                    .header_bytes(self.library.library_id())
                    .ok_or_else(|| std::io::Error::other("original header missing"))?,
            ),
            (
                "library-policy.json",
                authority
                    .policy_bytes(self.library.library_id())
                    .ok_or_else(|| std::io::Error::other("original policy missing"))?,
            ),
        ]);
        let mut inventory = Vec::new();
        for (name, bytes) in artifacts {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(destination.join(name))?;
            file.write_all(bytes)?;
            file.sync_all()?;
            inventory
                .push(serde_json::json!({"path":name,"bytes":bytes.len(),"sha256":digest(bytes)}));
        }
        let mut report = self.report();
        report["artifacts"] = serde_json::json!(inventory);
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(destination.join("receipt.json"))?;
        file.write_all(&serde_json::to_vec_pretty(&report)?)?;
        file.sync_all()
    }
}

impl Failure {
    pub(crate) fn report(&self) -> serde_json::Value {
        serde_json::json!({"kind":"failed-private-compile-link-observation","execution_authorized":false,
            "target_executed":false,"missing_prerequisites":MISSING,
            "diagnostics":self.diagnostics.iter().map(Diagnostic::code).collect::<Vec<_>>(),
            "invocations":self.invocations.iter().map(invocation).collect::<Vec<_>>()})
    }
}
