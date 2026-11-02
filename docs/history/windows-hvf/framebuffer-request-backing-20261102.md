# Framebuffer request backing identity (2026-11-02)

A display request used a writable descriptor checked against the current
pathname, without comparing it to the framebuffer already mapped by the
view. A regular-file replacement between mapping validation and request
posting could receive the8byte request word at offset32. Reusing a requester
with another mapping could also keep writing its previous descriptor.

The request descriptor is now a lease bound to its mapping. On opening,
both descriptors must identify the same regular backing inode/device.
A different mapping closes the old lease before opening another. A request
never writes a different regular replacement merely because it has the
right pathname. Existing no-follow/path checks and reset behavior remain.

The mapping opener was extracted under existing structural ceilings. It
retains the descriptor/mmap ownership and failure cleanup. The request mark,
clock, header offset and old-writer no-write behavior are unchanged.

Two focused temporary-file regressions fail on the previous implementation
with three assertions: the replacement's bytes change, and requester reuse
updates the wrong file. Both tests pass after the repair. Tests use owned
synthetic frame files; no operator data is used.

Final exact local/hosted/PR gates remain required. This is host-file data
safety evidence, not a live guest, screen-presentation or performance claim.
No criterion statement, threshold, product state or guest contract changes.
