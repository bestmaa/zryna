//! Reachable project ceilings with fixed per-file lexical and parser inventories.

pub type Files = Vec<(String, String)>;

fn chunks(items: usize, per_file: usize, mut source: impl FnMut(usize) -> String) -> Files {
    let mut files = Vec::new();
    for start in (0..items).step_by(per_file) {
        let text = (start..items.min(start + per_file)).map(&mut source).collect::<String>();
        files.push((format!("src/p{:02}.zry", files.len()), text));
    }
    files
}

pub fn cases(version: u32) -> Vec<(String, Files)> {
    let mut cases = Vec::new();
    let export = if version == 2 { "export " } else { "" };
    for count in [16_384, 16_385] {
        cases.push((
            format!("project-functions-{count}"),
            chunks(count, 4096, |index| format!("{export}function f{index}():i32{{}}\n")),
        ));
    }
    if version <= 3 {
        for extra in [false, true] {
            let body =
                if version == 2 { "{}\n".repeat(4096) } else { "{}return 1;\n".repeat(2048) };
            let mut files =
                chunks(16, 4, |index| format!("{export}function f{index}():i32{{{body}}}\n"));
            if extra {
                files.push((
                    "src/p04.zry".to_owned(),
                    format!("{export}function extra():i32{{return 1;}}"),
                ));
            }
            cases.push((format!("project-statements-{}", 65_536 + usize::from(extra)), files));
        }
    }
    if version >= 3 {
        for extra in [false, true] {
            let mut files = chunks(256, 64, |declaration| {
                let bindings = (0..256)
                    .map(|binding| format!("p{}", declaration * 256 + binding))
                    .collect::<Vec<_>>()
                    .join(",\n");
                format!("import{{{bindings}}}from'./other.zry';\n")
            });
            if extra {
                files
                    .push(("src/p04.zry".to_owned(), "import{extra}from'./other.zry';".to_owned()));
            }
            cases.push((format!("project-imported-names-{}", 65_536 + usize::from(extra)), files));
            let mut files = chunks(16, 4, |index| {
                format!("function f{index}():i32{{{}}}\n", "{}\n".repeat(4095))
            });
            if extra {
                files.push(("src/p04.zry".to_owned(), "function extra():i32{}".to_owned()));
            }
            cases.push((format!("project-blocks-{}", 65_536 + usize::from(extra)), files));
        }
    }
    if version == 4 {
        for count in [16_384, 16_385] {
            cases.push((
                format!("project-declarations-{count}"),
                chunks(count, 4096, |index| {
                    format!("interface D{index} extends ZrynaStruct{{x:i32;}}\n")
                }),
            ));
        }
        for extra in [false, true] {
            let mut files = chunks(16, 4, |index| {
                format!("function f{index}():i32{{{}}}\n", "1;\n".repeat(4096))
            });
            if extra {
                files.push(("src/p04.zry".to_owned(), "function extra():i32{1;}".to_owned()));
            }
            cases.push((format!("project-statements-{}", 65_536 + usize::from(extra)), files));
            let mut files = chunks(16, 4, |index| {
                format!(
                    "function f{index}():Vec<i32>{{return Vec<i32>([{}]);}}\n",
                    vec!["1"; 4096].join(",\n")
                )
            });
            if extra {
                files.push((
                    "src/p04.zry".to_owned(),
                    "function extra():Vec<i32>{return Vec<i32>([1]);}".to_owned(),
                ));
            }
            cases.push((
                format!("project-aggregate-operands-{}", 65_536 + usize::from(extra)),
                files,
            ));
        }
    }
    cases
}
