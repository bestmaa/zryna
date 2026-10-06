//! Compile-time purpose and source binding; runtime environment cannot issue this authority.

const PREFIX: &[u8] = b"ZRYNA-NATIVE-INSTALLATION-INTERNAL-V1\0";
const LENGTH: usize = PREFIX.len() + 64 + 1 + 40 + 1 + 40 + 1;
static MARKER: [u8; LENGTH] = marker();

pub(super) struct Binding {
    pub(super) descriptor_sha256: &'static str,
    pub(super) commit: &'static str,
    pub(super) tree: &'static str,
}

pub(super) fn compiled() -> Option<Binding> {
    let _ = std::hint::black_box(&MARKER);
    Some(Binding {
        descriptor_sha256: option_env!("ZRYNA_PRIVATE_NATIVE_INSTALLATION_SHA256")?,
        commit: option_env!("ZRYNA_PRIVATE_NATIVE_SOURCE_COMMIT")?,
        tree: option_env!("ZRYNA_PRIVATE_NATIVE_SOURCE_TREE")?,
    })
}

const fn marker() -> [u8; LENGTH] {
    let mut bytes = [0; LENGTH];
    let mut index = 0;
    while index < PREFIX.len() {
        bytes[index] = PREFIX[index];
        index += 1;
    }
    let values = [
        (option_env!("ZRYNA_PRIVATE_NATIVE_INSTALLATION_SHA256"), 64),
        (option_env!("ZRYNA_PRIVATE_NATIVE_SOURCE_COMMIT"), 40),
        (option_env!("ZRYNA_PRIVATE_NATIVE_SOURCE_TREE"), 40),
    ];
    let mut field = 0;
    while field < values.len() {
        let (value, length) = values[field];
        if let Some(value) = value {
            let value = value.as_bytes();
            assert!(value.len() == length, "private native binding has an invalid length");
            let mut offset = 0;
            while offset < length {
                let byte = value[offset];
                assert!(
                    (byte >= b'0' && byte <= b'9') || (byte >= b'a' && byte <= b'f'),
                    "private native binding must use lowercase hexadecimal"
                );
                bytes[index + offset] = byte;
                offset += 1;
            }
        }
        index += length + 1;
        field += 1;
    }
    bytes
}
