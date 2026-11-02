# Archive the vTPM key when a managed HVF VM is deleted (2026-11-02)

Static reading found that a Library deletion of a managed HVF Windows VM removed
`<library>/<slug>/` but left the vTPM state key, which lives in the Keychain under
the VM slug. A later VM with the same slug (the default name "Windows 11" gives
`windows-11`) would adopt that key, and a first-run vTPM recovery import into the
slug was refused because a key already existed. That refusal matches what T19 r1
and r2 hit on 2026-10-22; that cause comes from reading the code.

After a deletion that actually removes the managed bundle, the Library now copies
the active key to `<slug>.deleted.<epoch>-<nonce8>` (the suffix format reset
uses). It removes the active key only after that copy is stored. If removing the
active key fails, it restores the key first, as reset's rollback does, and drops
the copy only after the restore succeeds. If there is no key, nothing changes. A
registration-only removal, a refused deletion, a running VM and non-HVF engines
never touch the store. If archiving fails, the deletion is still reported as
complete, with a separate warning alert. It is never shown as a deletion failure.
Native CLI and legacy CLI have no command that deletes app-library bundles.

Evidence: 16 new XCTest cases (in-memory key stores, real deletion path,
scripted lease helper) pass with the existing deletion suites; they use new
interfaces, so the old code has no runtime baseline. Not live evidence; keys
left by earlier builds are untouched; no criterion or product state changes.
