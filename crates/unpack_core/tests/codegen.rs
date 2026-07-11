use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use unpack_core::{
    AsyncBlockOrigin, AsyncDependenciesBlockId, ChunkGroupKind, Compiler, CompilerOptions, Entry,
};

#[tokio::test]
async fn seal_orchestrates_post_make_phases() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    write(temp.path().join("src/index.js"), "export const value = 42;")?;
    let compiler = Compiler::new(CompilerOptions::new(
        temp.path(),
        vec![Entry::new("main", "./src/index")],
    ));
    let mut compilation = compiler.create_compilation();

    compilation.make().await?;
    assert!(compilation.assets().is_empty());

    compilation.seal();

    assert_eq!(compilation.errors(), []);
    assert!(
        compilation
            .assets()
            .iter()
            .any(|asset| asset.filename == "main.js")
    );

    Ok(())
}

#[tokio::test]
#[should_panic(expected = "Render IDs must be assigned before code generation")]
async fn code_generation_rejects_an_unsealed_compilation() {
    let temp = tempfile::tempdir().expect("temp directory should be created");
    write(temp.path().join("src/index.js"), "export const value = 42;")
        .expect("fixture should be written");
    let compiler = Compiler::new(CompilerOptions::new(
        temp.path(),
        vec![Entry::new("main", "./src/index")],
    ));
    let mut compilation = compiler.create_compilation();

    compilation.make().await.expect("make should complete");
    compilation.build_chunk_graph();
    compilation.code_generation();
}

// Ported from webpack 5.108.1:
// test/configCases/optimization/named-modules
//
// Webpack's case establishes that named IDs remain recognizable. This variant
// also covers Unpack's named-ID contract for deterministic collision handling.
#[tokio::test]
async fn assigns_unique_readable_names_to_colliding_async_chunks()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    write(
        temp.path().join("src/index.js"),
        r#"
            export const loadDash = () => import("./a-b");
            export const loadUnderscore = () => import("./a_b");
        "#,
    )?;
    write(
        temp.path().join("src/a-b.js"),
        "export const value = 'dash';",
    )?;
    write(
        temp.path().join("src/a_b.js"),
        "export const value = 'underscore';",
    )?;

    let options = CompilerOptions::new(temp.path(), vec![Entry::new("main", "./src/index")]);
    let first = Compiler::new(options.clone()).run().await?;
    let second = Compiler::new(options).run().await?;

    let first_assets = first
        .assets()
        .iter()
        .map(|asset| (asset.filename.clone(), asset.source.clone()))
        .collect::<Vec<_>>();
    let second_assets = second
        .assets()
        .iter()
        .map(|asset| (asset.filename.clone(), asset.source.clone()))
        .collect::<Vec<_>>();
    assert_eq!(first_assets, second_assets);

    let async_filenames = first
        .assets()
        .iter()
        .filter(|asset| asset.filename.ends_with(".js") && asset.filename != "main.js")
        .map(|asset| asset.filename.as_str())
        .collect::<Vec<_>>();
    assert_eq!(async_filenames.len(), 2);
    assert_ne!(async_filenames[0], async_filenames[1]);
    assert!(
        async_filenames
            .iter()
            .all(|filename| filename.starts_with("src_a_b_js"))
    );

    Ok(())
}

// Ported from webpack 5.108.1:
// test/statsCases/named-chunks-plugin-async
// Collision precedence follows lib/ids/NamedChunkIdsPlugin.js.
#[tokio::test]
async fn entry_names_take_precedence_over_colliding_async_names()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    write(
        temp.path().join("src/index.js"),
        "export const load = () => import('./feature');",
    )?;
    write(
        temp.path().join("src/feature.js"),
        "export const value = 'feature';",
    )?;

    let compilation = Compiler::new(CompilerOptions::new(
        temp.path(),
        vec![Entry::new("src_feature_js", "./src/index")],
    ))
    .run()
    .await?;
    let entry = compilation
        .assets()
        .iter()
        .find(|asset| asset.filename == "src_feature_js.js")
        .expect("entry asset should keep its configured filename");

    assert!(entry.source.contains("  \"src_feature_js\": 1"));
    assert!(compilation.assets().iter().any(|asset| {
        asset.filename.starts_with("src_feature_js-") && asset.filename.ends_with(".js")
    }));

    Ok(())
}

