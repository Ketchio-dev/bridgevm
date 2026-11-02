# Agent host work outside the platform lock (2026-11-02)

The vCPU0 automation guard covered the agent tick, including recursive host
share scans, reads/hashes, received-file writes/fsync/quarantine, deletion
and stdout flushing. These synchronous host operations could prevent the
independent HDA service tick from acquiring VirtPlatform while they ran.
This identifies a source lock scope, not a retained run's lock holder.

Automation now drains inbound console bytes under a short platform guard,
then advances the locally owned harness without platform or guest-memory
references. A synchronous send closure acquires the platform only for
console send and guest-memory/MSI work. Remaining device/checkpoint
operations reacquire their guard after host advancement.

Prelude/input processing, the serial-panic gate, boot triggers, agent
progression and remaining automation retain their order. Replies still
precede enqueue/pump. PUT chunks use the real send closure before advancing
their wire slot; there is no outbox, new worker or timer. Existing test-only
transport adapters preserve the older unit-test entry points.

A focused263-test run passes, including real advance-path multi-chunk PUT
bytes/completion and GETEND host write before queued PING transmission.
Production non-test clippy with warnings denied passes. Initial compilation
errors from extraction visibility/import/borrows were repaired and retained.

Input-file preparation and snapshot artifact output remain separate work;
this does not claim every automation operation is free of host I/O. No live
Windows run or audible improvement is proven. Criteria and state unchanged.
