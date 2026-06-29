# Fix undocumented webpack lifecycle differences

During JavaScript lifecycle alignment, Unpack will treat differences from webpack as alignment gaps to fix by default. A difference may remain only when it is a documented webpack deviation with a concrete project constraint, such as ESM-only package loading or Rust-native resource cleanup details; otherwise callback timing, callback `err` versus `Stats` behavior, stats availability, and run, watch, close, or watching conflict semantics should move toward webpack behavior where practical.
