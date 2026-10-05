//! Frozen source assembly and native candidate helpers; no producer-derived expected DTOs.

use serde_json::Value;
use std::path::PathBuf;
use zryna_source::SourceFileInput;

pub(super) fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("repository")
        .to_path_buf()
}

pub(super) fn corpus() -> Value {
    serde_json::from_str(include_str!("../../../../tests/provider-conformance-v5/corpus.json"))
        .expect("frozen corpus")
}

pub(super) fn inputs(case: &Value) -> Vec<SourceFileInput> {
    case["files"]
        .as_array()
        .expect("files")
        .iter()
        .map(|file| {
            let text = file["fragments"]
                .as_array()
                .expect("fragments")
                .iter()
                .map(|fragment| {
                    fragment["text"].as_str().map_or_else(
                        || {
                            std::fs::read_to_string(
                                root().join(fragment["source"].as_str().expect("source")),
                            )
                            .expect("frozen source")
                        },
                        str::to_owned,
                    )
                })
                .collect::<String>();
            SourceFileInput { path: file["path"].as_str().expect("path").into(), text }
        })
        .collect()
}
