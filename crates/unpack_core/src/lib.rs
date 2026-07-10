mod build_cache;
mod cache_hash;
mod chunk_graph;
mod code_generation;
mod compilation;
mod compiler;
mod dependency;
mod error;
mod exports_info;
mod logging;
mod make;
mod module;
mod module_graph;
mod normal_module_factory;
mod pack_file;
mod parser;
mod rendered_source;
mod resolver;
mod snapshot;

pub use build_cache::{BuildDependency, CacheKind, CacheOptions};
pub use chunk_graph::{
    AsyncBlockOrigin, Chunk, ChunkGraph, ChunkGroup, ChunkGroupId, ChunkGroupKind, ChunkId,
};
pub use code_generation::{Asset, RuntimeRequirement, RuntimeRequirements};
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
pub use logging::{InfrastructureLogEvent, InfrastructureLogLevel, InfrastructureLoggingOptions};
pub use module::{Module, ModuleId, ModuleIdentity, ModuleType};
pub use module_graph::{ModuleGraph, ModuleGraphConnection};
pub use normal_module_factory::{FactorizedModule, NormalModuleFactory};
pub use resolver::{ResolveOptions, ResolvedResource, UnpackResolver};
pub use snapshot::{SnapshotOptions, SnapshotPathPattern, SnapshotStrategy};
