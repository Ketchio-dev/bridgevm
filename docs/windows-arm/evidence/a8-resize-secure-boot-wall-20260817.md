# A8 on the net-live image: Secure Boot closes the door viogpu3d needs (2026-08-17)

## What was attempted

`scripts/verify-dynamic-resize.sh` against the canonical `net-live-20260724`
image, requesting 1600x900. The 2026-08-01 evidence proved adoption works when
the viogpu3d display is up (`DISPLAY2 cur=1280x1024 modes=28`, three requests
adopted exactly).

## What happened instead, hop by hop

1. The run reported `a8_resize=fail` with empty before/after resolutions.
2. The guest's display list held only `\\.\DISPLAY1 current=800x600 modes=1` —
   the Microsoft Basic Display Driver. No viogpu3d display existed to resize.
3. The driver query the verifier already performs named the cause exactly:

   ```
   PCI\VEN_1AF4&DEV_10F7...  ConfigManagerErrorCode: 52
   Problem: CM_PROB_UNSIGNED_DRIVER
   ```

   The adapter is on the bus and the driver is injected; Windows refuses to
   load it because it is test-signed and the boot policy does not allow that.
4. The obvious remedy was tried live: `bcdedit /set testsigning on` inside the
   guest answered

   ```
   The value is protected by Secure Boot policy and cannot be modified or deleted.
   ```

So on this image the chain is closed: Secure Boot forbids testsigning,
testsigning is the only no-cost way to load the test-signed viogpu3d, and
without viogpu3d there is no display that can adopt a host resize. The
2026-08-01 success ran on an image state where the driver did load, which this
image no longer permits.

## What this decides for 1.0

The plan's P3 risk clause anticipated exactly this: "vTPM+SB images may refuse
testsigning; if so, images/instructions change to SB-off for 1.0 and that is
documented." That clause is now in force, from measurement:

- The guided install for 1.0 must either provision the guest with Secure Boot
  disabled, or enable testsigning before first boot (offline `bcdedit /store`
  against the image's BCD), and say plainly that 3D acceleration and dynamic
  resolution require it.
- Host-side resize plumbing needs no work: the host accepted the request and
  armed the config event in every earlier measurement; the 8/1 doc already
  ruled a host defect out.
- Paid attestation signing of viogpu3d stays out of scope by the no-payment
  constraint.

## Reproduction

`~/BridgeVM/runs/resize-verify-20260817-223919` (failed verify, driver query)
and `~/BridgeVM/runs/resize-ts-*` (the refused bcdedit) hold the run logs.
