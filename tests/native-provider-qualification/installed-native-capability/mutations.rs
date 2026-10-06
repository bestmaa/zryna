//! Closed test mutation transport; it issues no installation or source authority.

use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::Path,
    time::UNIX_EPOCH,
};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const MAX_FILE: u64 = 128 * 1024 * 1024;
const MAX_TOTAL: u64 = 512 * 1024 * 1024;
const MAX_ENTRIES: usize = 512;

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

fn ordinary(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return false;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return false;
        }
    }
    metadata.is_file() || metadata.is_dir()
}

fn no_links(path: &Path) -> io::Result<()> {
    if !path.is_absolute() || path.components().any(|c| c == std::path::Component::ParentDir) {
        return Err(invalid("test roots must be absolute"));
    }
    for item in path.ancestors() {
        if !ordinary(&fs::symlink_metadata(item)?) {
            return Err(invalid("test transport path has a link or special component"));
        }
    }
    Ok(())
}

fn absent(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
        Ok(_) => Err(invalid("mutation backup already exists")),
    }
}

fn state(metadata: &fs::Metadata) -> Value {
    let modified = metadata.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok());
    let mut value = json!({
        "bytes": metadata.len(), "file": metadata.is_file(), "directory": metadata.is_dir(),
        "modified_ns": modified.map(|t| t.as_nanos().to_string())
    });
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        value["device"] = json!(metadata.dev());
        value["inode"] = json!(metadata.ino());
        value["mode"] = json!(metadata.mode());
        value["links"] = json!(metadata.nlink());
        value["ctime"] = json!([metadata.ctime(), metadata.ctime_nsec()]);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        value["attributes"] = json!(metadata.file_attributes());
        value["creation_time"] = json!(metadata.creation_time());
        value["last_write_time"] = json!(metadata.last_write_time());
    }
    value
}

fn read_file(path: &Path) -> io::Result<Vec<u8>> {
    no_links(path)?;
    let before = fs::symlink_metadata(path)?;
    if !before.is_file() || before.len() > MAX_FILE {
        return Err(invalid("bounded ordinary mutation input required"));
    }
    let mut bytes = Vec::new();
    File::open(path)?.take(MAX_FILE + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_FILE || state(&before) != state(&fs::symlink_metadata(path)?) {
        return Err(invalid("mutation input changed during read"));
    }
    Ok(bytes)
}

fn tree(root: &Path) -> io::Result<Value> {
    no_links(root)?;
    let mut pending = vec![root.to_owned()];
    let mut records = serde_json::Map::new();
    let mut total = 0_u64;
    while let Some(path) = pending.pop() {
        if records.len() >= MAX_ENTRIES {
            return Err(invalid("test tree entry bound"));
        }
        let metadata = fs::symlink_metadata(&path)?;
        if !ordinary(&metadata) {
            return Err(invalid("test tree contains link or special entry"));
        }
        let key = path
            .strip_prefix(root)
            .map_err(|_| invalid("test tree escaped root"))?
            .to_str()
            .ok_or_else(|| invalid("test tree path is not UTF8"))?
            .replace('\\', "/");
        let mut record = state(&metadata);
        if metadata.is_file() {
            let bytes = read_file(&path)?;
            total =
                total.checked_add(bytes.len() as u64).ok_or_else(|| invalid("tree byte sum"))?;
            if total > MAX_TOTAL {
                return Err(invalid("test tree byte bound"));
            }
            record["sha256"] = json!(format!("{:x}", Sha256::digest(&bytes)));
        } else {
            let mut children = fs::read_dir(&path)?
                .map(|r| r.map(|e| e.path()))
                .collect::<io::Result<Vec<_>>>()?;
            children.sort();
            pending.extend(children.into_iter().rev());
        }
        records.insert(key, record);
    }
    Ok(Value::Object(records))
}

fn create_file(path: &Path, bytes: &[u8], image: bool) -> io::Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(if image { 0o700 } else { 0o600 });
    }
    #[cfg(windows)]
    let _ = image;
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.flush()
}

