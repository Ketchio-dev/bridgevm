# A8 dynamic resize — re-confirmed as a guest miniport blocker (2026-08-01)

Re-ran `scripts/verify-dynamic-resize.sh` on the BAR2-fixed build and the
ATTACH_BACKING-resident driver, to check whether the resize failure shared a root
cause with the `vkCreateInstance` spin. It does not.

Run: `a8-fix-025747`, `REQUEST=1600x900`, target
`canonical-attach-resident-20260731.raw`.

```
guest before: 800x600
host accepted resize=1600x900
A8 resize: FAIL (requested=1600x900 guest=800x600)
```

## The host side is complete

- `request_virtio_gpu_resolution` returned true, so the geometry was accepted and
  the config-change interrupt was armed (`live_input.rs:105-107`).
- The guest **did** re-query: `GET_DISPLAY_INFO` appears twice in the GPU trace,
  at `seq=33` and `seq=1598`, the second one after the resize request. Both
  returned `OK_DISPLAY_INFO` with a full 408-byte payload.

So the notification path works end to end: the host published new geometry, the
guest noticed and asked for it.

## The guest does not act on it

Every `SET_SCANOUT` in the run carries the same rectangle, before and after the
re-query:

```
seq 49 .. 978    SET_SCANOUT 1024x768
seq 1723, 1753, 1792, 1819, 1830, 1895   SET_SCANOUT 1024x768   (after seq 1598)
```

The final `SET_SCANOUT` is `rect_w=0 rect_h=0`, which is the shutdown teardown,
not a mode change.

`EnumDisplaySettings` in the guest still reports 800x600 as current with no
1600x900 entry, matching the earlier A8 evidence.

## Conclusion

A8 is not blocked by the host event/interrupt path and was not affected by the
BAR2 defect. The blocker is inside the `viogpu3d` miniport's VidPN/mode
enumeration: it re-reads display info and then keeps its existing mode. Fixing
it means changing the guest driver, not BridgeVM.
