# Apply snapshot path classification across categories

Unpack will apply managed, immutable, and unmanaged path classification inside the shared snapshot infrastructure for every effective snapshot category, including module resources, resolution inputs, build dependencies, and resolve-build-dependency inputs. The classification is not owned by individual cache items or facades; this keeps a filesystem input's invalidation semantics consistent regardless of which build-cache record references it.
