# Remove persistent cache schema version guard

Unpack persistent cache manifests and cache pack DTOs will not carry a separate cache schema version. Persistent cache containers remain Unpack-private and are guarded by cache magic, pack magic, user cache version, build-dependency snapshots, and DTO deserialization failure; incompatible DTO changes should be handled through Serde compatibility, explicit user cache version changes, or decode rejection rather than a central schema-version constant. This supersedes the schema-version guard portion of ADR 0082 and ADR 0109.
