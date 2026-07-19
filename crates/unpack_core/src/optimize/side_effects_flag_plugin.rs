// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/optimize/SideEffectsFlagPlugin.js

use regex::Regex;
use serde_json::Value;

use crate::{
    Compilation, Dependency, ModuleGraph, ModuleHandle, compilation::CompilationHookSet,
    compiler::CompilerHookSet,
};

pub(crate) struct SideEffectsFlagPlugin {
    analyse_source: bool,
}

impl SideEffectsFlagPlugin {
    pub(crate) fn new(analyse_source: bool) -> Self {
        Self { analyse_source }
    }

    pub(crate) fn apply(&self, hooks: &mut CompilerHookSet) {
        let analyse_source = self.analyse_source;
        hooks.compilation.tap(
            "SideEffectsFlagPlugin",
            move |compilation_hooks: &mut CompilationHookSet| {
                if analyse_source {
                    compilation_hooks.javascript_parser.program.tap(
                        "SideEffectsFlagPlugin",
                        b"side-effects-program/1",
                        |_parser, _program, result| {
                            result.build_meta.side_effect_free = Some(true);
                        },
                    );
                    compilation_hooks.javascript_parser.statement.tap(
                        "SideEffectsFlagPlugin",
                        b"side-effects-statement/1",
                        |statement, result| {
                            if result.build_meta.side_effect_free == Some(true)
                                && !statement.is_pure()
                            {
                                result.build_meta.side_effect_free = Some(false);
                            }
                        },
                    );
                    compilation_hooks.javascript_parser.require_pure_analysis();
                }
                compilation_hooks
                    .optimize_dependencies
                    .tap("SideEffectsFlagPlugin", optimize_dependencies);
            },
        );
    }

    pub(crate) fn module_has_side_effects(module_name: &str, flag_value: &Value) -> bool {
        match flag_value {
            Value::Bool(value) => *value,
            Value::String(pattern) => glob_matches(module_name, pattern),
            Value::Array(patterns) => patterns
                .iter()
                .any(|pattern| Self::module_has_side_effects(module_name, pattern)),
            _ => true,
        }
    }
}

fn optimize_dependencies(compilation: &mut Compilation) {
    let rewrites = compilation
        .module_graph()
        .connections()
        .iter()
        .filter_map(|connection| connection_rewrite(compilation.module_graph(), connection))
        .collect::<Vec<_>>();
    for (handle, module, export_name) in rewrites {
        compilation
            .module_graph_mut()
            .update_connection_module(handle, module);
        let connection = compilation.module_graph_mut().connection_mut(handle);
        match &mut connection.dependency {
            Dependency::HarmonyImportSpecifier(dependency) => {
                if let Some(first) = dependency.ids.first_mut() {
                    *first = export_name;
                }
            }
            Dependency::HarmonyExportImportedSpecifier(dependency) => {
                if let Some(first) = dependency.ids.first_mut() {
                    *first = export_name;
                }
            }
            _ => {}
        }
    }

    let states = compilation
        .module_graph()
        .connections()
        .iter()
        .map(|connection| {
            if connection_is_active(compilation.module_graph(), connection) {
                crate::ModuleGraphConnectionState::Active
            } else {
                crate::ModuleGraphConnectionState::Inactive
            }
        })
        .collect::<Vec<_>>();
    for (connection, state) in compilation
        .module_graph_mut()
        .connections_mut()
        .iter_mut()
        .zip(states)
    {
        connection.set_state(state);
    }
}

