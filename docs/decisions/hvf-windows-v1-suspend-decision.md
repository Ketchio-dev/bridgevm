# Windows HVF v1 suspend decision

Status: accepted for v1 (2026-07-22)

BridgeVM's custom Windows HVF engine does not expose suspend/resume as a v1
product capability. The UI must direct users to shut down and start the VM
again. This is a deliberate fail-closed boundary, not an untracked omission.

The probe contains an experimental, versioned checkpoint format for RAM, vCPU,
GIC, and emulated-device state. Checkpoint commits are atomic and durable and a
successful capture exits the probe, but the path is not a product suspend
contract because it currently:

- accepts only one vCPU;
- has no owner-thread rendezvous for secondary vCPUs;
- does not bind the checkpoint to immutable disk, firmware-vars, and device
  topology identities;
- has no application-level quiesce or Windows power-state handshake;
- has no end-to-end crash/reboot/upgrade compatibility evidence.

The experimental environment variables remain developer-only. Product code
must not advertise a suspended state or consume a checkpoint until all five
conditions above are implemented and covered by destructive restore testing.