fn clone_tree(original: &Path, destination: &Path, image: Option<&Path>) -> io::Result<()> {
    let snapshot = tree(original)?;
    fs::create_dir(destination)?;
    for (name, record) in snapshot.as_object().ok_or_else(|| invalid("tree record"))? {
        if name.is_empty() {
            continue;
        }
        let relative = Path::new(name);
        let target = destination.join(relative);
        if record["directory"] == true {
            fs::create_dir(&target)?;
        } else {
            let bytes = read_file(&original.join(relative))?;
            create_file(&target, &bytes, image == Some(relative))?;
        }
    }
    Ok(())
}

fn prevented(error: &io::Error) -> bool {
    if error.kind() == io::ErrorKind::PermissionDenied {
        return true;
    }
    #[cfg(unix)]
    return matches!(error.raw_os_error(), Some(1 | 13 | 16 | 26));
    #[cfg(windows)]
    return matches!(error.raw_os_error(), Some(5 | 32 | 33));
    #[cfg(not(any(unix, windows)))]
    false
}

fn prevention(id: &str, error: io::Error, root: &Path, before: &Value) -> io::Result<Value> {
    if !prevented(&error) || &tree(root)? != before {
        return Err(error);
    }
    Ok(json!({"id": id, "effective": false, "prevention": {
        "operation": "OS denied mutation", "errno": error.raw_os_error(),
        "kind": format!("{:?}", error.kind()), "message": error.to_string(),
        "original_tree_unchanged": true, "exercised_rejection_credit": false
    }}))
}

fn replace_file(
    id: &str,
    root: &Path,
    relative: &Path,
    outside: &Path,
    image: bool,
    linked: bool,
) -> io::Result<Value> {
    let before = tree(root)?;
    let path = root.join(relative);
    let bytes = read_file(&path)?;
    let backup = outside.join("original-file");
    absent(&backup)?;
    if let Err(error) = fs::rename(&path, &backup) {
        return prevention(id, error, root, &before);
    }
    // Any error after rename is partial mutation/setup failure, never prevention.
    if linked {
        #[cfg(unix)]
        std::os::unix::fs::symlink(&backup, &path)?;
        #[cfg(windows)]
        std::os::windows::fs::symlink_file(&backup, &path)?;
    } else {
        create_file(&path, &bytes, image)?;
        if read_file(&path)? != bytes {
            return Err(invalid("same-byte file replacement differs"));
        }
    }
    let after = if linked {
        json!({"substituted_path": path, "path_entry": state(&fs::symlink_metadata(&path)?),
            "symlink_target": fs::read_link(&path)?,
            "full_tree_snapshot": "unavailable: deliberate link is rejected by snapshot guard"})
    } else {
        tree(root)?
    };
    Ok(json!({"id": id, "effective": true, "operation": if linked {
        "original moved and source symlink substituted"
    } else { "original moved and identical bytes newly created" },
        "backup": backup, "before": before, "after": after,
        "original_bytes_sha256": format!("{:x}", Sha256::digest(&bytes))}))
}

fn replace_tree(id: &str, root: &Path, outside: &Path, image: Option<&Path>) -> io::Result<Value> {
    let before = tree(root)?;
    let backup = outside.join("original-tree");
    absent(&backup)?;
    if let Err(error) = fs::rename(root, &backup) {
        return prevention(id, error, root, &before);
    }
    clone_tree(&backup, root, image)?;
    let after = tree(root)?;
    let old = before.as_object().ok_or_else(|| invalid("original tree record"))?;
    let new = after.as_object().ok_or_else(|| invalid("replacement tree record"))?;
    if old.len() != new.len()
        || old.iter().any(|(name, record)| {
            new.get(name).is_none_or(|next| {
                record["file"] != next["file"]
                    || record["directory"] != next["directory"]
                    || record["sha256"] != next["sha256"]
            })
        })
    {
        return Err(invalid("same-byte tree replacement differs"));
    }
    Ok(json!({"id": id, "effective": true, "operation": "same-byte owned tree replacement",
        "backup": backup, "before": before, "after": after}))
}

