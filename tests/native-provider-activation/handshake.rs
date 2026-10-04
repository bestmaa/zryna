//! Real existing worker admission and independent hostile handshake mutations.

use std::{env, error::Error, ffi::OsString, path::Path};

use crate::worker::{PROVIDER, VERSION};
use zryna_frontend::{
    FrontendCapabilities, ProviderExpectation, ProviderExpectationV3, ProviderExpectationV4,
    WorkerError, WorkerFailure, WorkerFrontend, WorkerFrontendV3, WorkerFrontendV4, WorkerLimits,
    WorkerLimitsV3, WorkerLimitsV4, WorkerSpec, WorkerSpecV3, WorkerSpecV4, syntax_v2, syntax_v3,
    syntax_v4,
};
use zryna_source::{SourceFileInput, SourceMap};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn args(protocol: u32, fault: &str, marker: &Path) -> Vec<OsString> {
    vec!["--worker".into(), protocol.to_string().into(), fault.into(), marker.into()]
}

pub fn v2(
    sources: &SourceMap,
    cwd: &Path,
    fault: &str,
    marker: &Path,
) -> std::result::Result<syntax_v2::ProjectSyntaxSnapshot, WorkerError> {
    let spec = WorkerSpec::new(
        env::current_exe().expect("own absolute executable"),
        args(2, fault, marker),
        cwd,
        ProviderExpectation::new(
            PROVIDER,
            VERSION,
            2,
            FrontendCapabilities { module_resolution: false, semantic_diagnostics: false },
        )?,
        WorkerLimits::default(),
    )?;
    WorkerFrontend::new(spec).analyze_verified(sources)
}

pub fn v3(
    sources: &SourceMap,
    cwd: &Path,
    fault: &str,
    marker: &Path,
) -> std::result::Result<syntax_v3::ProjectSyntaxSnapshot, WorkerError> {
    let spec = WorkerSpecV3::new(
        env::current_exe().expect("own absolute executable"),
        args(3, fault, marker),
        cwd,
        ProviderExpectationV3::new(PROVIDER, VERSION)?,
        WorkerLimitsV3::default(),
    )?;
    WorkerFrontendV3::new(spec).analyze_verified_v3(sources)
}

pub fn v4(
    sources: &SourceMap,
    cwd: &Path,
    fault: &str,
    marker: &Path,
) -> std::result::Result<syntax_v4::ProjectSyntaxSnapshot, WorkerError> {
    let spec = WorkerSpecV4::new(
        env::current_exe().expect("own absolute executable"),
        args(4, fault, marker),
        cwd,
        ProviderExpectationV4::new(PROVIDER, VERSION)?,
        WorkerLimitsV4::default(),
    )?;
    WorkerFrontendV4::new(spec).analyze_verified_v4(sources)
}

pub fn smoke(cwd: &Path) -> Result<Vec<String>> {
    let sources = SourceMap::build(vec![SourceFileInput {
        path: "src/main.zry".into(),
        text: "// 😀 exact UTF-8 and CRLF\r\nexport function value(): i32 { return 7; }\r\n".into(),
    }])
    .map_err(|_| "fixture map rejected")?;
    let mut passed = Vec::new();
    for protocol in 2..=4 {
        let mut faults = vec![
            ("identity", WorkerFailure::ProviderIdentity),
            ("version", WorkerFailure::ProviderVersion),
            ("protocol", WorkerFailure::ProviderProtocol),
            ("module", WorkerFailure::ProviderCapabilities),
            ("semantic", WorkerFailure::ProviderCapabilities),
            ("extra", WorkerFailure::InvalidResponse),
            ("id", WorkerFailure::InvalidResponse),
            ("snapshot-version", WorkerFailure::SnapshotVerification),
            ("snapshot-path", WorkerFailure::SnapshotVerification),
            ("frame", WorkerFailure::InvalidResponse),
        ];
        if protocol >= 3 {
            faults.push(("control", WorkerFailure::ProviderCapabilities));
        }
        if protocol == 4 {
            faults.push(("ownership", WorkerFailure::ProviderCapabilities));
        }
        let marker = cwd.join(format!("v{protocol}-accepted.marker"));
        match protocol {
            2 => {
                let syntax = v2(&sources, cwd, "none", &marker)?;
                assert!(syntax.is_bound_to(&sources));
                assert!(syntax.diagnostics().is_empty());
            }
            3 => {
                let syntax = v3(&sources, cwd, "none", &marker)?;
                assert!(syntax.is_bound_to(&sources));
                assert!(syntax.diagnostics().is_empty());
            }
            _ => {
                let syntax = v4(&sources, cwd, "none", &marker)?;
                assert!(syntax.is_bound_to(&sources));
                assert!(syntax.diagnostics().is_empty());
            }
        }
        assert!(marker.is_file(), "successful admission must send actual source analysis");
        passed.push(format!("v{protocol}:accepted"));
        for (fault, expected) in faults {
            let marker = cwd.join(format!("v{protocol}-{fault}.marker"));
            let error = match protocol {
                2 => v2(&sources, cwd, fault, &marker).err(),
                3 => v3(&sources, cwd, fault, &marker).err(),
                _ => v4(&sources, cwd, fault, &marker).err(),
            }
            .ok_or("hostile provider was accepted")?;
            assert_eq!(error.failure(), expected, "v{protocol} {fault}: {error:?}");
            if fault.starts_with("snapshot-") || fault == "frame" {
                assert!(marker.is_file(), "malformed candidate tested after valid handshake");
            } else {
                assert!(!marker.exists(), "source disclosed after failed handshake");
            }
            passed.push(format!("v{protocol}:{fault}"));
        }
    }
    Ok(passed)
}
