mod build_cache;
mod cache_hash;
mod chunk_graph;
mod code_generation;
mod code_generation_record;
mod compilation;
mod compiler;
mod dependency;
mod error;
mod exports_info;
mod id_assignment;
mod loader;
mod logging;
mod make;
mod module;
mod module_graph;
mod normal_module_factory;
mod output_filename;
mod pack_file;
mod parser;
mod rendered_source;
mod resolver;
mod runtime;
mod snapshot;

pub use build_cache::{BuildDependency, CacheCompression, CacheKind, CacheOptions};
pub use chunk_graph::{
    AsyncBlockOrigin, Chunk, ChunkGraph, ChunkGroup, ChunkGroupId, ChunkGroupKind, ChunkId,
};
pub use code_generation::Asset;
pub use compilation::{Compilation, WatchDependencies};
pub use compiler::{
    CacheIdleReason, CacheLifecycleOutcome, Compiler, CompilerOptions, DEFAULT_EXTENSIONS, Entry,
    PendingCompilation,
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
pub use loader::{LoaderFuture, LoaderRequest, LoaderRunner, MatchedLoader, ModuleRule};
pub use logging::{InfrastructureLogEvent, InfrastructureLogLevel, InfrastructureLoggingOptions};
pub use module::{Module, ModuleId, ModuleIdentity, ModuleType};
pub use module_graph::{
    AsyncDependenciesBlockId, DependencyId, ModuleGraph, ModuleGraphConnection,
    ModuleGraphConnectionId,
};
pub use normal_module_factory::{FactorizedModule, NormalModuleFactory};
pub use resolver::{ResolveOptions, ResolvedResource, UnpackResolver};
pub use snapshot::{SnapshotOptions, SnapshotPathPattern, SnapshotStrategy};
