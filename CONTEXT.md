# Unpack

Unpack is a JavaScript bundling project that aims for webpack-like outcomes without treating webpack compatibility as part of its product contract.

## Language

**Webpack-like**:
Similar in purpose and workflow to webpack, but free to use its own public API, configuration model, loader model, and plugin model.
_Avoid_: Webpack-compatible

**Bundle**:
The emitted JavaScript and related assets produced from an application's dependency graph.
_Avoid_: Pack, build output

**Module Graph**:
The connected set of modules reachable from one or more entry points.
_Avoid_: Dependency tree
