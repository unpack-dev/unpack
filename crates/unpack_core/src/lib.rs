mod chunk_graph;
mod code_generation;
mod compilation;
mod compiler;
mod dependency;
mod error;
mod exports_info;
mod make;
mod module;
mod module_graph;
mod normal_module_factory;
mod parser;
mod resolver;

pub use chunk_graph::{
    AsyncBlockOrigin, Chunk, ChunkGraph, ChunkGroup, ChunkGroupId, ChunkGroupKind, ChunkId,
};
pub use code_generation::{Asset, RuntimeRequirement, RuntimeRequirements};
pub use compilation::Compilation;
pub use compiler::{Compiler, CompilerOptions, DEFAULT_EXTENSIONS, Entry};
pub use dependency::{
    AsyncDependenciesBlock, ConstDependency, Dependency, DependencyKind, EntryDependency,
    HarmonyExportExpressionDependency, HarmonyExportHeaderDependency,
    HarmonyExportImportedSpecifierDependency, HarmonyExportSpecifierDependency,
    HarmonyImportSideEffectDependency, HarmonyImportSpecifierDependency, ImportDependency,
    ModuleDependency, NullDependency, SourceRange,
};
pub use error::{Error, Result};
pub use exports_info::ExportsInfo;
pub use module::{Module, ModuleId, ModuleIdentity, ModuleType};
pub use module_graph::{ModuleGraph, ModuleGraphConnection};
pub use normal_module_factory::{FactorizedModule, NormalModuleFactory};
pub use resolver::{ResolveOptions, ResolvedResource, UnpackResolver};
