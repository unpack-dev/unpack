# Use structural module identity during make

Unpack will use a structural module identity for make-phase deduplication instead of treating the resolved file path as the module's identity. The first implementation may only populate the resource path and default module type, but the model reserves room for module type, layer, query, fragment, and loader pipeline so future features do not have to redefine what makes a module unique.
