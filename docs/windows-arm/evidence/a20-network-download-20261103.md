# A20 network download, 2026-11-03

Two live runs on one Mac (Apple M4 Max, macOS 27.0.1), each a cold boot of an
APFS clone of the staged Windows 11 Arm test image with the product's network
device: virtio-net with the VMM's NAT backend (`--virtio-net`). Probe built
from main at the merge of PR #461.

Method (`scripts/perf/vm_bench.py run --net --only=net-download`): the host
serves a 256 MiB payload over HTTP on its LAN address, which the guest reaches
through the NAT as an external host. The guest makes one warm-up GET, then a
timed GET, computes the SHA-256 of the received bytes and compares it with the
host's SHA-256 of the payload.

| run | engine | bytes received | SHA-256 match | throughput |
|---|---|---:|---|---:|
| 1 | BridgeVM | 256 MiB | yes | 298.9 MB/s |
| 2 | BridgeVM | 256 MiB | yes | 338.1 MB/s |
| ref | QEMU/HVF 11.0.2, `-nic user,model=virtio-net-pci` | 256 MiB | yes | 255.7, 226.5 MB/s |

Runs were interleaved BridgeVM, QEMU/HVF, BridgeVM, QEMU/HVF. 2 of 2
BridgeVM runs met the A20 statement. The image, its path and the payload
bytes are not retained here.
