# Require at least one snapshot validation method

Unpack will reject snapshot strategy objects where both `timestamp` and `hash` are false. Although webpack's schema permits this shape, Unpack will require every effective snapshot category to use at least one validation method so cache items cannot become silently permanent. Users who want to avoid reuse should disable the build cache with `cache: false` rather than configuring an unvalidated snapshot strategy.