// Ported from webpack 5.108.1:
// test/configCases/module-name/different-issuers-for-same-module
//
// Unpack does not expose loaders yet, so the public seam exercises the same
// identity rule through resource queries and fragments.
#[tokio::test]
async fn module_render_ids_distinguish_queries_and_fragments()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    write(
        temp.path().join("src/index.js"),
        r#"
            import { value as alpha } from "./shared?one#alpha";
            import { value as beta } from "./shared?two#beta";
            export const values = [alpha, beta];
        "#,
    )?;
    write(
        temp.path().join("src/shared.js"),
        "export const value = 'shared';",
    )?;

    let compilation = Compiler::new(CompilerOptions::new(
        temp.path(),
        vec![Entry::new("main", "./src/index")],
    ))
    .run()
    .await?;
    let main = compilation
        .assets()
        .iter()
        .find(|asset| asset.filename == "main.js")
        .expect("main asset should exist");

    assert!(
        main.source.contains(r#""./src/shared.js?one#alpha""#),
        "main asset did not contain the first assigned ID:\n{}",
        main.source
    );
    assert!(
        main.source.contains(r#""./src/shared.js?two#beta""#),
        "main asset did not contain the second assigned ID:\n{}",
        main.source
    );
    assert!(!main.source.contains("??"));
    assert!(!main.source.contains("##"));

    Ok(())
}

// Ported from webpack 5.108.1:
// test/configCases/optimization/named-modules
#[tokio::test]
async fn renders_module_factories_in_render_id_order() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    write(
        temp.path().join("src/index.js"),
        r#"
            import { z } from "./z";
            import { a } from "./a";
            export const value = a + z;
        "#,
    )?;
    write(temp.path().join("src/a.js"), "export const a = 'a';")?;
    write(temp.path().join("src/z.js"), "export const z = 'z';")?;

    let compilation = Compiler::new(CompilerOptions::new(
        temp.path(),
        vec![Entry::new("main", "./src/index")],
    ))
    .run()
    .await?;
    let main = compilation
        .assets()
        .iter()
        .find(|asset| asset.filename == "main.js")
        .expect("main asset should exist");
    let a = main.source.find(r#""./src/a.js": "#).unwrap();
    let index = main.source.find(r#""./src/index.js": "#).unwrap();
    let z = main.source.find(r#""./src/z.js": "#).unwrap();

    assert!(a < index && index < z);

    Ok(())
}

#[tokio::test]
async fn produces_stable_static_async_and_sourcemap_outputs()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    write(
        temp.path().join("src/index.js"),
        r#"
            import { eager } from "./eager";

            export async function load() {
                const feature = await import("./feature");
                return [eager, feature.value];
            }
        "#,
    )?;
    write(
        temp.path().join("src/eager.js"),
        "export const eager = 'eager';",
    )?;
    write(
        temp.path().join("src/feature.js"),
        "export const value = 'feature';",
    )?;

    let compilation = Compiler::new(CompilerOptions::new(
        temp.path(),
        vec![Entry::new("main", "./src/index")],
    ))
    .run()
    .await?;
    let fingerprints = compilation
        .assets()
        .iter()
        .map(|asset| {
            (
                asset.filename.as_str(),
                asset.source.len(),
                fnv1a64(asset.source.as_bytes()),
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(
        fingerprints,
        [
            ("main.js", 3166, 11891285027332120831),
            ("main.js.map", 480, 5745374754696300241),
            ("src_feature_js.js", 398, 7014656015713497901),
            ("src_feature_js.js.map", 164, 2414748877879059404)
        ]
    );

    Ok(())
}

#[tokio::test]
async fn emits_node_require_chunks_for_dynamic_import() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    write(
        temp.path().join("src/index.js"),
        r#"
            import { eager } from "./eager";

            export async function loadFeature() {
                const mod = await import("./feature");
                return [eager, mod.feature, mod.shared, mod.alpha, mod.beta];
            }
        "#,
    )?;
    write(
        temp.path().join("src/eager.js"),
        r#"export const eager = "eager";"#,
    )?;
    write(
        temp.path().join("src/feature.js"),
        r#"
            import { shared } from "./shared";
            export const feature = "feature";
            export { shared };
            export * from "./extra";
            export const alpha = "feature-alpha";
        "#,
    )?;
    write(
        temp.path().join("src/shared.js"),
        r#"export const shared = "shared";"#,
    )?;
    write(
        temp.path().join("src/extra.js"),
        r#"
            export const alpha = "alpha";
            export const beta = "beta";
        "#,
    )?;

    let compiler = Compiler::new(CompilerOptions::new(
        temp.path(),
        vec![Entry::new("main", "./src/index")],
    ));
    let compilation = compiler.run().await?;

    assert_eq!(compilation.errors(), []);
    assert_eq!(compilation.assets().len(), 4);
    assert!(
        compilation
            .assets()
            .iter()
            .any(|asset| asset.filename == "main.js")
    );
    assert!(
        compilation
            .assets()
            .iter()
            .any(|asset| asset.filename == "src_feature_js.js")
    );
    assert!(
        compilation
            .assets()
            .iter()
            .any(|asset| asset.filename == "main.js.map")
    );
    assert!(
        compilation
            .assets()
            .iter()
            .any(|asset| asset.filename == "src_feature_js.js.map")
    );

    let chunk_graph = compilation.chunk_graph();
    assert_eq!(chunk_graph.entrypoints().len(), 1);
    assert!(chunk_graph.chunk_groups().iter().any(|group| {
        matches!(group.kind(), ChunkGroupKind::Async) && group.parents().len() == 1
    }));

    let main = compilation
        .assets()
        .iter()
        .find(|asset| asset.filename == "main.js")
        .expect("main asset should exist");
    assert!(main.source.contains("__webpack_require__.e"));
    assert!(main.source.contains("__webpack_require__.f.require"));
    assert!(main.source.contains("__webpack_require__.u"));
    assert!(
        main.source
            .contains("__webpack_require__.m = __webpack_modules__")
    );
    assert!(main.source.contains("__webpack_require__.o"));
    assert!(main.source.contains("__webpack_require__.d"));
    assert!(main.source.contains("__webpack_require__.r"));
    assert!(
        main.source
            .contains(r#"__webpack_require__.e("src_feature_js").then(__webpack_require__.bind"#)
    );
    assert!(
        main.source
            .contains(r#"require("./" + __webpack_require__.u(chunkId))"#)
    );
    let register_factory = main
        .source
        .find("__webpack_require__.m[moduleId] = moreModules[moduleId]")
        .expect("Require Chunk Loading must register payload factories");
    let execute_runtime = main
        .source
        .find("if(runtime) runtime(__webpack_require__)")
        .expect("Require Chunk Loading must execute an optional payload runtime");
    let mark_loaded = main
        .source
        .find("installedChunks[chunkIds[i]] = 1")
        .expect("Require Chunk Loading must mark payload chunk IDs loaded");
    assert!(register_factory < execute_runtime && execute_runtime < mark_loaded);
    assert!(main.source.contains("./src/index.js"));
    assert!(main.source.contains("//# sourceMappingURL=main.js.map"));

    let main_map = compilation
        .assets()
        .iter()
        .find(|asset| asset.filename == "main.js.map")
        .expect("main sourcemap should exist");
    assert!(main_map.source.contains(r#""file":"main.js""#));
    assert!(main_map.source.contains("./src/index.js"));
    assert!(main_map.source.contains("sourcesContent"));

    if !node_available() {
        return Ok(());
    }

    let out_dir = temp.path().join("dist");
    fs::create_dir_all(&out_dir)?;
    write_assets(&out_dir, compilation.assets())?;
    let output = Command::new("node")
        .arg("-e")
        .arg(
            r#"
            const entry = require("./main.js");
            entry.loadFeature()
              .then(value => {
                console.log(JSON.stringify(value));
              })
              .catch(error => {
                console.error(error && error.stack || error);
                process.exit(1);
              });
        "#,
        )
        .current_dir(&out_dir)
        .output()?;

    assert!(
        output.status.success(),
        "node failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout)?.trim(),
        r#"["eager","feature","shared","feature-alpha","beta"]"#
    );

    Ok(())
}

#[tokio::test]
async fn disables_sourcemap_assets_when_configured() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    write(
        temp.path().join("src/index.js"),
        r#"
            export const value = 42;
        "#,
    )?;

    let mut options = CompilerOptions::new(temp.path(), vec![Entry::new("main", "./src/index")]);
    options.sourcemap = false;
    let compiler = Compiler::new(options);
    let compilation = compiler.run().await?;

    assert_eq!(compilation.errors(), []);
    assert_eq!(compilation.assets().len(), 1);
    assert!(
        compilation
            .assets()
            .iter()
            .any(|asset| asset.filename == "main.js")
    );
    assert!(
        compilation
            .assets()
            .iter()
            .all(|asset| !asset.filename.ends_with(".map"))
    );

    let main = compilation
        .assets()
        .iter()
        .find(|asset| asset.filename == "main.js")
        .expect("main asset should exist");
    assert!(!main.source.contains("sourceMappingURL"));

    Ok(())
}

#[tokio::test]
async fn preserves_import_live_bindings() -> Result<(), Box<dyn std::error::Error>> {
    if !node_available() {
        return Ok(());
    }

    let temp = tempfile::tempdir()?;
    write(
        temp.path().join("src/index.js"),
        r#"
            import { value, setValue } from "./dep";

            export function read() {
                return value;
            }

            export function object() {
                return { value };
            }

            export function property(obj) {
                return obj.value;
            }

            export function computed(obj) {
                return obj[value];
            }

            export function write(next) {
                setValue(next);
            }
        "#,
    )?;
    write(
        temp.path().join("src/dep.js"),
        r#"
            export let value = 1;
            export function setValue(next) {
                value = next;
            }
        "#,
    )?;

    let compiler = Compiler::new(CompilerOptions::new(
        temp.path(),
        vec![Entry::new("main", "./src/index")],
    ));
    let compilation = compiler.run().await?;
    let out_dir = temp.path().join("dist");
    fs::create_dir_all(&out_dir)?;
    write_assets(&out_dir, compilation.assets())?;

    let output = Command::new("node")
        .arg("-e")
        .arg(
            r#"
            const entry = require("./main.js");
            const before = entry.read();
            const objectBefore = entry.object().value;
            entry.write(42);
            const after = entry.read();
            const objectAfter = entry.object().value;
            const member = entry.property({ value: "member" });
            const computed = entry.computed({ 42: "computed" });
            console.log(JSON.stringify([before, objectBefore, after, objectAfter, member, computed]));
        "#,
        )
        .current_dir(&out_dir)
        .output()?;

    assert!(
        output.status.success(),
        "node failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout)?.trim(),
        r#"[1,1,42,42,"member","computed"]"#
    );

    Ok(())
}

#[tokio::test]
async fn reused_async_chunk_contains_modules_needed_by_each_entry()
-> Result<(), Box<dyn std::error::Error>> {
    if !node_available() {
        return Ok(());
    }

    let temp = tempfile::tempdir()?;
    write(
        temp.path().join("src/a.js"),
        r#"
            import { shared } from "./shared";

            export async function load() {
                const mod = await import("./feature");
                return [shared, mod.feature, mod.shared];
            }

            export async function loadExtra() {
                return (await import("./extra")).extra;
            }
        "#,
    )?;
    write(
        temp.path().join("src/b.js"),
        r#"
            export async function load() {
                const mod = await import("./feature");
                return [mod.feature, mod.shared];
            }

            export async function loadExtra() {
                return (await import("./extra")).extra;
            }
        "#,
    )?;
    write(
        temp.path().join("src/feature.js"),
        r#"
            import { shared } from "./shared";
            export const feature = "feature";
            export { shared };
        "#,
    )?;
    write(
        temp.path().join("src/shared.js"),
        r#"export const shared = "shared";"#,
    )?;
    write(
        temp.path().join("src/extra.js"),
        r#"export const extra = "extra";"#,
    )?;

    let compiler = Compiler::new(CompilerOptions::new(
        temp.path(),
        vec![Entry::new("a", "./src/a"), Entry::new("b", "./src/b")],
    ));
    let compilation = compiler.run().await?;
    let async_asset = compilation
        .assets()
        .iter()
        .find(|asset| asset.filename == "src_feature_js.js")
        .expect("shared Async Chunk asset should exist");
    assert!(async_asset.source.contains(r#""./src/feature.js": "#));
    assert!(
        async_asset.source.contains(r#""./src/shared.js": "#),
        "a module available on only one parent path must remain in the shared Async Chunk"
    );

    let one_parent = Compiler::new(CompilerOptions::new(
        temp.path(),
        vec![Entry::new("a", "./src/a")],
    ))
    .run()
    .await?;
    let one_parent_async_asset = one_parent
        .assets()
        .iter()
        .find(|asset| asset.filename == "src_feature_js.js")
        .expect("one parent should emit its Async Chunk");
    assert!(
        !one_parent_async_asset
            .source
            .contains(r#""./src/shared.js": "#),
        "a module available on the only parent path should be excluded"
    );

    let reversed = Compiler::new(CompilerOptions::new(
        temp.path(),
        vec![Entry::new("b", "./src/b"), Entry::new("a", "./src/a")],
    ))
    .run()
    .await?;
    let async_assets = compilation
        .assets()
        .iter()
        .filter(|asset| {
            asset.filename != "a.js" && asset.filename != "b.js" && asset.filename.ends_with(".js")
        })
        .map(|asset| (asset.filename.as_str(), asset.source.as_str()))
        .collect::<Vec<_>>();
    let reversed_async_assets = reversed
        .assets()
        .iter()
        .filter(|asset| {
            asset.filename != "a.js" && asset.filename != "b.js" && asset.filename.ends_with(".js")
        })
        .map(|asset| (asset.filename.as_str(), asset.source.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(
        async_assets, reversed_async_assets,
        "multiple shared Async targets must be stable across parent discovery order"
    );

    let out_dir = temp.path().join("dist");
    fs::create_dir_all(&out_dir)?;
    write_assets(&out_dir, compilation.assets())?;

    let output = Command::new("node")
        .arg("-e")
        .arg(
            r#"
            Promise.all([
              require("./a.js").load(),
              require("./b.js").load()
            ])
              .then(value => {
                console.log(JSON.stringify(value));
              })
              .catch(error => {
                console.error(error && error.stack || error);
                process.exit(1);
              });
        "#,
        )
        .current_dir(&out_dir)
        .output()?;

    assert!(
        output.status.success(),
        "node failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout)?.trim(),
        r#"[["shared","feature","shared"],["feature","shared"]]"#
    );

    Ok(())
}

// Ported from webpack 5.108.1:
// test/cases/chunks/nested-blocks-with-available-parent-modules
// test/cases/chunks/nested-in-empty
#[tokio::test]
async fn nested_async_groups_terminate_and_collapse_available_back_edges()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    write(
        temp.path().join("src/index.js"),
        r#"globalThis.loadA = () => import("./a");"#,
    )?;
    write(
        temp.path().join("src/a.js"),
        r#"
            globalThis.aValue = "a";
            globalThis.loadB = () => import("./b");
        "#,
    )?;
    write(
        temp.path().join("src/b.js"),
        r#"
            export const value = "b";
            export const loadA = () => import("./a");
        "#,
    )?;

    let compilation = Compiler::new(CompilerOptions::new(
        temp.path(),
        vec![Entry::new("main", "./src/index")],
    ))
    .run()
    .await?;
    assert_eq!(
        compilation
            .assets()
            .iter()
            .filter(|asset| asset.filename.ends_with(".js"))
            .map(|asset| asset.filename.as_str())
            .collect::<Vec<_>>(),
        ["main.js", "src_a_js.js", "src_b_js.js"]
    );
    let main_asset = compilation
        .assets()
        .iter()
        .find(|asset| asset.filename == "main.js")
        .expect("Initial Asset must exist");
    assert!(main_asset.source.contains("__webpack_require__.d"));
    assert!(main_asset.source.contains("__webpack_require__.r"));

    let chunk_graph = compilation.chunk_graph();
    assert_eq!(chunk_graph.chunk_groups().len(), 3);
    let entry = chunk_graph.entrypoints()[0];
    let a_group = chunk_graph.chunk_groups()[entry.index()].children()[0];
    let b_group = chunk_graph.chunk_groups()[a_group.index()].children()[0];
    assert!(
        chunk_graph.chunk_groups()[b_group.index()]
            .children()
            .is_empty()
    );
    assert_eq!(
        chunk_graph.chunk_groups()[a_group.index()].parents(),
        [entry]
    );
    assert_eq!(
        chunk_graph.chunk_groups()[b_group.index()].parents(),
        [a_group]
    );

    let b_module = compilation
        .module_graph()
        .modules()
        .iter()
        .find(|module| module.identity().resource.ends_with("b.js"))
        .expect("fixture B Module must exist");
    assert_eq!(b_module.blocks().len(), 1);
    assert_eq!(
        chunk_graph.block_chunk_group(AsyncBlockOrigin {
            module: b_module.id(),
            block: AsyncDependenciesBlockId::new(0),
        }),
        None,
        "B-to-A edge must collapse because A is already available on the loading path"
    );

    Ok(())
}

#[tokio::test]
async fn shared_async_cross_imports_do_not_materialize_a_group_cycle()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    write(
        temp.path().join("src/entry-a.js"),
        r#"globalThis.loadDirectA = () => import("./a");"#,
    )?;
    write(
        temp.path().join("src/entry-b.js"),
        r#"globalThis.loadDirectB = () => import("./b");"#,
    )?;
    write(
        temp.path().join("src/a.js"),
        r#"
            globalThis.aValue = "a";
            globalThis.loadNestedB = () => import("./b");
        "#,
    )?;
    write(
        temp.path().join("src/b.js"),
        r#"
            export const value = "b";
            export const loadA = () => import("./a");
        "#,
    )?;

    let compilation = Compiler::new(CompilerOptions::new(
        temp.path(),
        vec![
            Entry::new("entry-a", "./src/entry-a"),
            Entry::new("entry-b", "./src/entry-b"),
        ],
    ))
    .run()
    .await?;
    let chunk_graph = compilation.chunk_graph();
    assert_eq!(chunk_graph.chunk_groups().len(), 4);
    let entry_a = chunk_graph.entrypoints()[0];
    let entry_b = chunk_graph.entrypoints()[1];
    let a_group = chunk_graph.chunk_groups()[entry_a.index()].children()[0];
    let b_group = chunk_graph.chunk_groups()[entry_b.index()].children()[0];
    let left_children = chunk_graph.chunk_groups()[a_group.index()].children();
    let right_children = chunk_graph.chunk_groups()[b_group.index()].children();
    assert_eq!(left_children.len() + right_children.len(), 1);
    assert!(
        !(left_children.contains(&b_group) && right_children.contains(&a_group)),
        "globally reused Async targets must not create reciprocal Chunk Group edges"
    );
    for entry_asset in ["entry-a.js", "entry-b.js"] {
        let source = &compilation
            .assets()
            .iter()
            .find(|asset| asset.filename == entry_asset)
            .expect("Entrypoint Asset must exist")
            .source;
        assert!(source.contains("__webpack_require__.d"));
        assert!(source.contains("__webpack_require__.r"));
    }

    let reversed = Compiler::new(CompilerOptions::new(
        temp.path(),
        vec![
            Entry::new("entry-b", "./src/entry-b"),
            Entry::new("entry-a", "./src/entry-a"),
        ],
    ))
    .run()
    .await?;
    assert_eq!(
        chunk_group_topology(&compilation),
        chunk_group_topology(&reversed),
        "Chunk Group origins and retained materialized edges must use stable identities"
    );

    Ok(())
}

#[tokio::test]
async fn nested_shared_parent_shrink_rescans_newly_required_modules()
-> Result<(), Box<dyn std::error::Error>> {
    let context = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../packages/unpack/test/e2e-cases/nested-shared-requeue")
        .canonicalize()?;
    let compilation = Compiler::new(CompilerOptions::new(
        &context,
        vec![Entry::new("main", "./src/index")],
    ))
    .run()
    .await?;
    let c_asset = compilation
        .assets()
        .iter()
        .find(|asset| asset.filename == "src_c_js.js")
        .expect("shared nested C Asset must exist");
    assert!(
        c_asset.source.contains(r#""./src/x.js": "#),
        "X must be added when C's second parent shrinks the available intersection"
    );

    let find_module = |filename: &str| {
        compilation
            .module_graph()
            .modules()
            .iter()
            .find(|module| module.identity().resource.ends_with(filename))
            .map(|module| module.id())
            .unwrap_or_else(|| panic!("fixture Module {filename} must exist"))
    };
    let p = find_module("p.js");
    let q = find_module("q.js");
    let x = find_module("x.js");
    let chunk_graph = compilation.chunk_graph();
    let c_from_p = chunk_graph
        .block_chunk_group(AsyncBlockOrigin {
            module: p,
            block: AsyncDependenciesBlockId::new(0),
        })
        .expect("P-to-C must map to an Async Chunk Group");
    let c_from_q = chunk_graph
        .block_chunk_group(AsyncBlockOrigin {
            module: q,
            block: AsyncDependenciesBlockId::new(0),
        })
        .expect("Q-to-C must reuse the Async Chunk Group");
    assert_eq!(c_from_p, c_from_q);
    assert_eq!(
        chunk_graph.chunk_groups()[c_from_p.index()].parents().len(),
        2
    );

    let y_group = chunk_graph
        .block_chunk_group(AsyncBlockOrigin {
            module: x,
            block: AsyncDependenciesBlockId::new(0),
        })
        .expect("rescanning X must retain its nested Y split point");
    assert!(
        chunk_graph.chunk_groups()[y_group.index()]
            .parents()
            .contains(&c_from_p),
        "requeued C must become a parent of X's nested Y group"
    );

    Ok(())
}

fn write(path: PathBuf, source: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, source)
}

fn chunk_group_topology(
    compilation: &unpack_core::Compilation,
) -> Vec<(String, Vec<String>, Vec<String>)> {
    let chunk_graph = compilation.chunk_graph();
    let label = |group: unpack_core::ChunkGroupId| {
        let group = &chunk_graph.chunk_groups()[group.index()];
        match group.kind() {
            ChunkGroupKind::Entrypoint { name } => format!("entry:{name}"),
            ChunkGroupKind::Async => {
                let origin = group
                    .origin()
                    .expect("Async Chunk Group must retain an origin");
                let module = compilation
                    .module_graph()
                    .module(origin.module)
                    .expect("Chunk Group origin Module must exist");
                format!(
                    "async:{}:{}",
                    module.identity().resource.display(),
                    origin.block.index()
                )
            }
        }
    };
    let mut topology = chunk_graph
        .chunk_groups()
        .iter()
        .map(|group| {
            let mut parents = group
                .parents()
                .iter()
                .copied()
                .map(&label)
                .collect::<Vec<_>>();
            parents.sort();
            let mut children = group
                .children()
                .iter()
                .copied()
                .map(&label)
                .collect::<Vec<_>>();
            children.sort();
            (label(group.id()), parents, children)
        })
        .collect::<Vec<_>>();
    topology.sort();
    topology
}

fn write_assets(out_dir: &Path, assets: &[unpack_core::Asset]) -> std::io::Result<()> {
    for asset in assets {
        write(out_dir.join(&asset.filename), &asset.source)?;
    }
    Ok(())
}

fn node_available() -> bool {
    Command::new("node")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    })
}
