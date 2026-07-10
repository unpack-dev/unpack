use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use unpack_core::{ChunkGroupKind, Compiler, CompilerOptions, Entry};

#[tokio::test]
async fn separates_module_code_generation_from_asset_rendering()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    write(
        temp.path().join("src/index.js"),
        r#"
            export const value = 42;
        "#,
    )?;

    let compiler = Compiler::new(CompilerOptions::new(
        temp.path(),
        vec![Entry::new("main", "./src/index")],
    ));
    let mut compilation = compiler.create_compilation();

    compilation.make().await?;
    compilation.build_chunk_graph();
    compilation.code_generation();

    assert_eq!(compilation.assets(), []);

    compilation.create_assets();

    assert_eq!(compilation.errors(), []);
    assert_eq!(
        compilation
            .assets()
            .iter()
            .map(|asset| asset.filename.as_str())
            .collect::<Vec<_>>(),
        ["main.js", "main.js.map"]
    );

    Ok(())
}

#[tokio::test]
async fn builds_a_render_manifest_before_creating_asset_bookkeeping()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    write(
        temp.path().join("src/index.js"),
        r#"
            export async function load() {
                return import("./feature");
            }
        "#,
    )?;
    write(
        temp.path().join("src/feature.js"),
        "export const value = 42;",
    )?;

    let compiler = Compiler::new(CompilerOptions::new(
        temp.path(),
        vec![Entry::new("main", "./src/index")],
    ));
    let mut compilation = compiler.create_compilation();

    compilation.make().await?;
    compilation.build_chunk_graph();
    compilation.code_generation();
    compilation.create_asset_render_manifest();

    assert_eq!(compilation.assets(), []);

    compilation.render_assets();

    assert_eq!(compilation.errors(), []);
    assert_eq!(
        compilation
            .assets()
            .iter()
            .map(|asset| asset.filename.as_str())
            .collect::<Vec<_>>(),
        [
            "main.js",
            "main.js.map",
            "src_feature_js.js",
            "src_feature_js.js.map"
        ]
    );

    Ok(())
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
            ("main.js", 3166, 13406043872577102803),
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
    assert!(main.source.contains("__webpack_require__.d"));
    assert!(main.source.contains("__webpack_require__.r"));
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
        "#,
    )?;
    write(
        temp.path().join("src/b.js"),
        r#"
            export async function load() {
                const mod = await import("./feature");
                return [mod.feature, mod.shared];
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

    let compiler = Compiler::new(CompilerOptions::new(
        temp.path(),
        vec![Entry::new("a", "./src/a"), Entry::new("b", "./src/b")],
    ));
    let compilation = compiler.run().await?;
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

fn write(path: PathBuf, source: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, source)
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
