# Use a webpack-like compilation pipeline

Unpack will structure compilation around make, build chunk graph, code generation, and asset creation phases. The make phase builds modules and dependency blocks, build chunk graph derives chunks and chunk groups, code generation applies dependency templates with `rspack_sources`, and asset creation assembles webpack-shaped runtime output. These explicit phase boundaries align the implementation with webpack and Rspack concepts and provide natural future persistent-cache boundaries.
