use super::{LOCAL_SYSTEM, verify};

const OWNER: &[u8] = &[1, 1, 0, 0, 0, 0, 0, 5, 21, 0, 0, 0];
const EVERYONE: &[u8] = &[1, 1, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0];

fn descriptor(entries: &[(u8, u8, u32, &[u8])]) -> Vec<u8> {
    let mut bytes = vec![1, 0, 4, 128];
    bytes.extend(20_u32.to_le_bytes());
    bytes.extend([0; 8]);
    bytes.extend(32_u32.to_le_bytes());
    bytes.extend(OWNER);
    let size = 8 + entries.iter().map(|(_, _, _, sid)| 8 + sid.len()).sum::<usize>();
    bytes.extend([2, 0]);
    bytes.extend(u16::try_from(size).expect("bounded ACL fixture").to_le_bytes());
    bytes.extend(u16::try_from(entries.len()).expect("bounded count").to_le_bytes());
    bytes.extend([0; 2]);
    for (kind, flags, mask, sid) in entries {
        bytes.extend([*kind, *flags]);
        bytes.extend(u16::try_from(8 + sid.len()).expect("bounded ACE").to_le_bytes());
        bytes.extend(mask.to_le_bytes());
        bytes.extend(*sid);
    }
    bytes
}

#[test]
fn exact_owner_and_system_are_admitted_but_other_readers_or_acl_editors_are_denied() {
    let private = descriptor(&[(0, 0, u32::MAX, OWNER), (0, 0, u32::MAX, LOCAL_SYSTEM)]);
    verify(&private, OWNER).expect("explicit private ACL");
    for mask in [1, 0x8000_0000, 0x4000_0000, 0x0004_0000, u32::MAX] {
        assert!(verify(&descriptor(&[(0, 0, mask, EVERYONE)]), OWNER).is_err());
    }
    verify(&descriptor(&[(1, 0, u32::MAX, EVERYONE)]), OWNER)
        .expect("deny ACE cannot widen access");
    verify(&descriptor(&[(0, 8, u32::MAX, EVERYONE)]), OWNER)
        .expect("inherit-only does not apply to this file");
    assert!(verify(&private, EVERYONE).is_err(), "the exact current caller must own the file");
}

#[test]
fn malformed_null_unknown_and_first_extra_security_inputs_fail_closed() {
    let private = descriptor(&[(0, 0, 1, OWNER)]);
    for offset in [0, 1, 19, 33, u32::MAX] {
        let mut changed = private.clone();
        changed[16..20].copy_from_slice(&offset.to_le_bytes());
        assert!(verify(&changed, OWNER).is_err());
    }
    for kind in [2, 5, 9, 11, 255] {
        assert!(verify(&descriptor(&[(kind, 0, 1, OWNER)]), OWNER).is_err());
    }
    for flags in [0x20, 0x40, 0x80] {
        assert!(verify(&descriptor(&[(0, flags, 1, OWNER)]), OWNER).is_err());
    }
    for length in 0..private.len() {
        assert!(verify(&private[..length], OWNER).is_err());
    }
    let exact = descriptor(&vec![(0, 0, 1, OWNER); 256]);
    verify(&exact, OWNER).expect("exact ACE bound");
    assert!(verify(&descriptor(&vec![(0, 0, 1, OWNER); 257]), OWNER).is_err());
    let mut extra = private;
    extra.resize(65_536, 0);
    verify(&extra, OWNER).expect("exact security byte bound");
    extra.resize(65_537, 0);
    assert!(verify(&extra, OWNER).is_err());
}
