use super::{Errors, NormalizedSourcePath, UntrustedSpan};

pub(in crate::v4) fn contains_claim(parent: UntrustedSpan, child: UntrustedSpan) -> bool {
    parent.file == child.file && child.start >= parent.start && child.end <= parent.end
}

pub(in crate::v4) fn require_claim_contains(
    parent: UntrustedSpan,
    child: UntrustedSpan,
    path: &NormalizedSourcePath,
    errors: &mut Errors,
    label: &str,
) {
    if !contains_claim(parent, child) {
        errors.node(path, format!("{label} is outside its owner span"));
    }
}

pub(in crate::v4) fn require_claim_order(
    spans: &[UntrustedSpan],
    path: &NormalizedSourcePath,
    errors: &mut Errors,
    label: &str,
) {
    for pair in spans.windows(2) {
        if pair[0].file != pair[1].file || pair[1].start < pair[0].end {
            errors.node(path, format!("{label} are not in source order"));
        }
    }
}

pub(in crate::v4) fn checked_index<T>(values: &[T], id: u32) -> Option<&T> {
    usize::try_from(id).ok().and_then(|index| values.get(index))
}

pub(in crate::v4) fn check_sequence(
    owner: UntrustedSpan,
    children: &[UntrustedSpan],
    path: &NormalizedSourcePath,
    errors: &mut Errors,
    label: &str,
) {
    for child in children {
        require_claim_contains(owner, *child, path, errors, label);
    }
    require_claim_order(children, path, errors, label);
}