fn connection_rewrite(
    module_graph: &ModuleGraph,
    connection: &crate::ModuleGraphConnection,
) -> Option<(crate::ModuleGraphConnectionHandle, ModuleHandle, String)> {
    let export_name = match &connection.dependency {
        Dependency::HarmonyImportSideEffect(_) => {
            let origin = connection.origin_module?;
            let imports = module_graph
                .outgoing_connections(origin)
                .filter(|candidate| candidate.module == connection.module)
                .filter(|candidate| {
                    matches!(candidate.dependency, Dependency::HarmonyImportSpecifier(_))
                })
                .collect::<Vec<_>>();
            let targets = imports
                .iter()
                .filter_map(|candidate| match &candidate.dependency {
                    Dependency::HarmonyImportSpecifier(dependency) => {
                        dependency.ids.first().and_then(|name| {
                            resolve_reexport_target(module_graph, candidate.module, name)
                        })
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            if targets.len() != imports.len() {
                return None;
            }
            let first = targets.first()?;
            if targets.iter().any(|target| target.0 != first.0) {
                return None;
            }
            return Some((connection.handle, first.0, first.1.clone()));
        }
        Dependency::HarmonyImportSpecifier(dependency) => {
            let origin = connection.origin_module?;
            let imported_names = module_graph
                .outgoing_connections(origin)
                .filter(|candidate| candidate.module == connection.module)
                .filter_map(|candidate| match &candidate.dependency {
                    Dependency::HarmonyImportSpecifier(dependency) => dependency.ids.first(),
                    _ => None,
                })
                .collect::<rustc_hash::FxHashSet<_>>();
            if imported_names.len() != 1 {
                return None;
            }
            dependency.ids.first()?
        }
        Dependency::HarmonyExportImportedSpecifier(dependency) if !dependency.is_star => {
            dependency.ids.first()?
        }
        _ => return None,
    };
    let (module, export_name) =
        resolve_reexport_target(module_graph, connection.module, export_name)?;
    Some((connection.handle, module, export_name))
}

fn resolve_reexport_target(
    module_graph: &ModuleGraph,
    mut module: ModuleHandle,
    export_name: &str,
) -> Option<(ModuleHandle, String)> {
    let mut export_name = export_name.to_string();
    let mut moved = false;
    let mut visited = rustc_hash::FxHashSet::default();
    while visited.insert(module) && module_is_side_effect_free(module_graph, module) {
        let outgoing = module_graph
            .outgoing_connections(module)
            .filter_map(|connection| match &connection.dependency {
                Dependency::HarmonyExportImportedSpecifier(dependency)
                    if dependency.name.as_deref() == Some(export_name.as_str()) =>
                {
                    Some((connection, dependency.ids.first().cloned()))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        let next = if outgoing.len() == 1 {
            outgoing.into_iter().next()
        } else if export_name != "default" {
            let stars = module_graph
                .outgoing_connections(module)
                .filter_map(|connection| match &connection.dependency {
                    Dependency::HarmonyExportImportedSpecifier(dependency)
                        if dependency.is_star =>
                    {
                        Some((connection, Some(export_name.clone())))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            (stars.len() == 1)
                .then(|| stars.into_iter().next())
                .flatten()
        } else {
            None
        };
        let Some((connection, next_export)) = next else {
            break;
        };
        if visited.contains(&connection.module) {
            return None;
        }
        module = connection.module;
        if let Some(next_export) = next_export {
            export_name = next_export;
        }
        moved = true;
    }
    moved.then_some((module, export_name))
}

fn connection_is_active(
    module_graph: &ModuleGraph,
    connection: &crate::ModuleGraphConnection,
) -> bool {
    match &connection.dependency {
        Dependency::HarmonyImportSideEffect(_) => {
            module_has_side_effects(module_graph, connection.module)
                || connection.origin_module.is_some_and(|origin| {
                    module_graph.outgoing_connections(origin).any(|candidate| {
                        candidate.module == connection.module
                            && matches!(candidate.dependency, Dependency::HarmonyImportSpecifier(_))
                    })
                })
        }
        Dependency::HarmonyExportImportedSpecifier(dependency) => {
            dependency.name.as_ref().is_some_and(|name| {
                connection.origin_module.is_some_and(|origin| {
                    module_graph
                        .exports_info(origin)
                        .get_used_name(name)
                        .is_some()
                })
            }) || (dependency.is_star && module_exports_are_used(module_graph, connection.module))
        }
        _ => true,
    }
}

fn module_exports_are_used(module_graph: &ModuleGraph, handle: ModuleHandle) -> bool {
    let exports_info = module_graph.exports_info(handle);
    exports_info.are_all_exports_used()
        || exports_info
            .used_exports()
            .is_some_and(|mut used| used.next().is_some())
}

fn module_has_side_effects(module_graph: &ModuleGraph, handle: ModuleHandle) -> bool {
    !module_is_side_effect_free(module_graph, handle)
}

fn module_is_side_effect_free(module_graph: &ModuleGraph, handle: ModuleHandle) -> bool {
    fn visit(
        module_graph: &ModuleGraph,
        handle: ModuleHandle,
        visiting: &mut rustc_hash::FxHashSet<ModuleHandle>,
    ) -> bool {
        let Some(module) = module_graph.module(handle) else {
            return false;
        };
        if !module.is_side_effect_free() {
            return false;
        }
        if !visiting.insert(handle) {
            return true;
        }
        let dependencies_are_free = module_graph.outgoing_connections(handle).all(|connection| {
            !matches!(
                connection.dependency,
                Dependency::HarmonyImportSideEffect(_)
            ) || visit(module_graph, connection.module, visiting)
        });
        visiting.remove(&handle);
        dependencies_are_free
    }

    visit(module_graph, handle, &mut rustc_hash::FxHashSet::default())
}

fn glob_matches(module_name: &str, pattern: &str) -> bool {
    let pattern = if pattern.contains('/') {
        pattern.to_string()
    } else {
        format!("**/{pattern}")
    };
    let mut source = String::from("^(?:\\./)?");
    let chars = pattern.chars().collect::<Vec<_>>();
    let mut index = 0;
    while index < chars.len() {
        match chars[index] {
            '*' if chars.get(index + 1) == Some(&'*') => {
                if chars.get(index + 2) == Some(&'/') {
                    source.push_str("(?:.*/)?");
                    index += 2;
                } else {
                    source.push_str(".*");
                    index += 1;
                }
            }
            '*' => source.push_str("[^/]*"),
            '?' => source.push_str("[^/]"),
            character => source.push_str(&regex::escape(&character.to_string())),
        }
        index += 1;
    }
    source.push('$');
    Regex::new(&source)
        .expect("generated sideEffects glob must be a valid regular expression")
        .is_match(module_name)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::SideEffectsFlagPlugin;

    #[test]
    fn module_has_side_effects_matches_webpack_flag_shapes() {
        assert!(!SideEffectsFlagPlugin::module_has_side_effects(
            "./src/a.js",
            &json!(false)
        ));
        assert!(SideEffectsFlagPlugin::module_has_side_effects(
            "./src/a.js",
            &json!(true)
        ));
        assert!(SideEffectsFlagPlugin::module_has_side_effects(
            "./src/a.js",
            &json!("*.js")
        ));
        assert!(SideEffectsFlagPlugin::module_has_side_effects(
            "./src/a.js",
            &json!(["./styles/*.css", "./src/*.js"]),
        ));
        assert!(!SideEffectsFlagPlugin::module_has_side_effects(
            "./src/a.js",
            &json!(["./styles/*.css"]),
        ));
    }
}
