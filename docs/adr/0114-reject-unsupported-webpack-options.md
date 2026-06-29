# Reject unsupported webpack options

Unpack will reject JavaScript API options that webpack supports but Unpack has not implemented yet, and it will continue rejecting unknown options until a specific option surface deliberately chooses webpack's tolerance behavior. This prevents silent no-op configuration from masquerading as webpack API alignment: an exposed option should either match webpack's behavior where practical or fail with a clear validation error.
