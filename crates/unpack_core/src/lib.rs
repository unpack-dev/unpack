// Webpack source: https://github.com/webpack/webpack/tree/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib

mod async_dependencies_block;
mod build_chunk_graph;
mod cache;
mod cache_facade;
mod cache_hash;
mod chunk;
mod chunk_graph;
mod chunk_group;
mod code_generation;
mod code_generation_record;
mod compilation;
mod compiler;
mod dependencies;
mod dependencies_block;
mod dependency;
mod error;
mod exports_info;
mod flag_dependency_exports_plugin;
mod flag_dependency_usage_plugin;
mod hooks;
mod id_assignment;
mod loader;
mod logging;
mod make;
mod module;
mod module_graph;
mod module_graph_connection;
mod normal_module_factory;
mod optimize;
mod output_filename;
mod parser;
mod rendered_source;
mod resolver;
mod runtime;
mod snapshot;

pub use async_dependencies_block::AsyncDependenciesBlock;
pub use cache::{BuildDependency, CacheCompression, CacheKind, CacheOptions};
pub use chunk::{Chunk, ChunkHandle};
pub use chunk_graph::ChunkGraph;
pub use chunk_group::{AsyncBlockOrigin, ChunkGroup, ChunkGroupHandle, ChunkGroupKind};
pub use code_generation::Asset;
pub use compilation::{Compilation, WatchDependencies};
pub use compiler::{
    CacheIdleReason, CacheLifecycleOutcome, Compiler, CompilerOptions, DEFAULT_EXTENSIONS, Entry,
    PendingCompilation, SideEffectsOption,
};
pub use dependencies::{
    ConstDependency, EntryDependency, HarmonyExportExpressionDependency,
    HarmonyExportHeaderDependency, HarmonyExportImportedSpecifierDependency,
    HarmonyExportSpecifierDependency, HarmonyImportSideEffectDependency,
    HarmonyImportSpecifierDependency, ImportDependency, ModuleDependency, NullDependency,
};
pub use dependencies_block::DependenciesBlock;
pub use dependency::{Dependency, DependencyKind, SourceRange};
pub use error::{Error, Result};
pub use exports_info::ExportsInfo;
pub use hooks::{CompilationHooks, HookFuture};
pub use loader::{LoaderFuture, LoaderRequest, LoaderRunner, MatchedLoader, ModuleRule};
pub use logging::{InfrastructureLogEvent, InfrastructureLogLevel, InfrastructureLoggingOptions};
pub use module::{Module, ModuleHandle, ModuleIdentity, ModuleType};
pub use module_graph::{AsyncDependenciesBlockIndex, DependencyIndex, ModuleGraph};
pub use module_graph_connection::{
    ModuleGraphConnection, ModuleGraphConnectionHandle, ModuleGraphConnectionState,
};
pub use normal_module_factory::{FactorizedModule, NormalModuleFactory};
pub use resolver::{ResolveOptions, ResolvedResource, UnpackResolver};
pub use snapshot::{SnapshotOptions, SnapshotPathPattern, SnapshotStrategy};
