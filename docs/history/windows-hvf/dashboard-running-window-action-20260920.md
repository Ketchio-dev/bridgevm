# Dashboard running-window action regression

## Observation

The dashboard showed an HVF VM as running and relabelled its primary action
from `시작` to `창 열기`, but clicking the action produced no visible result.
The observed app process came from the retained development preview rather
than `/Applications/BridgeVM.app`, and its live runtime used the historical
experimental 3D path. This record does not promote that path or any capability.

## Deterministic cause

The dashboard routed both labels to `ControlModel.start()`. That method
intentionally returns immediately when `running` is true, so the running-VM
button was a deterministic no-op. The advanced view's separate display action
already used `HvfDisplayWindowController` and remained a manual workaround.

## Repair and limits

Source `395932530e234a143f86972679ff8680dc47dc30` gives the dashboard action a
separate running path. It resolves the retained HVF session, adopts the exact
running process when needed, and presents the existing display controller only
after attachment is established. Missing or unattachable sessions now publish
an explicit status message instead of pretending that a window opened. A
stopped VM still routes to the existing start lifecycle.

Four deterministic contracts cover stopped start routing, adoption before
presentation, retained-session presentation, and fail-closed errors. All four
Swift shim suites passed: 425 BridgeVMApp, 818 BridgeVMControl with the two
existing intentional skips, 62 AppleVzRunnerCore, and 105 BridgeVMProductE2E.
Structural budgets passed without raising an existing ceiling; the complete project check passed, and its log SHA-256 is `58a88af2096e1970a657be75da65d73cea582433e03539df43def59154142a80`.

These checks prove the action routing and attachment boundary. They do not
prove a WindowServer-visible display for a real guest. The installed app must
be replaced only after exact-source hosted CI is green, and a live app run must
then confirm that the window appears and receives frames. A9 and A11 remain
OPEN, product state is unchanged, and 3D remains outside the release path.
