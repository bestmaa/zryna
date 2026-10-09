use super::*;
use crate::native::{self, ArtifactOutputRoot};
use sha2::Sha256;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

pub(super) struct Fixture {
    pub(super) requirements: HandleLinkRequirements,
    pub(super) runtime: super::super::super::CompiledObject,
    pub(super) root: ArtifactOutputRoot,
    path: PathBuf,
}

impl Fixture {
    pub(super) fn new() -> Self {
        let capture = native::native_c_v0::test_capture::reference();
        let ir = zryna_native_c_ir::lower(&capture.sources, &capture.authority)
            .expect("original IR issuer");
        let mir = zryna_native_mir::native_c_v0::lower(&ir).expect("original MIR issuer");
        let symbol =
            &mir.functions().find(|f| f.name() == "copied").expect("byte fixture").entry().symbol;
        let object = zryna_backend_native::native_c_v0::resources::emit_byte_entries(
            &mir,
            &[symbol],
            zryna_backend_native::select_object_target(zryna_backend_native::NATIVE_OBJECT_TARGET)
                .expect("exact target"),
        )
        .expect("actual audited program object");
        let requirements =
            native::native_c_v0::resource_identity::handle_link_requirements(&object)
                .expect("original requirements");
        let path = std::env::temp_dir().join(format!(
            "zryna-runtime-object-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(path.join(".zryna/out")).expect("private fixture root");
        let root = ArtifactOutputRoot::for_workspace(&path).expect("retained output root");
        let tools = native::discover_linux_native_toolchain(native::NativeProcessLimits::default())
            .expect("existing fixture tool capability");
        let source = requirements.private_runtime_source().expect("original rendered runtime");
        let digest: [u8; 32] = Sha256::digest(source).into();
        assert_eq!(Some(&digest), requirements.private_runtime_source_sha256());
        let runtime = super::super::super::compile_object(&root, source, &tools)
            .expect("actual separate runtime compilation");
        Self { requirements, runtime, root, path }
    }

    pub(super) fn foreign(&self) -> (CapturedForeignLibrary, super::super::super::CompiledObject) {
        use crate::native::native_c_v0::foreign_library::{
            ForeignLibraryInput, capture_foreign_library,
        };
        let source = format!(
            "{}\n{}",
            std::str::from_utf8(native::native_c_v0::test_capture::HEADER)
                .expect("original header"),
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/src/native/native_c_v0/foreign_library/tests/library.c"
            ))
        );
        let foreign = super::super::super::compile_object(
            &self.root,
            source.as_bytes(),
            self.runtime.tools(),
        )
        .expect("original foreign fixture");
        let authority = self
            .requirements
            .object()
            .program()
            .source()
            .private_authority()
            .body_authority()
            .declaration_authority();
        let hash: [u8; 32] = Sha256::digest(foreign.bytes()).into();
        let library = capture_foreign_library(
            &self.requirements,
            &ForeignLibraryInput {
                library_id: "fixture-c-v0@0",
                header_bytes: authority.header_bytes("fixture-c-v0@0").expect("original header"),
                policy_bytes: authority.policy_bytes("fixture-c-v0@0").expect("original policy"),
                object_bytes: foreign.bytes(),
                object_size: foreign.bytes().len(),
                object_sha256: &hash,
                dependency_symbols: &["free", "malloc"],
            },
        )
        .expect("original captured foreign object");
        (library, foreign)
    }

    pub(super) fn empty(&self) {
        assert_eq!(
            fs::read_dir(self.path.join(".zryna/out")).expect("actual stage inventory").count(),
            0
        );
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.path).expect("remove only this disposable test fixture");
    }
}