fn change_bytes(id: &str, root: &Path, relative: &Path, invalid_syntax: bool) -> io::Result<Value> {
    let before = tree(root)?;
    let path = root.join(relative);
    let mut bytes = read_file(&path)?;
    if invalid_syntax {
        bytes = b"export function main(: i32 { return ; }\n".to_vec();
    } else {
        bytes.push(b'\n');
    }
    let mut file = match OpenOptions::new().write(true).open(&path) {
        Ok(file) => file,
        Err(error) => return prevention(id, error, root, &before),
    };
    file.set_len(0)?;
    file.write_all(&bytes)?;
    file.flush()?;
    Ok(json!({"id": id, "effective": true, "operation": "changed existing bytes",
        "before": before, "after": tree(root)?}))
}

fn extra_file(id: &str, root: &Path, relative: &Path, bytes: &[u8]) -> io::Result<Value> {
    let before = tree(root)?;
    if let Err(error) = create_file(&root.join(relative), bytes, false) {
        return prevention(id, error, root, &before);
    }
    Ok(json!({"id": id, "effective": true, "operation": "new case-selected file",
        "before": before, "after": tree(root)?}))
}

/// Performs one closed, case-owned mutation; caller records Err as setup failure.
pub fn perform(
    id: &str,
    installation: &Path,
    source_root: &Path,
    outside: &Path,
) -> io::Result<Value> {
    for path in [installation, source_root, outside] {
        no_links(path)?;
        if !fs::symlink_metadata(path)?.is_dir() {
            return Err(invalid("mutation roots must be ordinary directories"));
        }
    }
    if outside.starts_with(installation)
        || outside.starts_with(source_root)
        || installation.starts_with(outside)
        || source_root.starts_with(outside)
        || installation.starts_with(source_root)
        || source_root.starts_with(installation)
        || outside.file_name() != Some(std::ffi::OsStr::new("mutation-backups"))
    {
        return Err(invalid("backups must be outside both test roots"));
    }
    let cli = Path::new(if cfg!(windows) {
        "bin/native-installation-proof.exe"
    } else {
        "bin/native-installation-proof"
    });
    let main = Path::new("packages/app/main.zry");
    match id {
        "descriptor-replaced-identical-bytes" => replace_file(
            id,
            installation,
            Path::new("metadata/native-provider.json"),
            outside,
            false,
            false,
        ),
        "license-replaced-identical-bytes" => {
            replace_file(id, installation, Path::new("LICENSE"), outside, false, false)
        }
        "executable-path-replaced-identical-bytes" => {
            replace_file(id, installation, cli, outside, true, false)
        }
        "installation-parent-replaced-identical-tree" => {
            replace_tree(id, installation, outside, Some(cli))
        }
        "source-parent-replaced" => replace_tree(id, source_root, outside, None),
        "source-replaced-identical-bytes" => {
            replace_file(id, source_root, main, outside, false, false)
        }
        "source-symlink-substitution" => replace_file(id, source_root, main, outside, false, true),
        "package-manifest-replaced-identical-bytes" => replace_file(
            id,
            source_root,
            Path::new("packages/app/zryna.package.json"),
            outside,
            false,
            false,
        ),
        "package-lock-byte-change" => {
            change_bytes(id, source_root, Path::new("packages/app/zryna.lock.json"), false)
        }
        "source-byte-change" => change_bytes(id, source_root, main, false),
        "late-mutated-invalid-syntax" => change_bytes(id, source_root, main, true),
        "callback-license-change" | "callback-error-license-change" => {
            change_bytes(id, installation, Path::new("LICENSE"), false)
        }
        "extra-file-after-capture" => {
            extra_file(id, installation, Path::new("unexpected"), b"extra\n")
        }
        "source-extra-file" => extra_file(
            id,
            source_root,
            Path::new("packages/app/extra.zry"),
            b"//observational unrelated source\n",
        ),
        _ => Err(invalid("unsupported independent mutation ID")),
    }
}
