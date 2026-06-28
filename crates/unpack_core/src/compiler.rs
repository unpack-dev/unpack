use std::path::PathBuf;

use crate::{Compilation, ResolveOptions, Result, UnpackResolver, build_cache::BuildCache};

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
    pub resolve: ResolveOptions,
    pub parallelism: usize,
}

impl CompilerOptions {
    pub fn new(context: impl Into<PathBuf>, entries: Vec<Entry>) -> Self {
        Self {
            context: normalize_context(context.into()),
            entries,
            resolve: default_resolve_options(),
            parallelism: 100,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Compiler {
    options: CompilerOptions,
    resolver: UnpackResolver,
    build_cache: BuildCache,
}

impl Compiler {
    pub fn new(options: CompilerOptions) -> Self {
        let resolver = UnpackResolver::new(options.resolve.clone());
        Self {
            options,
            resolver,
            build_cache: BuildCache::default(),
        }
    }

    pub fn options(&self) -> &CompilerOptions {
        &self.options
    }

    pub fn create_compilation(&self) -> Compilation {
        Compilation::new(
            self.options.clone(),
            self.resolver.clone(),
            self.build_cache.clone(),
        )
    }

    pub async fn run(&self) -> Result<Compilation> {
        let mut compilation = self.create_compilation();
        compilation.make().await?;
        compilation.build_chunk_graph();
        compilation.create_assets();
        Ok(compilation)
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
    use std::{collections::BTreeMap, fs, path::PathBuf};

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

        let second = compiler.run().await?;
        let second_cache = compiler.build_cache.stats();
        assert_eq!(second_cache.module_entries, 2);
        assert_eq!(second_cache.module_hits, 2);
        assert_eq!(second_cache.module_misses, 2);

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

    fn write(path: PathBuf, source: &str) -> std::io::Result<()> {
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
