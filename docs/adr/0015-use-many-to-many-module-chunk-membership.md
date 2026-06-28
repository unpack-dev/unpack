# Use many-to-many module chunk membership

Unpack's chunk graph will represent module-to-chunk membership as a many-to-many relationship from the first implementation. Even when the initial code splitting rules usually place a module in one chunk, split-chunks support needs modules and chunks to be related independently so shared modules can move into or be associated with shared chunks without redefining the chunk graph model.
