mod build_cache;
mod build_chunk_graph;
mod cache_hash;
mod chunk;
mod chunk_graph;
mod chunk_group;
mod code_generation;
mod code_generation_record;
mod compilation;
mod compiler;
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
mod normal_module_factory;
mod optimize;
mod output_filename;
mod pack_file;
mod parser;
mod rendered_source;
mod resolver;
mod runtime;
mod snapshot;

pub use build_cache::{BuildDependency, CacheCompression, CacheKind, CacheOptions};
pub use chunk::{Chunk, ChunkHandle};
pub use chunk_graph::ChunkGraph;
pub use chunk_group::{AsyncBlockOrigin, ChunkGroup, ChunkGroupHandle, ChunkGroupKind};
pub use code_generation::Asset;
pub use compilation::{Compilation, WatchDependencies};
pub use compiler::{
    CacheIdleReason, CacheLifecycleOutcome, Compiler, CompilerOptions, DEFAULT_EXTENSIONS, Entry,
    PendingCompilation, SideEffectsOption,
};
pub use dependency::{
    AsyncDependenciesBlock, ConstDependency, Dependency, DependencyKind, EntryDependency,
    HarmonyExportExpressionDependency, HarmonyExportHeaderDependency,
    HarmonyExportImportedSpecifierDependency, HarmonyExportSpecifierDependency,
    HarmonyImportSideEffectDependency, HarmonyImportSpecifierDependency, ImportDependency,
    ModuleDependency, NullDependency, SourceRange,
};
pub use error::{Error, Result};
pub use exports_info::ExportsInfo;
pub use hooks::{CompilationHooks, HookFuture};
pub use loader::{LoaderFuture, LoaderRequest, LoaderRunner, MatchedLoader, ModuleRule};
pub use logging::{InfrastructureLogEvent, InfrastructureLogLevel, InfrastructureLoggingOptions};
pub use module::{Module, ModuleHandle, ModuleIdentity, ModuleType};
pub use module_graph::{
    AsyncDependenciesBlockIndex, DependencyIndex, ModuleGraph, ModuleGraphConnection,
    ModuleGraphConnectionHandle, ModuleGraphConnectionState,
};
pub use normal_module_factory::{FactorizedModule, NormalModuleFactory};
pub use resolver::{ResolveOptions, ResolvedResource, UnpackResolver};
pub use snapshot::{SnapshotOptions, SnapshotPathPattern, SnapshotStrategy};
