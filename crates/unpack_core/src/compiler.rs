use std::path::PathBuf;

use crate::{
    CacheOptions, Compilation, InfrastructureLoggingOptions, ResolveOptions, Result,
    SnapshotOptions, UnpackResolver, build_cache::BuildCache,
};
use tracing::Instrument;

pub const DEFAULT_EXTENSIONS: &[&str] = &[".ts", ".tsx", ".js", ".jsx"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub request: String,
}

impl Entry {
    pub fn new(name: impl Into<String>, request: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            request: request.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CompilerOptions {
    pub context: PathBuf,
    pub entries: Vec<Entry>,
    pub cache: CacheOptions,
    pub resolve: ResolveOptions,
    pub snapshot: SnapshotOptions,
    pub infrastructure_logging: InfrastructureLoggingOptions,
    pub parallelism: usize,
}

impl CompilerOptions {
    pub fn new(context: impl Into<PathBuf>, entries: Vec<Entry>) -> Self {
        Self {
            context: normalize_context(context.into()),
            entries,
            cache: CacheOptions::default(),
            resolve: default_resolve_options(),
            snapshot: SnapshotOptions::default(),
            infrastructure_logging: InfrastructureLoggingOptions::disabled(),
            parallelism: 100,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Compiler {
    options: CompilerOptions,
    build_cache: BuildCache,
}

impl Compiler {
    pub fn new(options: CompilerOptions) -> Self {
        let build_cache = BuildCache::new(options.cache.clone(), options.snapshot.clone());
        Self {
            options,
            build_cache,
        }
    }

    pub fn options(&self) -> &CompilerOptions {
        &self.options
    }

    pub fn create_compilation(&self) -> Compilation {
        Compilation::new(
            self.options.clone(),
            UnpackResolver::new(self.options.resolve.clone()),
            self.build_cache.clone(),
        )
    }

    pub async fn run(&self) -> Result<Compilation> {
        async {
            let mut compilation = self.create_compilation();
            compilation.make().await?;
            compilation.build_chunk_graph();
            compilation.create_assets();
            Ok(compilation)
        }
        .instrument(tracing::trace_span!("Compiler::run"))
        .await
    }

    pub fn flush_cache(&self) -> std::result::Result<(), String> {
        let span = tracing::trace_span!("Compiler::flush_cache");
        let _enter = span.enter();
        self.build_cache
            .flush_to_filesystem()
            .map_err(|error| error.to_string())
    }
}

fn default_resolve_options() -> ResolveOptions {
    let mut options = ResolveOptions::default();
    options.extensions = DEFAULT_EXTENSIONS
        .iter()
        .map(|extension| (*extension).to_string())
        .collect();
    options
}

fn normalize_context(context: PathBuf) -> PathBuf {
    if context.is_absolute() {
        context
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(&context))
            .unwrap_or(context)
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, fs, path::Path};

    use super::*;

    #[tokio::test]
    async fn repeated_runs_reuse_memory_module_build_records_without_sharing_compilations()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        write(
            temp.path().join("index.js"),
            r#"
                import "./dep";
                export const result = "ok";
            "#,
        )?;
        write(temp.path().join("dep.js"), "export const value = 1;")?;

        let compiler = Compiler::new(CompilerOptions::new(
            temp.path(),
            vec![Entry::new("main", "./index")],
        ));

        let first = compiler.run().await?;
        let first_cache = compiler.build_cache.stats();
        assert_eq!(first_cache.module_entries, 2);
        assert_eq!(first_cache.module_hits, 0);
        assert_eq!(first_cache.module_misses, 2);
        assert_eq!(first_cache.resolve_entries, 2);
        assert_eq!(first_cache.resolve_hits, 0);
        assert_eq!(first_cache.resolve_misses, 2);

        let second = compiler.run().await?;
        let second_cache = compiler.build_cache.stats();
        assert_eq!(second_cache.module_entries, 2);
        assert_eq!(second_cache.module_hits, 2);
        assert_eq!(second_cache.module_misses, 2);
        assert_eq!(second_cache.resolve_entries, 2);
        assert_eq!(second_cache.resolve_hits, 2);
        assert_eq!(second_cache.resolve_misses, 2);

        assert_eq!(first.errors(), []);
        assert_eq!(second.errors(), []);
        assert_eq!(asset_sources(&first), asset_sources(&second));
        assert_eq!(first.module_graph(), second.module_graph());
        assert_ne!(
            first.module_graph().modules().as_ptr(),
            second.module_graph().modules().as_ptr()
        );

        Ok(())
    }

    #[tokio::test]
    async fn hash_module_snapshot_strategy_invalidates_changed_source()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        let entry = temp.path().join("index.js");
        write(&entry, "export const value = 'before';")?;

        let mut options = CompilerOptions::new(temp.path(), vec![Entry::new("main", "./index")]);
        options.snapshot.module = crate::SnapshotStrategy::hash();
        let compiler = Compiler::new(options);

        let first = compiler.run().await?;
        assert!(
            asset_sources(&first)
                .get("main.js")
                .expect("main asset should exist")
                .contains("before")
        );

        write(&entry, "export const value = 'after';")?;

        let second = compiler.run().await?;
        assert!(
            asset_sources(&second)
                .get("main.js")
                .expect("main asset should exist")
                .contains("after")
        );

        Ok(())
    }

    #[tokio::test]
    async fn filesystem_cache_restores_module_build_records_for_later_compiler_instances()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        write(
            temp.path().join("index.js"),
            r#"
                import "./dep";
                export const result = "ok";
            "#,
        )?;
        write(temp.path().join("dep.js"), "export const value = 1;")?;
        let cache_location = temp.path().join(".cache/unpack/default");

        let mut options = CompilerOptions::new(temp.path(), vec![Entry::new("main", "./index")]);
        options.cache = CacheOptions::filesystem();
        options.cache.cache_location = Some(cache_location.clone());
        options.cache.version = Some("test-version".to_string());

        let first_compiler = Compiler::new(options.clone());
        let first = first_compiler.run().await?;
        first_compiler.flush_cache()?;
        assert_eq!(first.errors(), []);
        assert!(cache_location.join("container.json").exists());
        assert!(cache_location.join("packs/modules.cbor").exists());
        let manifest = fs::read_to_string(cache_location.join("container.json"))?;
        assert!(manifest.contains("UNPACK_PERSISTENT_CACHE"));
        assert!(manifest.contains("test-version"));
        let manifest_json: serde_json::Value = serde_json::from_str(&manifest)?;
        assert!(manifest_json.get("schema_version").is_none());

        let second_compiler = Compiler::new(options);
        assert_eq!(second_compiler.build_cache.stats().resolve_entries, 2);
        assert_eq!(second_compiler.build_cache.stats().module_entries, 2);

        let second = second_compiler.run().await?;
        let second_cache = second_compiler.build_cache.stats();
        assert_eq!(second_cache.resolve_hits, 2);
        assert_eq!(second_cache.module_hits, 2);
        assert_eq!(asset_sources(&first), asset_sources(&second));

        Ok(())
    }

    #[tokio::test]
    async fn filesystem_cache_rejects_invalid_build_dependency_snapshots()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        write(temp.path().join("index.js"), "export const value = 1;")?;
        let config = temp.path().join("config.js");
        write(&config, "export default 'before';")?;
        let cache_location = temp.path().join(".cache/unpack/default");

        let mut options = CompilerOptions::new(temp.path(), vec![Entry::new("main", "./index")]);
        options.cache = CacheOptions::filesystem();
        options.cache.cache_location = Some(cache_location);
        options.cache.build_dependencies = vec![crate::BuildDependency {
            name: "config".to_string(),
            files: vec![config.clone()],
        }];

        let first_compiler = Compiler::new(options.clone());
        first_compiler.run().await?;
        first_compiler.flush_cache()?;
        assert_eq!(
            Compiler::new(options.clone())
                .build_cache
                .stats()
                .module_entries,
            1
        );

        write(&config, "export default 'after';")?;
        assert_eq!(Compiler::new(options).build_cache.stats().module_entries, 0);

        Ok(())
    }

