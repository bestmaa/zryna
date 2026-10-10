use std::cmp::Ordering;
use zryna_diagnostics::Diagnostic;

pub(super) fn compare_diagnostics(left: &Diagnostic, right: &Diagnostic) -> Ordering {
    match (left.primary_span(), right.primary_span()) {
        (Some(left_span), Some(right_span)) => (
            left_span.file().index(),
            left_span.start(),
            left_span.end(),
            left.code(),
            left.message(),
            left.guidance(),
        )
            .cmp(&(
                right_span.file().index(),
                right_span.start(),
                right_span.end(),
                right.code(),
                right.message(),
                right.guidance(),
            )),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => (left.code(), left.message(), left.guidance()).cmp(&(
            right.code(),
            right.message(),
            right.guidance(),
        )),
    }
}
