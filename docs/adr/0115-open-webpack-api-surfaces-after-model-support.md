# Open webpack API surfaces after model support

Unpack will stage webpack API alignment by building the internal compilation model before exposing the corresponding public API surface. Options, hooks, loaders, and plugin entrypoints should only be accepted when the internal model can support their observable webpack behavior; until then they should be rejected as unsupported webpack options or documented as alignment gaps. This keeps API growth tied to real bundler capability instead of accumulating no-op compatibility placeholders.
