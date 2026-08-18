# B4 batch: the click is lost nine times in ten, and not where we thought (2026-08-17)

## The rate

`scripts/measure-pointer-latency-batch.sh` ran the single-run measurement ten
times against fresh clones of the same image. Every run had a real target: the
pre-click capture shows the PPSSPP Graphics Error dialog with its OK button at
the aimed coordinate, and in every failing run the 1000 ms capture is
byte-identical to it.

```
reacted 1/10        (run 7: first_changed_ms=1000)
HID fired=true 10/10
```

So B4's earlier "one of two runs lost the click" was optimistic. The click is
delivered by the host and ignored by the guest roughly nine times in ten.

## Hypotheses measured and killed

**Report pacing (the plan's prime suspect).** The 30 ms spacing between the
move and the button reports was raised to 300 ms via
`BRIDGEVM_XHCI_REPORT_INTERVAL_MS` (the boot runner now forwards it): 0/3
reactions. Pacing is not the mechanism.

**Boot-protocol mismatch.** The HID summary shows `current_protocol=boot`, but
per-interface counters resolve it: interface 0 (keyboard) is boot, interface 1
(pointer) is `report` with the 6-byte absolute report it declares. The pointer
path is protocol-correct.

**Endpoint doorbells as a separator.** `dci3_doorbell count=0` and
`dci5_doorbell count=0` looked alarming until compared across outcomes: the
reacting run and a failing run show identical counters. Not a separator.

**The whole host-visible surface, in fact.** Diffing every `interrupt_*`,
`ring_*` and `event_ring*` counter between the reacting run and a failing run
finds no difference at all. As far as the host can observe -- enumeration,
protocol setup, report emission, ring state -- a lost click and a landed click
are indistinguishable.

## Where that leaves B4

The divergence is strictly inside the guest, after the point where the host's
instrumentation ends. The earlier evidence (b4-pointer-latency-20260816.md)
shows that when a click does land, the cursor appears at the aimed pixel, so
coordinate mapping is right; this batch shows the button transition itself is
what usually vanishes.

Next candidates live in the guest HID stack, not in BridgeVM's emitter:
capturing the guest's own event stream (Get-WinEvent / ETW on the HID class
driver) during a batch, or instrumenting the xHCI transfer-ring completion
against the exact TRB the button report rode, to see whether the guest driver
ever completed it. Both need guest-side tooling that does not exist yet;
neither is a host-side code change.

The registry's B4 stays open with this batch as its measured state.
