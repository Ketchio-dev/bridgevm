# Windows kernel debugging (KD) over the HVF PL011 serial

Both remaining Windows bring-up frontiers stop at the same wall — a driver
aborts inside its `StartDevice`/post-start path and leaves no host-visible
reason:

- **HDA codec** (`crates/bridgevm-hvf/src/hda.rs`): the controller binds
  (`Status=OK`), `hdaudio.sys` enumerates codec 0 and reads the AFG's
  vendor/revision/function-group-type/power/subsystem-id, then stops *before*
  querying `SUBORDINATE_NODE_COUNT` (GET_PARAMETER `0x04`), so it never finds
  the DAC/pin widgets. A clean device removal reproduces it, so it is not a
  cached-state artifact — `hdaudio.sys` deterministically refuses to descend.
- **venus / viogpu3d WDDM** (`viogpu3d` on ARM64): `DxgkDdiStartDevice`
  succeeds then the adapter fails post-start (Code 43), no event-log reason.

Both need guest-kernel visibility. This is the transport for it.

## Transport (implemented, committed)

`BRIDGEVM_KD_SERIAL_SOCKET=<path>` makes the probe
(`examples/hvf_gic_boot_probe/kd_serial_bridge.rs`) bind a non-blocking
Unix-domain listener at `<path>` and bridge it to the guest's PL011:

- guest UART TX (KDCOM transmit) → socket
- socket bytes → guest UART RX (KDCOM receive)

It pumps on every pre-run drain and forces a 20 ms service wake so the pipe
keeps flowing while the guest is halted at a breakpoint (no vCPU exits). The
bridge owns the serial for the run, so do not combine it with the boot-marker
scanner. Unit tests cover the pure byte pump and a real socket round-trip.

## Guest configuration (debuggee)

In the target Windows (over the agent channel or offline), enable KDCOM on the
PL011, which Windows sees as COM1:

```
bcdedit /debug on
bcdedit /dbgsettings serial debugport:1 baudrate:115200
```

Reboot. KDCOM now drives COM1 (our PL011). NOTE: with `/debug on` Windows takes
COM1 for the debug protocol, so the firmware/boot-log serial output stops being
plain text — that is expected while KD is enabled.

## Debugger side (external — not yet wired end-to-end)

WinDbg must run on a Windows host and connect to the same byte stream. Options,
easiest first:

1. **socat/nc relay + WinDbg named pipe.** Relay the Unix socket to a TCP port
   or a named pipe WinDbg understands:
   `socat UNIX-CONNECT:<path> TCP-LISTEN:5555,reuseaddr` then point WinDbg at a
   `com:pipe` / KDNET-style transport reaching that port. (KDCOM-over-serial in
   WinDbg uses `-k com:port=\\.\pipe\<name>,baud=115200,pipe`.)
2. **VZ Windows VM as the debugger.** Run WinDbg inside the app's VZ
   `windows-11-arm64` VM and bridge the host Unix socket into that VM's serial
   or a vsock/TCP the VM can reach.

KDNET (network KD) is not an option: our virtio-net is not a KDNET-supported
NIC, so serial (KDCOM) is the path.

## First diagnostic targets once attached

- HDA codec: break in, set a breakpoint in `hdaudio!...FunctionGroupStart` /
  the AFG enumeration, and read why it returns before `GET_PARAMETER(0x04)` —
  compare the codec parameter responses it validates against the spec table.
  Verb trace of the current wall: `~/BridgeVM/hda-codec-verb-trace-rescan.log`.
- viogpu3d: break on `DxgkDdiStartDevice` return / the adapter post-start path
  and read the failing status.
