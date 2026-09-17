# Installed-disk import tier checkpoint — 2026-09-16

The original T19 checkpoint added a strict 3D-off packaged-app journey,
immutable per-lane disk, vars and vTPM clones, authenticated receipts, and
fail-closed synthetic normal and tamper cases.

Review before the first live run found that copying encrypted swtpm state was
insufficient: its device-local key remained bound to the source VM's Keychain
identity. Such an import would register successfully and then fail closed at
boot because a new VM ID could not open the copied state.

The corrected flow requires the source vTPM state, BridgeVM recovery package
and private recovery-code file together. It authenticates the package against
the copied state, refuses to overwrite an existing destination identity, and
installs the recovered key under the imported VM ID. Failed publication rolls
the new key back. T19 also proves that its temporary key matches the same
package and state before removing it during cleanup.

This remains deterministic integration evidence. A9 stays OPEN until exact
hosted checks and the required physical-Mac T17 and T19 campaigns pass.
