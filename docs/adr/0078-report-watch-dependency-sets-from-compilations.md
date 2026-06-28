# Report watch dependency sets from compilations

Each compilation will report watch dependency sets for files, directories, and missing paths, and a watch session will replace its subscriptions from the latest completed compilation. The first implementation primarily fills file dependencies from resolved module resources and may use missing dependencies for unresolved requests, while context dependencies remain available for future context-module support.
