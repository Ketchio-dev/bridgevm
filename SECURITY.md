# Security policy

## Report a vulnerability privately

Do not open a public issue for an unpatched vulnerability, private-media leak,
signature bypass, guest-to-host escape, or a way to weaken a fail-closed trust
boundary.

Use GitHub's
[private vulnerability reporting](https://github.com/Ketchio-dev/bridgevm/security/advisories/new)
for this repository. Include:

- the affected commit, tag, or release;
- host macOS and Apple-silicon model;
- guest operating system and configuration, without license keys or private
  media;
- minimal reproduction steps and expected impact;
- logs or a proof of concept with secrets and personal paths removed;
- whether you believe exploitation is active.

You should receive an acknowledgement through the private advisory. Please do
not disclose details publicly until a fix and release plan are agreed.

## Supported versions

BridgeVM is currently an Engineering Preview. Security fixes target current
`main` and the newest General Preview. Historical releases remain available as
records but may not receive backports. In particular, `v1.0.0` predates the
current fail-closed Windows driver policy and is not recommended.

## Important trust boundaries

- The General Preview is ad-hoc signed, not Developer ID signed or notarized.
  Its installer verifies integrity relative to the GitHub release, not a legal
  publisher identity.
- The General Preview contains no Windows test driver and exposes only 3D-off
  Windows install/import.
- Graphics Lab test certificates and packages are development inputs. They do
  not establish Microsoft kernel-policy signing provenance.
- Windows media, VM disks, UEFI vars, vTPM state, recovery keys, and private live
  receipts must not be attached to public issues or pull requests.
- Public GitHub Actions never run on a personal self-hosted machine and never
  receive private Windows media.

The full design is in the [security model](docs/security/model.md) and the
[distribution channel contract](docs/distribution-channels.md).
