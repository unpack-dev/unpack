# Use the SWC experimental parser

Unpack will parse JavaScript and TypeScript modules with `swc_experimental_ecma_parser` and its matching experimental AST and allocator crates. This follows the project's parser direction while keeping parser-owned lifetimes inside the parser adapter, so the make phase stores owned dependency data rather than leaking SWC AST lifetimes into the module graph.
