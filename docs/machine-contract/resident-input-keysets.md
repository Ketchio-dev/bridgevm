# Resident input keyset negotiation

`INPUTCAPS <uuid>` retains the existing successful v3 response:

```text
BVINPUT_CAPS <uuid> 3 TEXTINPUT KEYINPUT POINTERINPUT 65536
```

An agent that can dry-build both Shift+Enter and Shift+Space as four key events
adds this separate line after the base marker:

```text
BVINPUT_KEYS <uuid> SHIFT_ENTER_SPACE
```

The host accepts the extension only once, after the exact base marker and
before the matching `BVAGENT END INPUTCAPS <uuid>`, inside a successful command
receipt. The UUID, token, ordering and spelling must match exactly. A foreign,
malformed, duplicate or premature extension fails negotiation. An extension
after the completed envelope cannot upgrade support. Restart or expiry
invalidates the pending receipt; support becomes usable only after completion.

The base v3 keyset excludes `shift+enter` and `shift+space`. A new host connected
to an older resident refuses those two chords before queueing or sending them,
using existing submission-refusal feedback. Pending and later ordinary input
remain available; the host does not mix in legacy HID after ordered activation.
An older host ignores the separate extension and keeps its v3 ordered stream.
If a new PowerShell agent encounters an already-loaded older C# input module,
the optional dry-build probe fails and it advertises only base v3 support.

Host target cancellation preserves the negotiated keyset. A service restart or
new owned boot clears it and requires a fresh successful negotiation. Marked
text remains owned by the macOS input method, including Shift+Enter/Space.

Deterministic tests cover host admission, receipt boundaries, control-file
dispatch and dry-built Windows events. No native input is injected by the
guest contract tests. These results do not prove Windows foreground delivery,
application consumption, a particular Korean input source or closure of the
legacy mutable-clipboard race. `BVINPUT_INSERTED` remains an insertion-count
receipt; neither it nor the capability marker acknowledges application use.
