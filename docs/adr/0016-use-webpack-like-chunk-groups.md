# Use webpack-like chunk groups

Unpack will model chunk groups as webpack-like loading relationships rather than simple labels on chunks. Entrypoints are specialized initial chunk groups, async dependencies blocks map to async chunk groups through the chunk graph, chunk groups maintain ordered chunks plus parent and child group sets, and chunks may belong to multiple chunk groups. This shape is required for planned split-chunks support where a shared chunk can be inserted into multiple existing chunk groups.
