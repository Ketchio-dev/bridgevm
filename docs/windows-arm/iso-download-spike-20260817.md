# Can BridgeVM fetch the Windows 11 ARM64 ISO itself? (spike, 2026-08-17)

## Question

PLAN.md's D6: implement in-app ISO download against Microsoft's official
consumer endpoints if a spike proves it feasible without payment; otherwise
ship the guided-manual flow and record the decision. This is that record.

## What was probed, with results

The consumer flow behind microsoft.com/software-download, exercised with plain
`curl` from this machine:

1. **Edition discovery** — `GET /en-us/software-download/windows11arm64`
   returns the product option `3324` ("Windows 11 (multi-edition ISO for
   Arm64)"). Worked.
2. **Session registration** — `GET vlscppe.microsoft.com/tags?org_id=y6jn8c31&
   session_id=<uuid>` returned 200. Worked.
3. **SKU listing** — `GET .../api/getskuinformationbyproductedition?...
   ProductEditionId=3324&sessionID=<uuid>` returned clean JSON naming SKU
   `20086` ("English", product display name "Windows 11 Arm64 25H2__V2").
   Worked.
4. **Download-link issuance** — `GET .../api/GetProductDownloadLinksBySku?...
   SKU=20086&sessionID=<uuid>` returned:

   ```json
   {"Errors":[{"Key":"ErrorSettings.SentinelReject",
               "Value":"Sentinel marked this request as rejected.","Type":8}]}
   ```

   A browser User-Agent made it worse (steps 3 and 4 both degrade to an HTML
   block page); the default curl UA reaches step 4 and is rejected there by
   name. **Blocked.**

So three of four hops are open and the last one is gated by Microsoft's
Sentinel anti-automation service, which evaluates browser fingerprints beyond
anything a polite HTTP client presents.

## Why 1.0 does not fight this

- Sentinel's behaviour is adversarial and changes without notice; a fetcher
  that works today is a support ticket next month. The failure mode would sit
  in the app's critical first-run path.
- Evading a bot-check is also outside what this project is willing to ship;
  the constraint has always been official endpoints, politely used.

## What 1.0 ships instead

The guided-manual flow that already exists in `HvfWindowsInstall`: the app
sends the user to Microsoft's download page (the human passes Sentinel by
being one), then validates the chosen ISO before install. `docs/install.md`
states the requirement.

## What would reopen this

If Microsoft publishes an unauthenticated link-issuance path (or the Fido
script's UUID route stabilises for ARM64 consumer ISOs), the fetcher becomes a
`URLSession` walk of the same four hops above, and steps 1-3 of this document
are its request map.
