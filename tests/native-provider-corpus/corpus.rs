use crate::Context;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{fs, path::Path};
use zryna_source::{NormalizedSourcePath, SourceFileInput, SourceMap};

pub fn registry(context: &Context, name: &str) -> Value {
    let bytes = fs::read(context.root.join(format!("tests/{name}-conformance-v1.json")))
        .expect("frozen registry");
    let value: Value = serde_json::from_slice(&bytes).expect("registry JSON");
    assert_eq!(value["schemaVersion"], 1);
    value
}

pub fn text(context: &Context, path: &str) -> String {
    fs::read_to_string(context.root.join(path)).expect("original UTF-8 source bytes")
}

pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn checked_text(context: &Context, fixture: &Value) -> String {
    let source = text(context, fixture["path"].as_str().expect("fixture path"));
    assert_eq!(digest(source.as_bytes()), fixture["sha256"].as_str().expect("source hash"));
    source
}

pub fn path(name: &str) -> NormalizedSourcePath {
    NormalizedSourcePath::new(name).expect("normalized source path")
}

pub fn sources(files: &[(String, String)]) -> SourceMap {
    SourceMap::build(
        files
            .iter()
            .rev()
            .map(|(path, text)| SourceFileInput { path: path.clone(), text: text.clone() })
            .collect(),
    )
    .expect("same immutable canonical source map")
}

pub fn entry(sources: &SourceMap, name: &str) -> zryna_source::FileId {
    sources.file_id(&path(name)).expect("entry is in immutable source map")
}

pub fn fixture_files(root: &Path) -> Vec<String> {
    fn visit(root: &Path, directory: &Path, names: &mut Vec<String>) {
        for item in fs::read_dir(directory).expect("complete fixture inventory") {
            let file = item.expect("fixture entry").path();
            let metadata = fs::symlink_metadata(&file).expect("fixture metadata");
            assert!(!metadata.file_type().is_symlink(), "real frozen fixtures");
            if metadata.is_dir() {
                visit(root, &file, names);
            } else if file.extension().is_some_and(|extension| extension == "zry") {
                names.push(
                    file.strip_prefix(root)
                        .expect("contained fixture")
                        .components()
                        .map(|part| part.as_os_str().to_str().expect("UTF-8 path"))
                        .collect::<Vec<_>>()
                        .join("/"),
                );
            }
        }
    }
    let mut names = vec![];
    visit(root, root, &mut names);
    names.sort();
    names
}

pub fn install(context: &Context, label: &str, files: &[(String, String)]) -> std::path::PathBuf {
    let root = context.output.join(label);
    fs::create_dir(&root).expect("create-only isolated fixture workspace");
    for (name, text) in files {
        let destination = root.join(name);
        fs::create_dir_all(destination.parent().expect("source parent")).expect("source directory");
        fs::write(destination, text.as_bytes()).expect("exact source bytes");
    }
    root
}

pub fn m3_files(context: &Context, registry: &Value, fixture: &Value) -> Vec<(String, String)> {
    let mut files = vec![("src/main.zry".to_owned(), checked_text(context, fixture))];
    if let Some(id) = fixture["dependency"].as_str() {
        let dependency = registry["fixtures"]
            .as_array()
            .expect("frozen fixtures")
            .iter()
            .find(|fixture| fixture["id"] == id)
            .expect("exact contextual dependency");
        files.push(("src/math.zry".to_owned(), checked_text(context, dependency)));
    }
    files
}
