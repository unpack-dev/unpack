use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use unpack_core::{Compiler, CompilerOptions, DependencyKind, Entry, Error};

#[tokio::test]
async fn make_constructs_static_esm_module_graph() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    write(
        temp.path().join("src/index.ts"),
        r#"
            import "./side-effect";
            import { value } from "./dep";
            export { thing } from "./reexport";
            export * from "./star";
            console.log(value);
        "#,
    )?;
    write(temp.path().join("src/dep.ts"), "export const value = 1;")?;
    write(
        temp.path().join("src/reexport.js"),
        r#"export { value as thing } from "./dep";"#,
    )?;
    write(temp.path().join("src/star.ts"), r#"export * from "./dep";"#)?;
    write(
        temp.path().join("src/side-effect.jsx"),
        "globalThis.sideEffect = true;",
    )?;

    let compiler = Compiler::new(CompilerOptions::new(
        temp.path(),
        vec![Entry::new("main", "./src/index")],
    ));

    let compilation = compiler.run().await?;
    let graph = compilation.module_graph();

    assert_eq!(compilation.errors(), []);
    assert_eq!(compilation.entries().len(), 1);
    assert_eq!(graph.modules().len(), 5);

    let resources = relative_resources(temp.path(), graph);
    assert!(resources.contains("src/index.ts"));
    assert!(resources.contains("src/dep.ts"));
    assert!(resources.contains("src/reexport.js"));
    assert!(resources.contains("src/star.ts"));
    assert!(resources.contains("src/side-effect.jsx"));

    let entry = compilation.entries()[0];
    let outgoing = graph
        .outgoing_connections(entry)
        .map(|connection| {
            (
                connection.dependency.kind,
                connection.dependency.request.as_str(),
            )
        })
        .collect::<HashSet<_>>();

    assert_eq!(
        outgoing,
        HashSet::from([
            (DependencyKind::StaticImport, "./side-effect"),
            (DependencyKind::StaticImport, "./dep"),
            (DependencyKind::StaticExport, "./reexport"),
            (DependencyKind::StaticExport, "./star"),
        ])
    );

    Ok(())
}

#[tokio::test]
async fn make_deduplicates_shared_module_identity() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    write(
        temp.path().join("index.js"),
        r#"
            import "./a";
            import "./b";
        "#,
    )?;
    write(temp.path().join("a.js"), r#"import "./shared";"#)?;
    write(temp.path().join("b.js"), r#"import "./shared";"#)?;
    write(temp.path().join("shared.js"), "export const shared = true;")?;

    let compiler = Compiler::new(CompilerOptions::new(
        temp.path(),
        vec![Entry::new("main", "./index")],
    ));
    let compilation = compiler.run().await?;
    let graph = compilation.module_graph();

    let shared = graph
        .modules()
        .iter()
        .find(|module| module.identity().resource.ends_with("shared.js"))
        .expect("shared module should exist")
        .id();

    assert_eq!(graph.modules().len(), 4);
    assert_eq!(graph.incoming_connections(shared).count(), 2);

    Ok(())
}

#[tokio::test]
async fn make_records_parse_errors() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    write(temp.path().join("index.js"), "import {")?;

    let compiler = Compiler::new(CompilerOptions::new(
        temp.path(),
        vec![Entry::new("main", "./index")],
    ));
    let mut compilation = compiler.create_compilation();

    let error = compilation.make().await.expect_err("make should fail");

    assert!(matches!(error, Error::Parse { .. }));
    assert_eq!(compilation.errors().len(), 1);
    assert!(matches!(compilation.errors()[0], Error::Parse { .. }));

    Ok(())
}

fn write(path: PathBuf, source: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, source)
}

fn relative_resources(root: &Path, graph: &unpack_core::ModuleGraph) -> HashSet<String> {
    let root = fs::canonicalize(root).expect("fixture root should exist");
    graph
        .modules()
        .iter()
        .map(|module| {
            let resource =
                fs::canonicalize(&module.identity().resource).expect("resource should exist");
            resource
                .strip_prefix(&root)
                .expect("resource should be inside fixture root")
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect()
}
