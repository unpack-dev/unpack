# Accept a minimal webpack-like entry option

The JavaScript API will accept `entry` as either a string request, which becomes the `main` entry, or an object mapping entry names to requests. This gives JavaScript users a familiar webpack-like entry shape while keeping the first public configuration contract narrow and avoiding array entries, descriptor objects, function configs, and webpack config-file loading.
