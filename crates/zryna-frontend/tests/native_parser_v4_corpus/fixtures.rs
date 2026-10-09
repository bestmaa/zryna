//! Checked-in M3 sources in portable source-map order.

use std::{fs, path::Path};

pub(crate) fn sources() -> Vec<(String, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/m3-fixtures");
    let mut directories = vec![root.clone()];
    let mut files = Vec::new();
    while let Some(directory) = directories.pop() {
        for entry in fs::read_dir(directory).expect("M3 fixture directory") {
            let path = entry.expect("M3 fixture entry").path();
            if path.is_dir() {
                directories.push(path);
            } else if path.extension().is_some_and(|extension| extension == "zry") {
                let relative = path.strip_prefix(&root).expect("M3 source relative path");
                let portable = relative
                    .components()
                    .map(|part| part.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/");
                files.push((
                    format!("src/{portable}"),
                    fs::read_to_string(&path).expect("UTF-8 fixture"),
                ));
            }
        }
    }
    files.sort_by(|left, right| left.0.cmp(&right.0));
    assert_eq!(files.len(), 97, "complete M3 source fixture count");
    files
}