    #[tokio::test]
    async fn filesystem_cache_rechecks_missing_resolve_candidates()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        write(
            temp.path().join("index.js"),
            r#"
                import { value } from "./dep";
                export const result = value;
            "#,
        )?;
        write(temp.path().join("dep.js"), "export const value = 'js';")?;
        let cache_location = temp.path().join(".cache/unpack/default");

        let mut options = CompilerOptions::new(temp.path(), vec![Entry::new("main", "./index")]);
        options.cache = CacheOptions::filesystem();
        options.cache.cache_location = Some(cache_location);

        let first_compiler = Compiler::new(options.clone());
        let first = first_compiler.run().await?;
        first_compiler.flush_cache()?;
        assert!(
            asset_sources(&first)
                .get("main.js")
                .expect("main asset should exist")
                .contains("'js'")
        );

        write(temp.path().join("dep.ts"), "export const value = 'ts';")?;

        let second_compiler = Compiler::new(options);
        let second = second_compiler.run().await?;
        assert!(
            asset_sources(&second)
                .get("main.js")
                .expect("main asset should exist")
                .contains("'ts'")
        );

        Ok(())
    }

    fn write(path: impl AsRef<Path>, source: &str) -> std::io::Result<()> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, source)
    }

    fn asset_sources(compilation: &Compilation) -> BTreeMap<String, String> {
        compilation
            .assets()
            .iter()
            .map(|asset| (asset.filename.clone(), asset.source.clone()))
            .collect()
    }
}
