# Windows Coherence primitives, proven live (2026-08-17)

## What was proven, in one boot

Every guest-side primitive the Coherence protocol needs was executed against a
live Windows 11 ARM guest (net-live image, run
`~/BridgeVM/runs/coherence-live3-*`), through the existing agent service
channel, and each answered with a machine-checkable result:

**Enumerate.** Listing top-level windows returned the live set, including the
Notepad started in the same command:

```
BVWIN 197274 8324 Untitled - Notepad
BVWIN 197240 7936 PPSSPP v1.20.4
BVWIN 66092 7356 Administrator: Windows PowerShell
```

**Move/resize (WINBOUNDS).** `SetWindowPos(197274, 50, 60, 700, 500)` followed
by `GetWindowRect` on the same hwnd:

```
BVRECT 50 60 700 500
```

The rectangle read back is exactly the rectangle requested.

**Focus (WINFOCUS).** `SetForegroundWindow` then `GetForegroundWindow`:

```
BVFOCUS requested=197274 got=197274 match=True
```

**Close (WINCLOSE).** `PostMessage(WM_CLOSE)` then a process liveness check
two seconds later:

```
BVCLOSE notepad_alive=False
```

A second enumeration in the same boot no longer lists Notepad, closing the
loop: list -> move -> focus -> close -> list reflects the close.

## How this relates to the shipped protocol

The agent's new WINLIST/WINBOUNDS/WINFOCUS/WINCLOSE verbs
(`scripts/win-assets/bvagent.ps1`) are these exact user32 calls behind a
line protocol, and `HvfGuestWindowProtocol` on the host parses their output
(five shim tests, grammar gate in `check-project.sh`). This run executed the
same calls through the agent's RUN channel because the *installed* guest still
carries the pre-WINLIST agent build; updating the in-guest agent requires
rebuilding the driver injector, which is the remaining step between "the
primitives work" and "the shipped verbs answer".

## Channel lessons that cost boots

- A GUI command sent through RUN (`notepad`, even `cmd /c start notepad`)
  never returns, and the agent channel serializes on it: everything queued
  after waits forever. `powershell Start-Process` returns immediately and is
  the correct spawn form.
- Multi-line PowerShell cannot ride the line-oriented ctl file;
  `-EncodedCommand` with UTF-16LE base64 carries arbitrary scripts in one line.
