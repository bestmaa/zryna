use std::borrow::Cow;

use wasm_encoder::{ComponentSection as _, CustomSection, InstanceSection, RawSection};
use wasmparser::{Parser, Payload};

use super::{Candidate, range, rewrite};

#[test]
fn independently_valid_embedded_core_changes_reject_retained_binding() {
    let candidate = Candidate::new("environment-match");
    for target in [0, 1] {
        let original =
            if target == 0 { candidate.artifact.storage() } else { candidate.artifact.language() };
        let mut core = original.to_vec();
        let custom =
            CustomSection { name: Cow::Borrowed("binding-probe"), data: Cow::Borrowed(&[0]) };
        wasm_encoder::Section::append_to(&custom, &mut core);
        let mut index = 0;
        let bytes = rewrite(candidate.artifact.bytes(), |chunk, component| {
            if chunk.id != 1 {
                return false;
            }
            let selected = index == target;
            index += 1;
            if selected {
                component.section(&RawSection { id: 1, data: &core });
            }
            selected
        });
        assert_eq!(index, 2);
        candidate.reject_valid(&bytes, "ZRYNA-W4104");
    }
}

#[test]
fn extra_unused_core_module_and_instance_reject_exact_execution_graph() {
    let candidate = Candidate::new("pure-entry");
    let mut bytes = candidate.artifact.bytes().to_vec();
    RawSection { id: 1, data: candidate.artifact.storage() }.append_to_component(&mut bytes);
    candidate.reject_valid(&bytes, "ZRYNA-W4104");
    let mut bytes = candidate.artifact.bytes().to_vec();
    let mut instances = InstanceSection::new();
    instances.export_items([] as [(&str, wasm_encoder::ExportKind, u32); 0]);
    instances.append_to_component(&mut bytes);
    candidate.reject_valid(&bytes, "ZRYNA-W4104");
}

#[test]
fn exact_language_module_base_translates_every_audited_trap_site_without_engine() {
    let candidate = Candidate::new("environment-match");
    let base = candidate.audit(candidate.artifact.bytes()).expect("retained component graph");
    let modules = Parser::new(0)
        .parse_all(candidate.artifact.bytes())
        .filter_map(|payload| {
            if let Payload::ModuleSection { unchecked_range, .. } =
                payload.expect("embedded modules")
            {
                Some(unchecked_range)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert!(modules.len() == 2 && base == modules[1].start);
    assert_eq!(
        &candidate.artifact.bytes()[range(modules[1].clone())],
        candidate.artifact.language()
    );
    let local = super::super::super::language_audit::audit(
        candidate.artifact.language(),
        candidate.artifact.program(),
    )
    .expect("independent language trap locations");
    assert!(local.len() == candidate.artifact.traps().len() && local.len() == 5);
    for (local, translated) in local.iter().zip(candidate.artifact.traps()) {
        assert_eq!(translated.function_index(), local.function_index());
        assert_eq!(translated.identity(), local.identity());
        assert_eq!(
            translated.module_offset(),
            local.module_offset().checked_add(base).expect("bounded offset")
        );
        let position =
            usize::try_from(translated.module_offset()).expect("bounded component position");
        assert_eq!(candidate.artifact.bytes()[position], 0x00);
    }
}
