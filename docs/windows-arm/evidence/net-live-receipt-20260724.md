# D1 NET-LIVE receipt — live guest internet egress (2026-07-24)

## Result: PASS

One packaged-firmware boot of the agent-planted `wall-run-20260723.raw` lineage
proved live guest network egress through the BridgeVM userspace NAT: DNS
resolution, an HTTP 200, and ICMP echo, all captured with `exit=0` over the
resident agent channel, followed by a clean agent-driven shutdown.

## Run

- Evidence dir: `~/BridgeVM/runs/net-live-20260724/`
- Disk: reflink clone of `wall-run-20260723.raw` (pristine lineage untouched)
- Firmware: packaged `edk2-aarch64-secure-code.fd`
  (`BridgeVMControl-final2-20260723.app`)
- vTPM: reused unencrypted `wall-20260723-bootstrap-vtpm-state`
- NIC: `--virtio-net` userspace NAT backend
- Probe: `scripts/win-assets/bv-net-proof.ps1`
  SHA-256 `edb5270cde0b5e5e3b1e7056fbafae5c7e90c0975ff3545a02495bc69b557ad9`
- Agent reached `BVAGENT READY` at poll loop 7; `whoami exit=0`.

## Guest-side proof (agent OUT in run.log)

```
BVAGENT CMD powershell ... bv-net-proof.ps1 exit=0
NETPROOF begin utc=2026-07-24T07:33:54.6132998Z
NETPROOF dns=OK 172.66.147.243
NETPROOF http=OK StatusCode : 200
NETPROOF icmp=OK
NETPROOF adapter=Ethernet ipv4=10.0.2.15 gw=10.0.2.2
NETPROOF verdict=PASS
NETPROOF end utc=2026-07-24T07:34:02.0379628Z
BVAGENT CMD shutdown.exe /p /f exit=0
```

The full probe (DNS + HTTP 200 + ICMP + adapter) completed in ~8 seconds of
guest wall time.

## Host-side NAT corroboration (run.log)

```
virtio-net NAT stats: guest_frames=19848 dhcp_discover=1 dhcp_request=1
  dns_queries=96 icmp_echo=2 tcp_segments=19553 lease=10.0.2.15 tcp_flows=1 udp_flows=38
virtio-net NAT replies: arp=7 dhcp_offers=1 dhcp_acks=1 dns=96 tcp=406560 udp=107
```

Independent host-side confirmation: 96 DNS queries answered, 2 ICMP echoes, a
completed TCP flow, and a DHCP lease of 10.0.2.15 matching the guest adapter.

## Clean shutdown / storage integrity

```
stop: PSCI 0x84000008 (system off)
storage target effect summary: io_write_success_count=199 io_write_command_count=199
  io_flush_success_count=16 target_effect_class=present_successful_io_write
cleanup_status=0
```

Agent-driven `shutdown.exe /p /f exit=0` → PSCI system off, NVMe writes 199/199,
flushes 16, cleanup 0.

## Reproduce

```
bash ~/BridgeVM/work/dx-probe-assets/run-d1-net-proof.sh
grep -E 'NETPROOF|StatusCode : 200' ~/BridgeVM/runs/net-live-20260724/run.log
```
