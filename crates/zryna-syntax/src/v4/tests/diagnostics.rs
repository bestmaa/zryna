use super::*;
use crate::v4::verify::is_sensitive;

#[test]
fn sensitive_names_and_diagnostics_are_deterministic_and_bounded() {
    assert!(is_sensitive("__proto__"));
    assert!(is_sensitive("prototype"));
    assert!(is_sensitive("constructor"));
    assert!(!is_sensitive("ordinary"));
    let authority = sources(SOURCE);
    let mut value = raw();
    value.diagnostics = (0..MAX_PROVIDER_DIAGNOSTICS)
        .rev()
        .map(|index| RawProviderDiagnostic {
            code: format!("P{index:03}"),
            severity: Severity::Warning,
            location: RawDiagnosticLocation::Global,
            message: "message".into(),
            guidance: "guidance".into(),
        })
        .collect();
    let verified = verify_snapshot(value, &authority).unwrap();
    let rendered = verified.diagnostics().iter().map(ToString::to_string).collect::<Vec<_>>();
    let mut sorted = rendered.clone();
    sorted.sort();
    assert_eq!(rendered, sorted);
}
