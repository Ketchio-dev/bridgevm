# Owned restore recovery — 2026-10-14

Evidence rank: **test**. This follows the preservation fix recorded in
[the prior checkpoint](autonomous-preservation-20261014.md). It restores
ordinary retry for authenticated newly produced staging while preserving
unproven staging. No live gate, guest boot or release criterion is closed.

## Reproduction and design

Preserving complete preexisting staging protected another VM's snapshot
export, but also refused the product's own interrupted restore. Using the
unchanged T22 observers with synthetic64MiB sparse disks and tiny vars,
both initial and swap staged-disk verification stops were authenticated,
the helper was killed and reaped, and ordinary CLI retry exited1. Selected
and source bytes remained intact. A diagnostic harness resolver was rejected;
T22 requires ordinary retry and its stop points, four boots and sample rules
remain unchanged.

The product now creates an immutable bounded ownership record before opening
the staged disk for verification. A hardlink pins the same record inode in
the original stage. The record binds root, self, original stage and all three
member identities, lengths and the already verified source manifest. It and
both directory names are synchronized before the observer's read boundary.
Restart cleanup requires intact identities, strict shape and matching hashes.
Copied, torn, missing, malformed or changed records never authorize deletion.

Current-attempt cleanup retains its captured ownership. After publication,
retirement checks the exact current directory/pin without rehashing writable
media. A single-link pin remains in current after the root record is retired;
repeated restore, mutable-current, digest, resolve and export controls pass.

## Portability and preserved generations

An actual exFAT baseline proved that existing first restore succeeds while
hardlinks returnENOTSUP45. Mandatory links would have regressed that path.
Only exact ENOTSUP/EOPNOTSUPP therefore permits removal of newly owned seal
metadata and continuation through the existing unsealed restore. Other I/O
errors fail closed. First implementation rejected native AppleDouble members
before probing link support; that real exFAT failure is retained. The repair
checks private directories and original/fresh identities before probing links,
and applies strict member shape only after a successful pin. It adds no broad
sidecar deletion rule or generic fallback.

An old current created before sealing can have no pin. After exchange and
partial cleanup, its remaining vars must not become ordinary copy debris.
A failing baseline demonstrated that loss. Independent review then found a
second route: successful publication can leave old staging after best-effort
cleanup, and direct retirement bypassed a recovery-only guard. The second
baseline also lost old vars. Both failures remain recorded. The final shared
retirement operation refuses any remaining staging before unlinking the root
record, preserving exchange knowledge across both direct and later retries.
Explicitly moving the old staging outside managed storage permits recovery.

## Final focused proof

The reviewed source conclusion is
`3cd5f89319a2dad5df5e7c08936b4bb89d946d17`.
Final patch SHA-256:
`3133551f27f10e2e25c4207fe7518662aaf998d4f21bbd6bd9c81e631aa902eb`.
Final author receipt SHA-256:
`e95be6e829ebdadbbca188525dee6fe3274e6876f539f88601589abb6f7bb275`.
Independent approval SHA-256:
`53cbb81bfd3e1e772e7fe876cee001d2f7dfb5b74d428e8ca5d070b1c8de4759`.

Focused snapshot210 and public-API1 cases passed, as did Clippy and formatting.
The exact final helper again crossed both unchanged authenticated FD-stop
boundaries, and ordinary retry exited0 with restored selection and unchanged
inputs. Real exFAT create, verify, restore, selected digest, export and exported
verification all exited0; no ownership/staging residue remained and the owned
device was released. Actual app CLI export B into A's managed staging survived
two refusals with all input/selected bytes unchanged; explicit move then retry
succeeded. Integration rehashed31 source and25 evidence files;23 new modules
were registered at actual structural size with no existing ceiling increased.

Earlier fixture lease/order failures, distinct-source fixture mistakes,
exFAT regression, the two old-generation loss baselines and a test-only lint
failure remain retained. Earlier passing receipts are intermediate and do not
erase those failures. Combined exact-source local and hosted checks remain
separately required.

## Limits

Complete-before-seal, unsealed legacy, torn metadata and interrupted cleanup
may still require explicit resolution. Unsupported-link filesystems retain
that conservative unsealed behavior. Ownership checks cover cooperating
managed operations, not arbitrary hostile same-UID namespace mutation or
power-loss durability. These synthetic tests do not substitute for T22's live
workload or promote A9, A11, A19 or the product state.
