# Resident input keyset negotiation

`INPUTCAPS <uuid>` retains the existing successful v3 response:

```text
BVINPUT_CAPS <uuid> 3 TEXTINPUT KEYINPUT POINTERINPUT 65536
```

After the base marker an agent adds, in this order, one separate line per
keyset that its loaded C# module dry-builds completely:

```text
BVINPUT_KEYS <uuid> SHIFT_ENTER_SPACE
BVINPUT_CHORDS <uuid> CTRL_SHIFT_CHORDS
```

`SHIFT_ENTER_SPACE` is Shift+Enter/Space. `CTRL_SHIFT_CHORDS` is Ctrl+B, D–U, W,
Enter, Tab, Ctrl+Shift+Tab, Shift+F1–F12 and Shift+Delete (v3 has Ctrl+A/C/V/X/Y/Z).

The host accepts each extension at most once and in that order, after the exact
base marker and before the matching `BVAGENT END INPUTCAPS <uuid>`, inside a
successful command receipt. UUID, token and spelling must match exactly. A
foreign, malformed, duplicate, misordered or premature extension fails
negotiation; one after the completed envelope cannot upgrade support. Restart
or expiry invalidates the pending receipt. Future keysets use a new keyword.

A new host connected to an older resident refuses chords outside its
advertised keysets before queueing or sending them, with existing refusal
feedback; pending and later ordinary input remain available, without legacy HID
after ordered activation. An older host ignores marker keywords it predates and
keeps v3. A new agent with an older C# module loaded advertises only what that
module builds. Before ordered activation the VMM's HID route sends these chords
itself. Target cancellation keeps the keyset; a service restart or new owned
boot clears it. Marked text stays with the macOS input method.

Deterministic tests cover host admission, receipt boundaries, control-file
dispatch and dry-built Windows events; none injects native input. They do not
prove Windows foreground delivery, application consumption, a particular Korean
input source or closure of the legacy mutable-clipboard race. Neither
`BVINPUT_INSERTED` nor a capability marker acknowledges application use.
