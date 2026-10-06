//! Source-first complete-key counts, reviewed before any producer admission change.
use super::tests::claim;
use std::fmt::Write;
use zryna_ir::generic_v1::Failure;

#[test]
fn genuine_copy_enum_key_credits_stop_at_the_frozen_first_extra_and_recover() {
    // Six snapshot deltas are0,0,q_yes,q_yes,q_yes+q_no,q_yes+q_no.
    // Capture3+L per arm and join4+2L retain every key byte before allocation.
    // These literals come from independently reviewed source operations, not emitted IR.
    for (ty, initial, yes, no, constant, increment, last, last_units, extra_units) in [
        (
            "Option<i32>",
            "Option.some<i32>(0)",
            "Option.some<i32>(7)",
            "Option.some<i32>(9)",
            33_692usize,
            42usize,
            30usize,
            1_029_030usize,
            1_063_982usize,
        ),
        (
            "Option<i32>",
            "Option.some<i32>(0)",
            "Option.none<i32>()",
            "Option.some<i32>(9)",
            33_688,
            36,
            30,
            1_026_300,
            1_061_068,
        ),
        (
            "Option<i32>",
            "Option.some<i32>(0)",
            "Option.some<i32>(7)",
            "Option.none<i32>()",
            33_690,
            36,
            30,
            1_026_360,
            1_061_130,
        ),
        (
            "Result<i32,bool>",
            "Result.ok<i32,bool>(0)",
            "Result.ok<i32,bool>(7)",
            "Result.err<i32,bool>(true)",
            45_712,
            42,
            22,
            1_015_366,
            1_062_002,
        ),
    ] {
        for (branches, units, accepted) in
            [(last, last_units, true), (last + 1, extra_units, false), (1, constant, true)]
        {
            assert_eq!(units, branches * constant + increment * branches * (branches - 1) / 2);
            assert_eq!(accepted, units <= 1_048_576);
            let mut source = String::from("export function root(flag:bool):i32 {");
            for i in 0..400 {
                write!(source, "let v{i}:{ty} ={initial};").expect("source");
            }
            for _ in 0..branches {
                write!(source, "if(flag){{v0={yes};}}else{{v0={no};}}").expect("source");
            }
            source.push_str("return 0;}");
            let source = source.replace(';', ";\n");
            let result = claim(&[("main.zry", &source)]);
            if accepted {
                result.expect("independent exact/recovery complete-key credit");
            } else {
                let Failure::Diagnostics(errors) = result.expect_err("independent first extra")
                else {
                    panic!("diagnostic");
                };
                assert_eq!(errors[0].code, "ZRYNA-M7201");
            }
        }
    }
}

#[test]
fn concrete_copy_enum_parameter_ceiling_counts_the_split_changed_union() {
    for (ty, initial, replacement) in [
        ("Option<i32>", "Option.some<i32>(0)", "Option.none<i32>()"),
        ("Result<i32,bool>", "Result.ok<i32,bool>(0)", "Result.err<i32,bool>(true)"),
    ] {
        for (places, split, accepted) in
            [(256, false, true), (257, false, false), (257, true, false), (1, false, true)]
        {
            let mut source = String::from("export function root(flag:bool):i32 {");
            for i in 0..places {
                write!(source, "let v{i}:{ty} ={initial};").expect("source");
            }
            source.push_str("if(flag){");
            for i in 0..places {
                if !split || i % 2 == 0 {
                    write!(source, "v{i}={replacement};").expect("source");
                }
            }
            source.push_str("}else{");
            for i in 0..places {
                if !split || i % 2 == 1 {
                    write!(source, "v{i}={replacement};").expect("source");
                }
            }
            source.push_str("}return 0;}");
            let source = source.replace(';', ";\n");
            let result = claim(&[("main.zry", &source)]);
            if accepted {
                result.expect("unchanged exact256/recovery");
            } else {
                let Failure::Diagnostics(errors) =
                    result.expect_err("first extra sparse/union place")
                else {
                    panic!("diagnostic");
                };
                assert_eq!(errors[0].code, "ZRYNA-M7201");
                assert!(errors[0].message.contains("finite Copy branch join parameter ceiling"));
            }
        }
    }
}
