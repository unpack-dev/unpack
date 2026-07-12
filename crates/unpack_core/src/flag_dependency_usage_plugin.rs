use std::collections::{BTreeSet, HashMap};

use crate::{
    Compilation, Dependency, ModuleHandle, compilation::CompilationHooks, compiler::CompilerHooks,
};

pub(crate) struct FlagDependencyUsagePlugin;

impl FlagDependencyUsagePlugin {
    pub fn apply(&self, hooks: &mut CompilerHooks) {
        hooks.compilation.tap(
            "FlagDependencyUsagePlugin",
            |compilation_hooks: &mut CompilationHooks| {
                compilation_hooks
                    .optimize_dependencies
                    .tap("FlagDependencyUsagePlugin", flag_dependency_usage);
            },
        );
    }
}

fn flag_dependency_usage(compilation: &mut Compilation) {
    let mut used: HashMap<ModuleHandle, (bool, BTreeSet<String>)> = compilation
        .module_graph()
        .modules()
        .iter()
        .map(|module| (module.handle(), (false, BTreeSet::new())))
        .collect();
    for entry in compilation.entries() {
        let provided = compilation
            .module_graph()
            .module(*entry)
            .and_then(|module| module.exports_info().provided_exports())
            .map(|exports| exports.map(str::to_string).collect::<Vec<_>>());
        let entry_usage = used.entry(*entry).or_default();
        if let Some(provided) = provided {
            entry_usage.1.extend(provided);
        } else {
            entry_usage.0 = true;
        }
    }

    loop {
        let mut changed = false;
        for connection in compilation.module_graph().connections() {
            if matches!(connection.dependency, Dependency::Import(_)) {
                let target = used.entry(connection.module).or_default();
                changed |= !target.0;
                target.0 = true;
                continue;
            }
            let requested = match &connection.dependency {
                Dependency::HarmonyImportSpecifier(dep) => dep.ids.first(),
                Dependency::HarmonyExportImportedSpecifier(dep) => {
                    let origin_uses_export = dep.name.as_ref().is_some_and(|name| {
                        connection.origin_module.is_some_and(|origin| {
                            used.get(&origin)
                                .is_some_and(|(all, names)| *all || names.contains(name))
                        })
                    });
                    origin_uses_export.then(|| dep.ids.first()).flatten()
                }
                _ => None,
            };
            if let Some(name) = requested {
                changed |= used
                    .entry(connection.module)
                    .or_default()
                    .1
                    .insert(name.clone());
            } else if let Dependency::HarmonyExportImportedSpecifier(dep) = &connection.dependency
                && dep.is_star
            {
                let origin_usage = connection
                    .origin_module
                    .and_then(|origin| used.get(&origin))
                    .cloned()
                    .unwrap_or_default();
                let target = used.entry(connection.module).or_default();
                if origin_usage.0 {
                    changed |= !target.0;
                    target.0 = true;
                } else {
                    let previous_len = target.1.len();
                    target
                        .1
                        .extend(origin_usage.1.into_iter().filter(|name| name != "default"));
                    changed |= target.1.len() != previous_len;
                }
            }
        }
        if !changed {
            break;
        }
    }

    for (handle, (all, names)) in used {
        if let Some(module) = compilation.module_graph_mut().module_mut(handle) {
            if all {
                module.exports_info_mut().set_all_exports_used();
            } else {
                module.exports_info_mut().set_used_exports(Some(names));
            }
        }
    }
}
