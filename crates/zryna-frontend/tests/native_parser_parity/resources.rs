//! Production-limit source cases shared by frozen and live boundary evidence.

use std::fmt::Write;

pub fn cases(version: u32) -> Vec<(String, String)> {
    let export = if version == 2 { "export " } else { "" };
    let mut cases = Vec::new();
    for count in [255, 256, 257] {
        if version == 2 {
            cases.push((format!("diagnostics-{count}"), format!(
                "export function rejected(): i32 {{{}}}\nexport function retained(): i32 {{ return 2; }}",
                "return (1);".repeat(count),
            )));
        }
    }
    if version == 2 {
        let rejected = format!("return {}+(2);", ["1"; 7].join("+"));
        for extra in [4, 5] {
            cases.push((format!("rejected-expression-rollback-{}", 1260 + extra), format!(
                "export function rejected(): i32 {{{}{}}}\nexport function retained(): i32 {{return 2;}}",
                rejected.repeat(1260), "return 1+(2);".repeat(extra),
            )));
        }
    }
    for count in [256, 257] {
        let annotation = if version == 2 { "string" } else { "i32" };
        let parameters =
            (0..count).map(|index| format!("p{index}: {annotation}")).collect::<Vec<_>>().join(",");
        cases.push((
            format!("parameters-{count}"),
            format!("{export}function f({parameters}): i32 {{ return 1; }}"),
        ));
    }
    for count in [4096, 4097] {
        cases.push((
            format!("functions-{count}"),
            (0..count).fold(String::new(), |mut text, index| {
                writeln!(text, "{export}function f{index}(): i32 {{}}").expect("source text");
                text
            }),
        ));
        cases.push((
            format!("statements-{count}"),
            format!("{export}function f(): i32 {{{}}}", "return 1;".repeat(count)),
        ));
    }
    let expression = vec!["1"; 127].join("+");
    let base = format!("return {expression};").repeat(64);
    let final_expression = vec!["1"; 96].join("+");
    for extra in [0, 1] {
        cases.push((
            format!("expressions-{}", 16384 + extra),
            format!(
                "{export}function f(): i32 {{{base}return {final_expression};{}}}",
                "return 1;".repeat(1 + extra),
            ),
        ));
    }
    if version >= 3 {
        for count in [4096, 4097] {
            cases.push((
                format!("locals-{count}"),
                format!(
                    "function f(): i32 {{{}}}",
                    (0..count).fold(String::new(), |mut text, index| {
                        writeln!(text, "const p{index}: i32 = 1;").expect("local source");
                        text
                    }),
                ),
            ));
            cases.push((
                format!("imports-{count}"),
                (0..count).fold(String::new(), |mut text, index| {
                    writeln!(text, "import {{p{index}}} from \"./other.zry\";")
                        .expect("import source");
                    text
                }),
            ));
        }
        for count in [256, 257] {
            cases.push((
                format!("call-arguments-{count}"),
                format!("function f(): i32 {{return g({});}}", vec!["1"; count].join(",")),
            ));
            cases.push((
                format!("import-bindings-{count}"),
                format!(
                    "import {{{}}} from \"./other.zry\";",
                    (0..count).map(|index| format!("p{index}")).collect::<Vec<_>>().join(","),
                ),
            ));
        }
        for count in [4095, 4096] {
            cases.push((
                format!("blocks-{}", count + 1),
                format!("function f(): i32 {{{}}}", "{}".repeat(count)),
            ));
        }
    }
    if version == 4 {
        v4_cases(&mut cases);
    }
    cases
}

fn v4_cases(cases: &mut Vec<(String, String)>) {
    for count in [4096, 4097] {
        cases.push((
            format!("declarations-{count}"),
            (0..count).fold(String::new(), |mut text, index| {
                writeln!(text, "interface D{index} extends ZrynaStruct {{x: i32;}}")
                    .expect("declaration source");
                text
            }),
        ));
    }
    for count in [1024, 1025] {
        cases.push((
            format!("match-arms-{count}"),
            format!(
                "function f(x: E): i32 {{return match(x, {{{}}});}}",
                (0..count)
                    .map(|index| format!("\"E.A{index}\": () => 1"))
                    .collect::<Vec<_>>()
                    .join(","),
            ),
        ));
    }
    for count in [127, 128] {
        cases.push((
            format!("type-depth-{}", count + 1),
            format!(
                "function f(x: {}i32{}): i32 {{return 1;}}",
                "Vec<".repeat(count),
                ">".repeat(count),
            ),
        ));
    }
    for length in [1_048_576, 1_048_577] {
        cases.push((
            format!("array-length-{length}"),
            format!("function f(x: FixedArray<i32, {length}>): i32 {{return 1;}}"),
        ));
    }
    for count in [1024, 1025] {
        cases.push((
            format!("members-{count}"),
            format!(
                "interface Data extends ZrynaStruct {{{}}}",
                (0..count).fold(String::new(), |mut text, index| {
                    write!(text, "p{index}: i32;").expect("member source");
                    text
                }),
            ),
        ));
        cases.push((
            format!("initializers-{count}"),
            format!(
                "function f(): Data {{return Data({{{}}});}}",
                (0..count).map(|index| format!("p{index}: 1")).collect::<Vec<_>>().join(","),
            ),
        ));
    }
    for count in [4096, 4097] {
        cases.push((
            format!("array-elements-{count}"),
            format!(
                "function f(): Vec<i32> {{return Vec<i32>([{}]);}}",
                vec!["1"; count].join(","),
            ),
        ));
    }
}
