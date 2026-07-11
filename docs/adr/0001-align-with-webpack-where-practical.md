---
status: superseded by ADR-0137
---

# Align with webpack where practical

Unpack will treat webpack's bundling behavior, public JavaScript API, configuration concepts, loader and plugin model, internal naming, and compilation flow as the reference design to follow where practical. Deviations from webpack should be deliberate, documented, and justified by current project constraints rather than treated as a blanket freedom to design an Unpack-specific API.
