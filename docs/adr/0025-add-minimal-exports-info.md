# Add minimal exports info

Unpack will introduce a minimal webpack-like `ExportsInfo` model before implementing tree shaking. The first version will record provided exports and treat every export as used, so harmony export dependency templates can ask for used names through a webpack-shaped interface without committing to full used-exports analysis or namespace re-export conflict handling yet.
