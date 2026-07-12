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

    let dep_module = graph
        .modules()
        .iter()
        .find(|module| module.identity().resource.ends_with("dep.ts"))
        .expect("dep module should exist");
    assert_eq!(
        dep_module.exports_info().is_export_provided("value"),
        Some(true)
    );

    let entry = compilation.entries()[0];
    let outgoing = graph
        .outgoing_connections(entry)
        .map(|connection| {
            (
                connection.dependency.kind(),
                connection
                    .dependency
                    .request()
                    .expect("dependency should have request"),
            )
        })
        .collect::<HashSet<_>>();

    assert_eq!(
        outgoing,
        HashSet::from([
            (DependencyKind::StaticImport, "./side-effect"),
            (DependencyKind::StaticImport, "./dep"),
            (DependencyKind::StaticImport, "./reexport"),
            (DependencyKind::StaticImport, "./star"),
            (DependencyKind::StaticExport, "./reexport"),
            (DependencyKind::StaticExport, "./star"),
        ])
    );

    Ok(())
}

#[tokio::test]
async fn make_records_dynamic_import_split_points() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    write(
        temp.path().join("src/index.js"),
        r#"
            import "./eager";
            export async function loadFeature() {
                return await import("./feature");
            }
            export const templateImport = () => import(`./template`);
        "#,
    )?;
    write(
        temp.path().join("src/eager.js"),
        "export const eager = true;",
    )?;
    write(
        temp.path().join("src/feature.js"),
        r#"import "./shared"; export const feature = true;"#,
    )?;
    write(
        temp.path().join("src/template.js"),
        "export const template = true;",
    )?;
    write(
        temp.path().join("src/shared.js"),
        "export const shared = true;",
    )?;

    let compiler = Compiler::new(CompilerOptions::new(
        temp.path(),
        vec![Entry::new("main", "./src/index")],
    ));

    let compilation = compiler.run().await?;
    let graph = compilation.module_graph();

    assert_eq!(compilation.errors(), []);
    assert_eq!(graph.modules().len(), 5);

    let resources = relative_resources(temp.path(), graph);
    assert!(resources.contains("src/index.js"));
    assert!(resources.contains("src/eager.js"));
    assert!(resources.contains("src/feature.js"));
    assert!(resources.contains("src/template.js"));
    assert!(resources.contains("src/shared.js"));

    let entry = compilation.entries()[0];
    let outgoing = graph
        .outgoing_connections(entry)
        .map(|connection| {
            (
                connection.dependency.kind(),
                connection
                    .dependency
                    .request()
                    .expect("dependency should have request"),
            )
        })
        .collect::<HashSet<_>>();

    assert_eq!(
        outgoing,
        HashSet::from([
            (DependencyKind::StaticImport, "./eager"),
            (DependencyKind::DynamicImport, "./feature"),
            (DependencyKind::DynamicImport, "./template"),
        ])
    );

    let feature = graph
        .modules()
        .iter()
        .find(|module| module.identity().resource.ends_with("feature.js"))
        .expect("feature module should exist")
        .handle();
    let feature_outgoing = graph
        .outgoing_connections(feature)
        .map(|connection| {
            (
                connection.dependency.kind(),
                connection
                    .dependency
                    .request()
                    .expect("dependency should have request"),
            )
        })
        .collect::<HashSet<_>>();

    assert_eq!(
        feature_outgoing,
        HashSet::from([(DependencyKind::StaticImport, "./shared")])
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
        .handle();

    assert_eq!(graph.modules().len(), 4);
    assert_eq!(graph.incoming_connections(shared).count(), 2);

    Ok(())
}

#[tokio::test]
async fn make_deduplicates_mixed_import_identity() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    write(
        temp.path().join("index.js"),
        r#"
            import "./feature";
            import("./feature");
        "#,
    )?;
    write(
        temp.path().join("feature.js"),
        "export const feature = true;",
    )?;

    let compiler = Compiler::new(CompilerOptions::new(
        temp.path(),
        vec![Entry::new("main", "./index")],
    ));
    let compilation = compiler.run().await?;
    let graph = compilation.module_graph();

    let feature = graph
        .modules()
        .iter()
        .find(|module| module.identity().resource.ends_with("feature.js"))
        .expect("feature module should exist")
        .handle();
    let incoming = graph
        .incoming_connections(feature)
        .map(|connection| connection.dependency.kind())
        .collect::<HashSet<_>>();

    assert_eq!(graph.modules().len(), 2);
    assert_eq!(
        incoming,
        HashSet::from([DependencyKind::StaticImport, DependencyKind::DynamicImport])
    );

    Ok(())
}

#[tokio::test]
async fn chunk_split_rewires_chunk_groups() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    write(temp.path().join("index.js"), r#"import "./dep";"#)?;
    write(temp.path().join("dep.js"), "export const dep = true;")?;

    let compiler = Compiler::new(CompilerOptions::new(
        temp.path(),
        vec![Entry::new("main", "./index")],
    ));
    let compilation = compiler.run().await?;
    let mut chunk_graph = compilation.chunk_graph().clone();

    let entry_group = chunk_graph.entrypoints()[0];
    let original_chunk = chunk_graph.chunk_groups()[entry_group.index()].chunks()[0];
    let split_chunk = chunk_graph
        .split_chunk(original_chunk, "split", "split.js")
        .expect("chunk should split");

    assert!(
        chunk_graph.chunk_groups()[entry_group.index()]
            .chunks()
            .contains(&split_chunk)
    );
    assert!(
        chunk_graph
            .chunk(split_chunk)
            .expect("split chunk should exist")
            .groups()
            .contains(&entry_group)
    );

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
    let compilation = compiler.run().await?;

    assert_eq!(compilation.errors().len(), 1);
    assert!(matches!(compilation.errors()[0], Error::Parse { .. }));
    assert!(
        compilation
            .assets()
            .iter()
            .any(|asset| asset.filename == "main.js")
    );

    let main = compilation
        .assets()
        .iter()
        .find(|asset| asset.filename == "main.js")
        .expect("main asset should exist");
    assert!(main.source.contains("throw new Error"));

    Ok(())
}

#[tokio::test]
async fn make_rejects_context_module_dynamic_imports() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    write(
        temp.path().join("index.js"),
        r#"
            const request = "./feature";
            import(request);
        "#,
    )?;

    let compiler = Compiler::new(CompilerOptions::new(
        temp.path(),
        vec![Entry::new("main", "./index")],
    ));
    let compilation = compiler.run().await?;

    assert_eq!(compilation.errors().len(), 1);
    assert!(matches!(
        compilation.errors()[0],
        Error::UnsupportedDynamicImport { .. }
    ));
    assert!(
        compilation
            .assets()
            .iter()
            .any(|asset| asset.filename == "main.js")
    );

    Ok(())
}

#[tokio::test]
async fn make_records_resolve_errors_as_failed_modules() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    write(temp.path().join("index.js"), r#"import "./missing";"#)?;

    let compiler = Compiler::new(CompilerOptions::new(
        temp.path(),
        vec![Entry::new("main", "./index")],
    ));
    let compilation = compiler.run().await?;

    assert_eq!(compilation.errors().len(), 1);
    assert!(matches!(compilation.errors()[0], Error::Resolve { .. }));

    let main = compilation
        .assets()
        .iter()
        .find(|asset| asset.filename == "main.js")
        .expect("main asset should exist");
    assert!(main.source.contains("failed to resolve"));
    assert!(main.source.contains("throw new Error"));

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
