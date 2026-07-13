# BridgeVM Windows D3D10 probes

Build the Windows ARM64 probes on macOS with:

```sh
scripts/build-hvf-windows-d3d10-smoke.sh
```

`bridgevm-d3d10-smoke.exe` verifies initialized-texture copy/readback and RTV
clear/readback. Its default binds the RTV; `--unbound` preserves the regression
gate for the pinned Mesa VirGL unbound-clear patch.

`bridgevm-d3d10-draw-smoke.exe` dynamically loads the Windows inbox
`d3dcompiler_47.dll`, compiles VS/PS 4.0 shaders, draws a fullscreen magenta
triangle, and verifies the 64x64 staging readback. It must not pass unless the
draw result is present; a clear-only or zero-filled result fails.

`bridgevm-debug-runner.exe PROGRAM` captures `OutputDebugString` messages from
one child process and returns the child's exit code.
