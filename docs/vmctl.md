# Control a development VM from the terminal

Use the native [`bridgevm app` CLI](app-cli.md) for saved VM inventory,
start, status, normal shutdown and powered-off snapshots. It asks the running
app to manage its own process; it does not click the app or open a VM window.
Start the installed app once first:

```sh
open /Applications/BridgeVM.app
/Applications/BridgeVM.app/Contents/Resources/target/release/bridgevm app list
```

For development and automated checks, `scripts/vmctl.py` adds Windows guest
commands, input, display-mode requests and 2D framebuffer capture. It needs
Python 3.9+, a current installed app and its guest agent. No Python packages,
Accessibility permission, screen recording permission, SSH server in Windows
or system clipboard access are needed.

From the source checkout, substitute the exact ID returned by `list`:

```sh
python3 scripts/vmctl.py list --json
python3 scripts/vmctl.py start MY_VM --json
python3 scripts/vmctl.py status MY_VM --json

# PowerShell source goes to the guest, without host-shell interpolation.
python3 scripts/vmctl.py exec MY_VM <<'PS'
Write-Output 'Guest command works'
Get-CimInstance Win32_OperatingSystem | Select-Object Caption,OSArchitecture
PS

python3 scripts/vmctl.py exec MY_VM --file /absolute/path/check.ps1
python3 scripts/vmctl.py key MY_VM win+r
python3 scripts/vmctl.py text MY_VM 'notepad.exe'
python3 scripts/vmctl.py key MY_VM enter
python3 scripts/vmctl.py pointer MY_VM click:16000x16000
python3 scripts/vmctl.py pointer MY_VM rightclick:16000x16000
python3 scripts/vmctl.py resize MY_VM 2560x1600
python3 scripts/vmctl.py screenshot MY_VM /tmp/guest.png
python3 scripts/vmctl.py stop MY_VM --json
```

Pointer coordinates cover the whole guest desktop from 0 to 32767 on each
axis. `text` sends Unicode through the guest agent without using the clipboard.
With a current agent and compatible ARM64 editor, it dispatches text on the
focused editor thread after earlier queued keys. Windows 11 Notepad uses one
undoable selection replacement; other compatible controls receive Unicode
character messages. Requests split at 64 KiB of UTF-8 without splitting a
Unicode scalar. Older agents and unsupported editors retain receipt-paced
codepoints. That path can still corrupt long commits in some Windows apps.
The whole operation shares one reply deadline, and an unknown result is never
automatically replayed. `key` accepts
the guest key protocol, including `enter`, `ctrl+v`, `win+r` and held-key
forms such as `down:ctrl` / `up:ctrl`. An input receipt confirms event insertion
or window-message dispatch, depending on the negotiated path; verify the application result separately. It does not
prove that an arbitrary application changed or saved its document.

`exec` runs PowerShell as the existing guest agent user, with that user's
permissions. Its exit code is returned to the shell (unsupported exit values
map to 1). Commands and their output remain in the VM's local run log, so do
not use it to transmit passwords or keys. `--timeout 120` before the command
extends the default 40-second reply wait. A timeout does not cancel guest work
and never resends it automatically.
Windows' `cmd.exe` command-line limit also bounds encoded PowerShell source.
For a larger script, copy it through the shared folder and invoke that guest
path from a short command. Replies over 8 MiB are refused; write large results
to the shared folder instead.

Guest operations require one live app-owned own-HVF run whose saved
configuration still matches. The tool checks the run identity before writing
and after receiving a reply, and reads only new command-specific output.
It uses the existing local `hvf.ctl` developer channel, not a new network
listener. Start/stop retain the native CLI's ownership and cleanup behavior.
This is a source-checkout developer tool, not an additional packaged app API.

Screenshots read the last complete published **2D** framebuffer using its
sequence guard and save a new PNG without overwriting another file. They do
not capture the Mac desktop, app chrome or hardware cursor, and a published
frame can be stale or black if the guest display is asleep. A successful
capture is not a guest-health or display-correctness check. 3D/IOSurface
capture is outside this command. `resize` updates the VMM's display geometry
and asks Windows to use it, following the same channel as the app window.
It can resynchronize the display after a guest restart while the VM window
was closed. The app also reapplies an explicit saved startup resolution after
guest restart when no display window has been laid out. Once a window opens,
its size and Retina scale take precedence. Resizing needs the guest driver;
an unavailable mode returns failure instead of pretending the resize worked.

Optional short command, from the source checkout:

```sh
chmod +x scripts/vmctl.py
mkdir -p "$HOME/.local/bin"
ln -s "$PWD/scripts/vmctl.py" "$HOME/.local/bin/bvctl"
bvctl status MY_VM --json
```

The chosen directory must be on PATH. Existing destinations are not replaced.
Use `--bridgevm /absolute/path/to/bridgevm` for another installed package and
`--library /absolute/library/path` for a library already managed by that app.
Keep these global options before the command. See `--help` for all commands.
