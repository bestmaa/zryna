use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

use sha2::{Digest as _, Sha256};
#[cfg(unix)]
use std::{path::Path, time::Duration};
use zryna_source::NormalizedSourcePath;

#[cfg(unix)]
use super::{ClosureFrontendV3, NativeImportFrontend};
#[cfg(unix)]
use crate::module_closure::discover_module_closure_with_clock;
use crate::{ModuleClosureError, WorkspaceSourceRoot, discover_native_import_only_closure};

struct Workspace(PathBuf);

impl Workspace {
    fn new(label: &str) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir()
            .join(format!("zryna-native-import-closure-{}-{label}-{id}", std::process::id()));
        fs::create_dir(&path).expect("unique source workspace");
        Self(path)
    }

    fn write(&self, name: &str, source: &str) {
        let path = self.0.join(name);
        fs::create_dir_all(path.parent().expect("source parent")).expect("source directory");
        fs::write(path, source).expect("source bytes");
    }

    fn root(&self) -> WorkspaceSourceRoot {
        WorkspaceSourceRoot::capture(&self.0).expect("retained source root")
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).expect("owned test workspace cleanup");
    }
}

fn entry() -> NormalizedSourcePath {
    NormalizedSourcePath::new("main.zry").expect("entry path")
}

fn rejected_code(error: ModuleClosureError) -> String {
    let ModuleClosureError::Rejected(diagnostics) = error else {
        panic!("native failure must retain driver rejection diagnostics");
    };
    diagnostics.into_iter().next().expect("native rejection diagnostic").code().to_owned()
}

#[test]
fn native_imports_seal_the_existing_driver_graph_and_source_map() {
    let main = concat!(
        "import { left as first } from './lib/left.zry';\n",
        "import { right } from './lib/right.zry';\n"
    );
    let left = "import { leaf } from './leaf.zry';\n";
    let right = "import { leaf as result } from './leaf.zry';\n";
    let first = Workspace::new("first");
    for (path, source) in
        [("main.zry", main), ("lib/left.zry", left), ("lib/right.zry", right), ("lib/leaf.zry", "")]
    {
        first.write(path, source);
    }
    let closure =
        discover_native_import_only_closure(&first.root(), entry()).expect("native closure");
    assert!(closure.syntax().is_bound_to(closure.sources()));
    assert_eq!(closure.modules().len(), 4);
    assert_eq!(closure.edges().len(), 4);
    assert!(closure.syntax().files().iter().all(|file| file.functions().is_empty()));
    let paths = closure.modules().iter().map(|module| module.path().as_str()).collect::<Vec<_>>();
    assert_eq!(paths, ["lib/leaf.zry", "lib/left.zry", "lib/right.zry", "main.zry"]);
    let main_record = closure
        .modules()
        .iter()
        .find(|record| record.path().as_str() == "main.zry")
        .expect("main module");
    let expected_hash: [u8; 32] = Sha256::digest(main.as_bytes()).into();
    assert_eq!(main_record.source_sha256(), &expected_hash);

    let reordered = Workspace::new("reordered");
    for (path, source) in
        [("lib/leaf.zry", ""), ("lib/right.zry", right), ("lib/left.zry", left), ("main.zry", main)]
    {
        reordered.write(path, source);
    }
    let again = discover_native_import_only_closure(&reordered.root(), entry())
        .expect("reordered native closure");
    assert_eq!(closure.graph_sha256(), again.graph_sha256());
    assert_eq!(closure.modules(), again.modules());

    first.write("main.zry", "// changed after sealing\n");
    let file = closure.sources().file_id(&entry()).expect("sealed entry");
    assert_eq!(closure.sources().source(file).expect("sealed source").text(), main);
}

#[test]
fn native_parser_rejects_trailing_source_instead_of_omitting_an_import() {
    let workspace = Workspace::new("omission");
    workspace
        .write("main.zry", "import { a } from './a.zry'; export function f(): i32 { return 1; }");
    workspace.write("a.zry", "");
    assert_eq!(
        rejected_code(
            discover_native_import_only_closure(&workspace.root(), entry())
                .expect_err("functionless candidate must reject trailing declaration")
        ),
        "ZRYNA-F2002"
    );

    let lexical = Workspace::new("lexical-error");
    lexical.write("main.zry", "import @;");
    assert_eq!(
        rejected_code(
            discover_native_import_only_closure(&lexical.root(), entry())
                .expect_err("lexical error must retain its diagnostic")
        ),
        "ZRYNA-F1501"
    );
}

#[test]
fn native_closure_rejects_portable_collision_and_cycle() {
    let collision = Workspace::new("collision");
    collision.write(
        "main.zry",
        concat!("import { a } from './Dep.zry';\n", "import { b } from './dep.zry';\n"),
    );
    assert_eq!(
        rejected_code(
            discover_native_import_only_closure(&collision.root(), entry())
                .expect_err("portable path collision must reject")
        ),
        "ZRYNA-D3005"
    );

    let cycle = Workspace::new("cycle");
    cycle.write("main.zry", "import { dep } from './dep.zry';\n");
    cycle.write("dep.zry", "import { main } from './main.zry';\n");
    assert_eq!(
        rejected_code(
            discover_native_import_only_closure(&cycle.root(), entry())
                .expect_err("native cycle must reject")
        ),
        "ZRYNA-D3007"
    );
}

#[cfg(unix)]
struct ReplaceDuringFinalAnalysis<'a> {
    source: &'a Path,
    calls: AtomicUsize,
}

#[cfg(unix)]
impl ClosureFrontendV3 for ReplaceDuringFinalAnalysis<'_> {
    fn minimum_analysis_timeout(&self) -> Duration {
        Duration::ZERO
    }

    fn analyze(
        &self,
        sources: &zryna_source::SourceMap,
        timeout: Duration,
    ) -> Result<zryna_frontend::syntax_v3::ProjectSyntaxSnapshot, ModuleClosureError> {
        let result = NativeImportFrontend.analyze(sources, timeout);
        if self.calls.fetch_add(1, Ordering::Relaxed) == 2 {
            fs::write(self.source, "import { changed } from './dep.zry';\n")
                .expect("concurrent source substitution");
        }
        result
    }
}

#[cfg(unix)]
#[test]
fn native_closure_rejects_mutation_during_final_analysis() {
    let workspace = Workspace::new("substitution");
    workspace.write("main.zry", "import { dep } from './dep.zry';\n");
    workspace.write("dep.zry", "");
    let source = workspace.0.join("main.zry");
    let provider = ReplaceDuringFinalAnalysis { source: &source, calls: AtomicUsize::new(0) };
    let result = discover_module_closure_with_clock(
        &workspace.root(),
        entry(),
        &provider,
        std::time::Instant::now,
    );
    assert_eq!(rejected_code(result.expect_err("substitution must reject")), "ZRYNA-D3004");
    assert_eq!(provider.calls.load(Ordering::Relaxed), 3);
}
