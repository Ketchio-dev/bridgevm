# Changelog

All notable changes to BridgeVM are documented in this file.

## Unreleased

### Changed

- A running VM uses about 190 MB less memory. The 3 MiB firmware no longer
  fills a 64 MiB region, and the UEFI variable file is read straight into the
  flash device instead of through two 64 MiB copies that stayed resident
  (measured: 6397 to 6208 MB footprint for a 6 GiB Windows VM at idle).
- The app now states and enforces macOS 15 as its minimum. The engine uses the
  Hypervisor.framework GIC API that first shipped in macOS 15, so on macOS 14 a
  VM could not start even though the app opened.
- The Fast VZ (Linux) and QEMU compatibility engines were removed from the app.
  A VM saved with one of them stays in the library, marked as no longer
  supported, with its files untouched, and can be deleted from there. The
  command palette, "Start when BridgeVM opens" and automatic startup no longer
  offer to start it.
- `bridgevm app start` runs the same start as the Start button: a failure now
  prints the app's own sentence with the underlying error, and a VM whose
  settings were saved after the app loaded them starts with the saved
  settings instead of being refused. Stop and install no longer refuse after
  a settings change either; install uses what is saved. The JSON schemas
  changed accordingly (`bridgevm.app-start.v2`, `bridgevm.app-stop.v2`,
  `bridgevm.app-install-command.v2`, `bridgevm.app-create-windows.v2`, which no
  longer carries `configurationDigest`).
- A VM started from the CLI keeps booting if Windows takes longer than 30 s to
  report ready, as after a Start click; the CLI still reports that readiness
  was not confirmed.
- A Windows install finishes sooner: the new disk is hashed once when it is
  sealed instead of six times (about 2–3 minutes less for a 64 GiB disk,
  estimated). Snapshot create, export and disk import no longer re-read the
  whole disk to verify right after writing it; a restore still verifies
  before it touches the VM. A restore reads the snapshot once instead of
  twice: it hashes only the staged copy it publishes (median 157 s to 74 s
  for a 64 GiB pair on one Mac). An A19 restore-boot lane, which creates,
  restores and exports a 64 GiB pair, took about 5 minutes instead of about 9
  on the same Mac.
- Two snapshot exports to different names in the same folder can run at the
  same time.

### Fixed

- VoiceOver no longer reads internal codes such as "hvf-import-invalid",
  "timed-out" or "none" after the error on the New VM sheet and the state and
  startup error on the VM status card. It reads only the text on screen.
- When VMs running at the same time together use more memory than your Mac can
  spare, each of them now says so in one sentence and suggests stopping one or
  giving the VMs less memory in Settings. Before, macOS just started swapping
  and Windows and the Mac slowed down with no explanation. Nothing is refused.
- The terminal installer works on a Mac without the Xcode Command Line Tools.
  It used to call Python and `lipo`, which on such a Mac only open the "install
  developer tools" dialog, so it failed or wrongly said the app was not Apple
  Silicon native. It now uses only tools that come with macOS, and every check
  it makes is as strict as before.
- Windows boot entries and Secure Boot settings now survive a crash, a power
  loss or a Force Stop that runs out of time while the VM runs. BridgeVM
  used to save the VM's UEFI settings only when the VM stopped cleanly, so
  every change made during the run could be lost: a boot entry added by a
  Windows update could go missing, or a Secure Boot revocation could be
  undone. Now a change is saved to the Mac's disk about a second after
  Windows makes it.
- The installer no longer replaces BridgeVM while BridgeVM or one of its VMs is
  running. It stops before downloading and asks you to shut down your VMs and
  quit BridgeVM first. Before, the old app kept running on top of the new
  app's files, and `--launch` only brought that old app to the front.
- `curl … | bash` run inside an older BridgeVM source folder no longer picks up
  that folder's installer and fails with misleading advice. If the downloaded
  installer does not match, the message now says so and suggests trying again in
  a few minutes.
- When GitHub can't be reached or rate-limits the newest-release lookup, the
  installer says so in one sentence instead of printing a Python traceback.
- Duplicating a VM stored on another drive no longer writes the full size of
  its virtual disk to the library: the copy keeps the disk's unused space
  empty, as the original has it.
- Moving a VM to another drive no longer writes the full size of its virtual
  disk there, so the move takes less time and space.
- When one of a VM's virtual processors crashes in BridgeVM, the VM now stops
  the way it does after other errors: its firmware settings are saved and its
  disk is flushed to the Mac's drive. Before, the stop crashed too and skipped
  that save. A crash in the middle of handling a device can still end the VM
  without saving its firmware settings.
- When a VM's process crashes, or Force Stop has to end it, BridgeVM now still
  flushes the VM's disk to the Mac's drive. Before, Windows' last saved
  changes could stay in the drive's cache and be lost if the Mac then lost
  power. If that flush fails, BridgeVM says it couldn't finish saving the
  Windows disk.
- When quitting BridgeVM waits for more than one VM to shut down, the
  "still shutting down" question now names the VMs it is still waiting for.
- The installers accept an Apple Silicon Mac when they are run from a Rosetta
  shell, such as an Intel Homebrew bash.
- The install guide no longer says Open Anyway is needed only once: each newly
  downloaded DMG version is blocked again on its first launch. The guide now
  also covers updating a DMG install.
- A Windows connection that BridgeVM has to drop, because Windows opened more
  than 256 at once or a closing connection sat idle, now fails at once in the
  Windows app instead of hanging until Windows gives up on its own.
- `bridgevm app install-status`, `install-cancel` and `install` now see a Windows installation
  started in the app window. Before, status and cancel answered `targetUnavailable`, and `install`
  failed as busy and then reported that failure while Windows went on installing. The code
  `targetUnavailable` also prints as "BridgeVM isn’t installing this VM." in text output.
- The library and VM page messages are clearer. Each VM no longer carries an
  "HVF" label (only a VM BridgeVM can’t run names its engine), and the alerts
  for moving, duplicating and deleting VMs no longer mention "registrations" or
  "bundles" and say what to do next.
- Stopping a VM that BridgeVM reconnected to after a relaunch no longer ends
  Windows mid-shutdown when the Mac sleeps during that shutdown. Its 180-second
  grace counted the time the Mac was asleep, so on wake the VM was ended at
  once.
- Quitting BridgeVM while it moves, duplicates or deletes a VM now asks to
  finish first and quits when it is done, as it already did while VM files were
  being prepared. Before, BridgeVM quit at once: a move cut short left the VM
  hidden from the library at the next launch.
- The Duplicate sheet now says that the duplicate starts with a new TPM, so
  on its first start you sign in to Windows with your password (your PIN won't
  work there) and, if BitLocker or device encryption is on, need the recovery
  key. Before, it only said the duplicate gets its own TPM.
- A VM whose disk and firmware settings files were removed outside BridgeVM
  (in Finder, say) can now be deleted. Every try said its library entry could
  not be removed from disk. BridgeVM still keeps the VM's folder when it holds a
  snapshot or any file BridgeVM didn't write, and after a delete that fails the
  library now shows what is still on disk.
- When a Mac port for port forwarding is taken, the notice now says another
  app or another BridgeVM VM may hold it, and to quit that app or shut down that
  VM. Two VMs set to the same Mac port (a duplicate keeps none, but a second VM
  set up the same way does) used to get only "Quit the app using it".
- Installing the display driver no longer pins the shared folder to Quick
  Access in Windows Explorer again after you unpinned it, and no longer leaves
  a `BridgeVM.unlocked-…` folder in `C:\ProgramData`. Before, BridgeVM kept its
  note that it had already pinned the folder in a place the driver install
  moves aside.
- Moving a VM whose files are on a disconnected drive now says "BridgeVM can't
  find this VM's files. Connect the drive that holds them, then try again."
  before it asks for a folder, and Show in Finder says the same. Before, Move
  asked for a folder and then said it could not confirm the move, though
  nothing had moved.
- Messages in the VM window and VM settings now use plain English. They call
  the in-Windows agent Guest Tools, no longer show helper output, byte counts
  or error codes, and say that a new display driver needs Windows to shut down
  and the VM to start again, not a restart inside Windows. The disk setting now
  says it waits until each Windows save reaches your Mac’s disk.
- Windows connections left idle, such as an SSH session or a database client,
  are no longer cut after 5 minutes without Windows being told. BridgeVM now
  keeps an idle connection open for at least 2 hours, as home routers do.
- If BridgeVM quits unexpectedly while it installs Windows, the installation
  now stops too, within a few seconds. Before, the installer VM kept running
  unseen with 4 GB of memory, and trying the installation again started a
  second one next to it.
- If you choose Deny or Cancel when macOS asks for your Mac password at Start,
  which can happen the first time after a BridgeVM update, BridgeVM now says it
  needs permission to use the VM's security key and to choose Always Allow
  next time. Before, it told you to unlock your login keychain, which was
  already unlocked. A locked keychain still asks you to unlock it.
- A Windows VM can start again after its VM process was killed or crashed. The
  VM's TPM helper kept running on its own and held the VM's TPM state, so every
  later start failed until you restarted the Mac. Start now stops that leftover
  helper first; a helper that belongs to a running VM is never touched.
- Importing a RAW Windows disk from another drive now copies only the space
  Windows uses, and no longer asks for room for the disk's full size: a
  256 GiB disk holding 40 GB needs about 40 GB free. Before, the import was
  refused unless the Mac had room for all 256 GiB, and then wrote all of it.
  Exporting or restoring a snapshot on another drive keeps unused space
  free the same way.
- TPM, import and installation messages now use plain words. The TPM card in
  Advanced Controls says "TPM" instead of "vTPM" and no longer shows
  fingerprints, receipt paths or key sizes. The startup checklist says "Windows
  disk" and "Windows boot settings file", and asks you to reinstall BridgeVM
  when an app component is missing instead of naming its path. Import explains
  how to make a recovery code file private in Finder instead of Terminal, the
  installer says "Canceling…" and "Canceled", and the library start screen
  says what went wrong instead of showing an internal code.
- If BridgeVM quit while a finished Windows installation was being saved, Start
  Installation now installs again. Before, every try said BridgeVM couldn't
  recover the interrupted installation, and deleting the VM was the only way
  out.
- A VPN tunnel or call running in Windows over UDP recovers after the Mac
  switches networks. Its traffic kept going to a connection tied to the old
  network, so it stayed dead for as long as Windows kept sending.
- When Windows can't start because BridgeVM couldn't use the VM's key in your
  login keychain, or because a move or installation cleanup hasn't finished,
  the message is now one plain sentence that says what to do. Before, it said
  "review the error below" and showed a raw system error or an internal note.
- More Actions > Refresh on a VM's page now also reads the library again, as
  messages that say "Refresh the library" expect. Before, it only checked
  whether the VM was running.
- Copying a large file or folder in Finder, or a large file arriving in a shared folder, no longer
  fills the Windows disk to zero: Windows now keeps 2 GB free, as the Mac side already does.
  Files copied in Finder also no longer pile up in Windows' Temp folder; only the current and the
  previous copy are kept, not the last four.
- Choosing a Windows installer ISO under **Import an installed disk** in the
  New VM sheet now says "This is a Windows installer ISO, not a disk with
  Windows installed. Install Windows from it instead." Before, only the first
  import screen refused it; the New VM sheet copied the ISO as a disk.
- Command-N now opens New VM in the library window, bringing that window back
  if you closed it. Before, it opened a second library window. The Window menu
  also has a Library item that brings the library back while a VM window is
  open.
- Starting a VM now empties its command file in the VM's folder
  (`metadata/hvf.ctl`). Before, every command BridgeVM sent to Windows in
  earlier runs, including the text and keys you typed, stayed in that file in a
  readable form and grew with each run; clones, moves and backups carried it
  along.
- Moving or duplicating a VM while BridgeVM is still cleaning up its Windows
  installation now says "BridgeVM is still cleaning up an earlier Windows
  installation for this VM. Choose Refresh, then try to move it again."
  instead of asking you to check "the saved installation transaction". The
  command palette names the Advanced Controls page by that name, not "HVF
  Engine".
- `bridgevm app start`, `stop`, `status`, `install`, `readiness` and `list` now
  say in plain sentences what happened and what to do, instead of printing
  internal codes, run tokens, process IDs and engineering notes. A failure code
  the command doesn't know yet still prints a sentence. `--json` output and exit
  codes are unchanged.
- The Delete dialog no longer tells you to shut the VM down first. BridgeVM
  already refuses to delete a running VM and tells you why.
- If BridgeVM can't archive a deleted VM's vTPM key, the warning now says to
  give a new VM a different name, instead of asking you to choose a VM ID that
  BridgeVM never lets you choose.
- When a running VM leaves the library, the page that keeps its controls now
  lists guest agent events as readable lines instead of internal program
  output.
- A Windows microphone that stops getting sound from the Mac in the middle of a
  call, for example after macOS restarts its audio service, now comes back by
  itself within a few seconds. Before, Windows recorded silence until the call
  app reopened the microphone.
- When Windows Setup fails during installation, BridgeVM now says "Windows Setup
  didn't finish. Choose Try Again." instead of showing the path to a log file
  that Try Again deletes. The log's location is still in the installation log.
- When Create VM fails for a reason you can fix, it now says so: the Windows
  ISO can't be read (its drive was ejected), the VM's folder can't be made in
  the chosen location, there isn't enough free space on another drive for the
  ISO copy, or another VM took the same ID at the same time. Before, each of
  these said only "Couldn't create the VM or save it to the library."
- When a Mac port for port forwarding is already in use, the notice now says to
  restart Windows after quitting the other app or choosing another port. The
  forward is set up only when Windows starts, so quitting the other app alone
  never brought it back.
- An import cut off by Force Quit, a crash or a power loss no longer blocks the
  next import of the same name. The half-made folder kept the name, so BridgeVM
  said "That name is already in your library" while the library was empty. The
  new import now gets the next free ID (such as `windows-11-2`), the old folder's
  files are left alone, and the library explains that a folder with no vm.json
  is usually left from a setup that did not finish and can be moved to the Trash.
- When macOS blocks BridgeVM from the local network, Windows can't reach the Mac's DNS
  server (usually the router) and no website opens. BridgeVM now says so in the VM's view and
  where to turn Local Network access back on, and clears the notice once DNS answers again.
- After an import fails, Virtual machines in the sidebar goes back to Get
  Started again and clears the error. Before, the failed import form stayed with
  no way back, and a later import form still showed the old red error.
- Delete, Duplicate and Move work right after Windows shuts down when a setting
  changed while it ran, such as "Start when BridgeVM opens" or the Command key.
  Before, saving that setting at shutdown could leave them saying the VM's
  registration changed until you chose Reload Library from Command-K.
- A Windows app that stops answering while it holds copied text no longer
  freezes BridgeVM's helper in Windows. Reading the clipboard text now gives up
  after 20 seconds and keeps the last text, so the shared clipboard, shared
  folders and window resizing keep working; before, they all stopped until
  Windows restarted.
- "Open Windows links on Mac" and "Print to Mac" can now be turned off (or on)
  while Windows runs: links stop opening and the printer goes at once, and the
  choice is saved when the VM stops.
- A DNS lookup Windows retries over TCP, as it does when an answer is too large for UDP,
  now reaches the Mac's DNS server. Before, the NAT dropped it without a reply and the lookup
  hung until Windows gave up.
- Importing a disk that a running VM is using now says "Shut down the VM that
  uses this disk, then import again." Before, it said only that the Windows
  files could not be imported and to check access and free space.
- The first-run import form now checks the name and files the way Create VM
  does, in plain sentences without file paths. A name too long for the
  library's folder name is refused at once instead of failing later with a
  general import error.
- Choosing a Windows installer ISO as the disk in the first-run import form now
  says "This is a Windows installer ISO. Use Install Windows instead." Before,
  the ISO was copied as the VM's disk and the import ended with "This disk has
  no UEFI boot partition."
- Putting the Mac to sleep while a key press or click was still on its way to
  Windows no longer stops the keyboard and mouse on wake until Windows restarts:
  the time Windows has to confirm input now counts only while the Mac is awake.
- VoiceOver now reads the Windows display in plain English, such as "Waiting for
  Windows to show a picture" or "1280 by 800". Before, it read internal test
  text such as "no-frame" and a click count after each click.
- After you choose Stop, the VM window now shows what Windows is doing while it
  shuts down, with Force Stop and, if Windows is still running after the wait,
  Shut Down Again. Before, only the library showed them, so a full-screen VM
  window gave no sign of the stop and no way out.
- A snapshot or export run from the command line no longer makes the stopped VM
  show as running in the app, and Start no longer "attaches" to it: Start,
  Delete, Duplicate and Move say another BridgeVM task is using the disk. A stop
  can no longer end that task in the middle of a copy.
- The microphone notice now follows what the running VM started with: after
  you allow the microphone in System Settings, it asks you to restart the VM
  (Windows still has none until then), and it no longer says Windows has no
  microphone when the run has one.
- Windows sound no longer goes silent when Windows or an app picks a playback
  rate other than 48 kHz (for example "16 bit, 44100 Hz" in Sound > Advanced).
  The VM now offers only the 48 kHz format the Mac side plays.
- After restoring a snapshot taken before a disk grow, the disk size setting no
  longer says "Expanded the disk to N GiB" about the disk that was replaced.
- If the drive with a running VM's disk is unplugged or ejected, the app now says
  so and shows Needs attention, with what to do, instead of staying silent while
  Windows gets disk errors.
- Download Drivers in the Create window now stops when you close the window,
  instead of downloading on for up to 30 minutes, and a download cut short by
  quitting is removed the next time you download. The download follows the
  Mac's proxy settings, shows its percent, and a failure now says whether the
  download couldn't be reached or didn't complete, instead of showing a curl
  error number.
- Choosing "Shut down Windows" when you close a VM window now keeps the window
  open while Windows shuts down, so you can still save work if an app asks, and
  the window then says Windows has shut down. A remembered "Shut down Windows"
  no longer stops you closing the window of a VM that is still starting or was
  started elsewhere; you are asked instead. Closing the window of a paused or
  already stopping VM now just closes it.
- A Windows driver touching a device from the very top of the address space can
  no longer crash the VM: the instruction pointer now wraps around as it does on
  real hardware.
- When Windows is still running after a Stop's waiting time (for example an app
  held the shutdown and you chose Cancel in Windows), the VM now offers Shut Down
  Again, which asks Windows once more, instead of only Force Stop.
- Stop now shuts Windows down after you signed out of Windows. Signing out ends
  Guest Tools, so Stop kept saying "Windows is closing apps" until Force Stop;
  it now also presses the VM's power button, and so does Shut Down Again.
- **Retina resolution** and **Start when BridgeVM opens** can now be changed
  while Windows runs, like the Command key setting. Retina applies to the window
  at once; both are saved when the VM stops. They were locked until shutdown.
- When Windows stops by itself because something failed (the VM process
  crashed or was ended, or the disk was still in use), the app says so and shows
  Needs attention, instead of "Windows has shut down."
- While a snapshot is being made, restored or deleted, Delete, Duplicate and Move
  say so, instead of "The VM is running" about a stopped VM. A disk being
  expanded or a security chip change says so too.
- Settings no longer offers up to 32 GiB of memory on any Mac: like Create, it
  stops 4 GiB short of the Mac's memory; a VM saved with more keeps it. **Keep
  Mac awake** and **When the VM window closes** now say they apply to all VMs.
- When keyboard and mouse stop because Windows didn't confirm input in time, the
  VM window now says so and how to get them back. Keys only beeped and clicks
  did nothing.
- After BridgeVM quits unexpectedly while Windows runs, reopening it shows that
  VM as running while Windows shuts down, instead of Stopped, and Start no longer
  launches a second run against the disk it still holds.
- Snapshots: restoring or deleting a snapshot that was deleted elsewhere since
  the list was shown now says so, instead of asking you to check the snapshots
  folder; a snapshot missing its disk or UEFI file now reads as damaged instead
  of offering a retry; and a Quit that waits for a snapshot no longer gets
  cancelled when only the snapshot's name could not be saved.
- A Command key, Retina or "Start when BridgeVM opens" choice made while Windows
  runs that can't be saved when it stops is now shown on the VM's dashboard, and
  a choice made later no longer gets overwritten by it at the next stop.
- The library dashboard now says when a run needs attention: a Windows disk
  BridgeVM couldn't finish saving at shutdown, or lost contact with Windows,
  used to show only in Advanced (the VM read "Stopped" or "Running"), and a
  failed stop read "Booting Windows". Open Display also stays available while
  Windows shuts down, since that window is where an app asking to save shows.
- Pausing Windows now lets go of the Mac microphone (the orange indicator turns
  off) instead of keeping it open for the whole pause; recording continues on
  resume.
- Shared folders: the "skipped because the per-file limit is …" notice now
  names the limit that applied (64 MiB until Guest Tools are updated, not
  always 16 GiB), gives advice that fits a Windows file, and goes away once the
  file syncs after the update.
- Starting a VM with **Use microphone** on now lets macOS ask for microphone
  access first if it has not asked yet (a reset permission, a re-signed build,
  another Mac). Before, Windows silently started without a microphone.
- "Start when BridgeVM opens" no longer skips a VM that is not ready (for
  example its disk is on a drive that is not connected) without a word: it
  starts it as the Start button does, which says why it could not.
- Settings now saves a shared folder whose Mac folder doesn't exist yet, such as
  a new VM's default folder before its first start or a folder on an unplugged
  drive. Turning on "Share a folder" no longer warns before you choose a folder,
  and a VM name typed in Settings is no longer lost if the sheet closes without Done.
- Shared folders: after you changed only the Windows folder of a share (the Mac
  folder stayed the same), the first sync could take same-named files in the new
  Windows folder for edits and copy them over the Mac's files. The saved sync
  state now belongs to both folders, so a new Windows folder starts fresh.
- A Windows driver folder you have opened in Finder no longer fails at Start
  Installation with "can't be used": Finder's `.DS_Store` file is not a driver
  file and is left out.
- When the Mac's audio output or microphone doesn't start for a VM, the VM's
  dashboard now says so in one sentence; before, Windows just had no sound or
  recorded silence with no explanation.
- Typing into a VM window that just opened, for example at the Windows sign-in
  screen, now reaches Windows. Before, keys went nowhere until you clicked
  inside the window.
- Copying more than about 24 MB of text in Windows stalled clipboard sharing,
  shared folders and input for the rest of the session. The copy is now
  refused like any text over 8 MB, and the rest keeps working.
- Create VM with a Windows ISO on another drive no longer stops copying it
  after 30 seconds and starts the copy again from the beginning.
- Importing a VM with its vTPM: a recovery code file other accounts can read, a
  mistyped code or a package for another vTPM now stops the import at once with
  a sentence that names the problem. It used to copy the whole disk first and
  then blame disk access or free space.
- `bridgevm app create-windows --resolution` was saved but never used: the VM
  started at 1280x800. It now sets the VM's startup resolution, and the default
  is the documented 1440x900.
- Installing Windows now keeps the Mac awake, as a running VM does (the "Keep
  Mac awake" setting). An unattended install used to stop when the Mac went to
  sleep, and looked stuck when you came back.
- Shared folders: while a file in the Mac folder kept changing (a log being
  written, a long Finder copy), files saved in the Windows folder never reached
  the Mac. A Windows listing now goes between the Mac's changes at least every
  12 seconds.
- Create VM (and `bridgevm app create-windows`) now looks inside the Windows
  ISO first. An x64 ISO, or a file that isn't a disc image, is refused there
  with a sentence to choose the Windows 11 for Arm ISO from Microsoft. It used
  to be copied into a new VM, and only Start Installation said to delete it.
- Installing Windows on a Mac whose disk fills now stops at once and says the
  disk is full. It sat on "Installing Windows" for up to 25 minutes, then
  failed with a general sentence.
- A duplicated Windows VM attached the Guest Tools disk again at its first start,
  and while its agent was still starting it could say Guest Tools were missing.
  The copy now keeps the record that Windows already has the guest agent.
- Print to Mac and Windows links: a "couldn't create the printer" or "couldn't
  set up links" notice no longer stays until you quit BridgeVM. It goes when
  Windows creates the printer or the VM starts again, and the printer notice
  now includes Windows' reason (for example a stopped Print Spooler) instead of
  always blaming Microsoft Print to PDF.
- Files and folders copied in Windows and pasted in Finder now carry the macOS
  quarantine mark, as files from the shared folder already did, so Gatekeeper
  checks a disk image, archive or app before it first opens. They are readable
  only by you, like shared-folder files. A copy that cannot be marked stays in
  Windows instead of reaching the Mac clipboard.
- Importing a Windows disk smaller than 64 GiB enlarged the disk to 64 GiB, but
  Windows' C: drive never grew into the added space. The next Windows start now
  extends C:, also for VMs imported with an earlier version.
- Clipboard: copying more than 8 MB of text in Windows now shows the "too large"
  notice. It was cleared again by the same poll, so the Mac silently kept its
  old clipboard.
- The VM dashboard said "Graphics mode unknown" for every Windows that BridgeVM
  itself had started. It now shows the running session's display mode.
- Canceling a Windows install, or quitting BridgeVM, while it prepares the setup
  files now stops the file copy too. It kept running and could leave a large
  scratch disk attached until the Mac restarted.
- Shared folder: a Mac file Windows couldn't take (open in a Windows app, or
  Windows out of space) is now copied again on its own a few seconds later, then
  less often, until it gets through. It stayed out of sync both ways until the
  Mac file changed, and Windows edits to it never reached the Mac.
- A failed Guest Tools update now says to quit and reopen BridgeVM to try
  again. "Restart BridgeVM" read like the Restart button beside it, which
  restarts Windows and does not retry the update.
- `bridgevm app install-status` now reports a finished install as done. Once
  Windows had installed, it failed with "unsupportedTarget".
- When Windows could not register BridgeVM as a browser or watch for links,
  "Open Windows links on Mac" stayed on and links silently never reached the
  Mac. The VM window now says so and suggests restarting Windows.
- Keyboard: on German, Swiss, Nordic and other Mac layouts that type @, [ ], { },
  | or ~ with Option, those characters now reach Windows. They were sent as
  Alt+L, Alt+5 and so on, so an email address could only be pasted.
- `bridgevm app create-windows` without `--memory-mib` now gets the memory the
  app recommends for this Mac (4 GiB on an 8 GiB Mac). It always sent 6 GiB.
- Installing from an x64 Windows ISO (not Windows for Arm) now stops as soon as
  the ISO is opened, with a sentence that says so and to create a new VM with
  the Arm64 ISO. It used to copy for minutes, then say only that the setup
  files couldn't be prepared, and Try Again repeated the same wait.
- Restoring a snapshot now gives back the space of the disk it replaces. The
  old disk stayed on the Mac, so a restored VM kept using about twice its disk
  space, even with every snapshot deleted.
- An interrupted snapshot create or export (for example, its process was
  killed) left a hidden, half-made copy of the VM's disk and vars in the
  snapshot folder, plus an empty folder for an app snapshot. The snapshot list
  never showed them and nothing removed them. On APFS each one held on to the
  disk's old blocks as the VM changed; on other volumes each was a full copy.
  The next create or export into the same folder now removes them, and keeps
  anything it does not recognise.
- Moving a VM's bundle into a folder that already holds another VM's bundle, or
  into the folder it is already in, now says so before anything starts, instead
  of "BridgeVM could not confirm the move".
- Display settings: the resolution menu's first entry is now "Automatic", not
  "Automatic (1280x800)", and its help says Automatic keeps the resolution
  Windows last started with. Going back to it after choosing 1920x1080 kept
  1920x1080.
- Canceling a snapshot export now removes the partial copy. It stayed in a
  hidden folder next to the destination, up to the size of the whole disk.
- After 256 starts or stops from the CLI in one app session, every further
  one was refused until the app was reopened.
- Memory and CPU limits now agree across the app. Importing a VM offered up to
  64 GiB and 16 CPUs, and Create offered every core but one, but Settings
  accepted only 32 GiB and 10 CPUs, so changing such a VM's resources later was
  refused. Import now offers Create's choices, and Settings accepts up to every
  core but one.
- A Stop could end as "not confirmed", needing Force Stop, when the app and
  the runner disagreed on a harmless detail of a status message. Both sides
  now ignore fields they do not know.
- Port forwarding: when another app already uses a forward's Mac port, the VM
  now says so ("Mac port 8080 is already in use, so it doesn't reach Windows
  port 80 …") instead of running without that forward in silence. Two rules for
  the same Mac port are refused when you enter them.
- A never-started VM in a folder that reports itself group- or
  world-writable, as exFAT and some shared folders do, can be deleted. The
  check refused it without protecting anything.
- Quitting BridgeVM after a VM's runner stopped unexpectedly no longer waits
  about two minutes behind "Shut down Windows and quit BridgeVM?", "Windows is
  still shutting down." and a Force Stop button that did nothing.
- Display driver install: the result now reads as plain sentences ("Installed
  the display driver.", "Windows didn't answer, so try again when Windows is
  idle.") instead of device status tokens and protocol names such as
  "status=OK" or "BVDRIVER result (exit -1)".
- The guest agent's input capability line is accepted when it carries extra
  keywords from a newer agent.
- Settings: the Command key choice (Mac shortcuts or Windows key) can now be
  changed while Windows runs. It applies at once and is saved when the VM
  stops; it was locked until you shut Windows down, though it only changes how
  BridgeVM maps keys.
- After an unclean stop, the shared folder keeps the Windows version of a file
  beside the Mac one as "(Windows copy)". If the VM stopped again before the
  Mac version was sent back, the next start made a second, identical
  "(Windows copy 2)"; now it keeps one copy and sends the Mac version again
  (#675).
- `create-windows` on an 8 GiB Mac now gives the VM 4 GiB by default, as the
  app's Create sheet does, instead of 6 GiB that left macOS 2 GiB.
- Expanding the disk of an imported VM whose C: drive is followed by another
  partition (usually Windows Recovery) now says that partition keeps C: from
  growing, instead of claiming C: already uses all available space.
- A VM with a Korean name finds its vTPM key even when its ID reaches the
  keychain in the other Unicode form (composed or decomposed, for example
  after the library was copied). Before, the VM refused to start with a
  missing-key error (#689).
- Creating a VM with your own driver folder now says what is wrong with it
  (for example a manifest inside the folder, or a file that isn't a BridgeVM
  driver manifest) before the ISO is copied. It said only "Couldn't create the
  VM", or created the VM and then failed at setup, blaming the ISO.
- Ctrl-C or SIGTERM during `bridgevm app snapshot-create`, `snapshot-restore`
  or `snapshot-export` left the snapshot helper running, so a restore could
  still change the VM after the command had ended. The command now stops the
  helper, waits for it, reports `cancelled` and exits 130 (143 for SIGTERM).
- The library now shows "Moving…", "Duplicating…" or "Deleting…" on a VM while
  that runs, and its Start button waits. A large copy read "Stopped" for
  minutes, and Start was refused only when pressed.
- Importing a VM: the file chooser now opens other VM apps' bundles, so you can
  pick the disk and UEFI variables file inside one. It showed such a bundle as
  a single file that could not be opened.
- Moving a VM's bundle to a folder BridgeVM can't write to no longer makes the
  VM disappear from the library. Nothing had moved, but a leftover "move in
  progress" record hid the VM, and nothing in the app could bring it back.
- Long typed or pasted text in a Windows app built on a RichEdit 5 control
  (for example a WinForms RichTextBox) went in one character at a time,
  about 5 ms each, and from roughly 2,000 characters failed partway with an
  input error. It now goes in as one undoable edit, as in Notepad: 5,000
  mixed characters in under half a second (#665). Needs the updated guest
  agent.
- Duplicate: a copy no longer inherits "Start when BridgeVM opens" (both VMs
  booted at every launch), and a failed duplicate says what is missing or that
  the Mac needs free space, instead of pointing at a destination bundle that had
  already been removed.
- Create VM now says what it still needs when Create is greyed out: the
  Windows ISO, then drivers (Download Drivers, or your folder and manifest),
  then a name. Before, it said nothing until the drivers were chosen.

### Removed

- The legacy Rust CLI commands (`bridgevm --store …`, `--socket`, the daemon,
  disk, snapshot-store and QEMU/VZ commands) and their crates. `bridgevm app …`
  is the CLI for the app's library; `bridgevm hvf` keeps three Graphics Lab
  report tools.

## [0.1.0] - 2026-11-23

Version numbers restart at 0.1.0. This release is built from the same source as
the withdrawn `v1.1.1` tag (`3b01ef93`); its changes are the ones listed under
1.1.1, 1.1.0 and 1.0.0 below. The preview tags `v1.0.0`, `v1.1.0` and `v1.1.1`
and their GitHub releases were withdrawn. An installed 1.1.1 app is replaced by
running the installer again; VMs and settings are unaffected.

## 1.1.1 (withdrawn tag, now 0.1.0) - 2026-11-21

Fixes and interface polish for the 1.1.0 General Preview; the product state
and the release contract are unchanged.

### Fixed

- `bridgevm app install` and `start` find a VM that `bridgevm app create-windows`
  created while the app was already open, instead of answering
  `targetUnavailable` until the app is reopened (#684).
- Windows no longer shows a warning "Red Hat VirtIO SCSI controller" in Device
  Manager: the unused installer slot is left out when no ISO is attached (#685).
- A Mac file put into the shared folder while Windows still ran an older guest
  agent is no longer copied back over itself at the next start (new date,
  quarantine mark): an identical Windows copy is recognised and left alone (#651).

### Changed

- The remaining English labels in the 가상 머신 menu (Windows 창 목록…) and the
  고급 view are in Korean; the developer diagnostics say what they are for.
- Memory and disk sizes say GiB everywhere, as the create sheet already did.
- The snapshot name field looks like a field, and 고급 has one entry point (the
  action grid) instead of three.

## 1.1.0 (withdrawn tag) - 2026-11-20

First General Preview release, replacing the superseded 1.0.0. The product is
still an **Engineering Preview**: Windows 11 Arm on BridgeVM's own
Hypervisor.framework VMM, 3D-off, in an ad-hoc-signed app without Developer ID
or notarization (`docs/install.md` has the first-open step). ISO installation
needs your own Windows 11 Arm ISO; the app downloads the signed ARM64
storage/serial/network drivers from the official virtio-win source on your
click, or you choose your own payload. BridgeVM has only been run on M4 and M5 Macs; M1, M2 and M3
are untested. The 1.0.0 claim that every release-blocking criterion was proven
no longer stands: 2 of 5 release scenarios are met, and A9 (in-app install on
a clean Mac) and A19 (disk safety) are open. See `STATUS.md`.

### Added

- **Install from an ISO alone:** "드라이버 자동 준비" in the create sheet, or
  `bridgevm app create-windows --fetch-drivers`, downloads the pinned
  Microsoft-signed Red Hat virtio-win drivers (about 95 MB) once, checks their
  SHA-256 and seals them in the VM; your own payload still works.
- **Import:** QCOW2 and Hyper-V VHDX disks are converted to sparse RAW,
  the UEFI vars file is optional, and an imported Windows that has the virtio
  serial driver but no guest tools is offered a guest tools installer.
- **Display:** One-click install of the Microsoft-signed virtio-win display
  driver; after a VM restart Windows follows the window and shows its pointer
  as the Mac cursor. Per-VM resolution (1280x800 default, up to 2560x1600), an
  experimental Retina option (off by default); the window reopens where it was.
- **Input:** Command acts as Ctrl (or, per VM, as the Windows key) and Option
  as Alt; held keys, modifier-clicks and F13 as Print Screen reach Windows.
- **Korean input:** Ctrl shortcuts work with the Korean input source, a syllable
  is committed before a shortcut or click, and candidates follow the caret.
- **Audio:** An opt-in per-VM microphone that uses the Mac's default input.
- **Clipboard:** Images, rich text, and files or folders (up to 4096 entries
  and 16 GiB) copy both ways between Finder and Explorer.
- **Shared folder:** New VMs share `~/BridgeVM 공유/<VM name>` as
  `C:\bridgevm-share` by default; existing VMs are unchanged. Files dropped on
  the VM window or opened with BridgeVM from Finder are copied into Windows.
- **Snapshots:** Several snapshots per stopped VM, to name, restore or delete.
- **Network:** Per-VM inbound TCP port forwards, on the Mac's 127.0.0.1 only.
- **Storage:** Grow a stopped VM's disk; deleting in Windows frees Mac space.
- **App:** A per-VM Settings sheet, a 가상 머신 menu (Pause/Resume, Restart,
  Windows apps), printing to Mac printers (on by default), opt-in Windows links
  in the Mac browser, Mac battery in Windows, stay-awake and opt-in auto-start.
- **Guest tools:** Updated at VM start; a crashed agent is back within a minute.
- **CLI:** `bridgevm app` lists, installs, starts, stops and snapshots VMs.

### Changed

- **Storage/flush:** Flushes are ordered and durable within about a second, so
  a Mac power loss can drop the last second of writes but never leaves the disk
  inconsistent; per-VM "안전한 쓰기" makes every flush wait for a full sync.
- **Stop and quit:** Stop, Quit and an app crash shut Windows down, by the ACPI
  power button if no agent runs; closing the last window no longer quits.
- **Boot and clock:** Boots skip a 5-second firmware countdown; Windows takes
  the Mac's time and time zone at every boot and its time after the Mac sleeps.
- **Network:** Guest DNS follows the Mac's resolver after a network change.
- **Shared folder:** Large Mac files reach Windows about six times faster, files
  keep their dates, and sync handles 16 GiB files and 262,144-entry shares.
- **Messages:** Start, install, import and snapshot failures read as one plain
  Korean sentence; a missing shared folder only turns sharing off for a start.
- **Dashboard:** Actions stay readable at any window width, the status shows
  the VM as observed, and the VM window shows startup progress or the failure
  instead of staying black.
- **First run:** The empty library shows one set of create and import choices
  in plain words; the sidebar no longer shows a fixed engine list.
- **Security:** The installer checks a General Preview release contract,
  checksum, archive and signature, rolls back on failure and refuses v1.0.0;
  new Windows installs let only administrators change `C:\BridgeVM`.

### Fixed

- **Audio:** Every other 10 ms of Windows playback reached the Mac as silence,
  and output did not follow the Mac's default device.
- **Network:** Large downloads stalled or timed out in the built-in NAT, and the
  last bytes of an upload could be lost.
- **Storage:** A full Mac disk failed Windows' writes; the VM now pauses so you
  can free space and resume, and the app warns if writes were not made safe.
- **Shared folder:** Windows edits were overwritten after a guest restart, just
  before a shutdown or in folders over the listing limit; a Force Stop or crash
  could leave uploads as zeros or fetch an empty Windows copy over the Mac's; a
  junction deleted Mac files; Korean names split; 260+ character paths failed.
- **Shared folder:** A Windows edit saved within about 20 seconds of a sync, or
  one whose copy to the Mac failed, was overwritten by the Mac's older copy
  after a Force Stop, a crash or a Windows restart; it is now kept beside the
  Mac's file as "(Windows copy)" (#643, #648).
- **Clipboard:** The "클립보드 공유" switch is saved per VM, so dashboard and
  command-line starts no longer turn sync back on; change it while stopped.
- **Clipboard:** Some Mac copies, including any text over 8 KiB, were lost or
  reverted, and legacy paste text could reach the Mac pasteboard; a copy made
  just before Ctrl+V or Shift+Insert could paste the previous text.
- **Input:** Input stopped after a Windows restart, an agent restart or update,
  or at the lock screen, and trackpads scrolled far too fast; a left click
  could open a context menu; input could stay off when the guest agent
  answered before the app saw it start.
- **Korean input:** Long text committed into Windows 11 Notepad repeated or
  lost characters; compatible editors now receive it in one piece.
- **Display:** A VM started without its window came back from a Windows
  restart at the boot resolution instead of the saved one.
- **Lifecycle:** Windows was powered off without a shutdown after 50 million
  hypervisor exits on one vCPU (about a day idle or 4-8 hours busy, estimated
  from exit rates), and a quote in a disk path crashed the app at Start; on
  M5 Macs some boots stopped in the firmware before Windows started.
- **vTPM:** A failed TPM reset could destroy the VM's TPM state, and a deleted
  VM's TPM key could be picked up by a new VM with the same name.
- **Security:** Guest-controlled values could crash device models, exhaust host
  memory or reach paths outside the shared folder.
- **Graphics Lab only:** Legacy TGSI shaders now infer integer buffer sampler
  types, restoring classic Notepad captions in the tested scene (B6 open).

### Removed

- **3D acceleration** left General Preview and the v1 scope on 2026-09-16: VMs
  start 3D-off, the app never injects a test-signed driver or enables
  TESTSIGNING, and Vulkan and D3D11 stay in the separate Graphics Lab.
- The "Guest windows" button over Windows' close button (now in the menu).

### Known issues

- In-app install and import are unproven on a clean Mac, the window may be
  choppy and sound may stutter (A9), so keep your ISO or source disk handy.
- Windows keeps the resolution set before start until you install the display
  driver from the VM window and restart (S2), so pick it in Settings first.
- Disk safety is open (A19): interrupted operations are only partly tested,
  power loss during a snapshot not at all, and VMs relocated by older builds
  before identity migration are refused; snapshot on mains power, keep backups.
- Titles, tabs and menus can be blank on the Graphics Lab 3D path (B6).
- After a Force Stop, a crash or a Windows restart within about 20 seconds of a
  shared file's last sync, a differing Windows copy arrives beside the Mac's as
  "(Windows copy)", even when it is only damage from the stop; keep the one you
  want. A second stop during that recovery can add a "(Windows copy 2)" (#675).
- Long text into editors other than Windows 11 Notepad and compatible controls
  can still repeat or lose characters (#665); paste through the clipboard.
- Files added to the share while an older guest agent updates come back once,
  unchanged but with a new date and quarantine flag (#651); wait a minute first.

## 1.0.0 (withdrawn tag) - 2026-08-20

Every release-blocking criterion in `capabilities/windows-hvf.json` is proven
by live evidence, including the final Windows closure gate
(driver-loadable image, host-driven resolution adoption, and the shipped
window-Coherence verbs answered live; receipt
`fefad903a56fa2f1c63e6067079208928d1dae1f415ff966611629ea37727a97`).

### Added

- A Mac-native Windows 11 Arm engine built directly on
  Hypervisor.framework, alongside the existing Apple Virtualization.framework
  and QEMU compatibility engines.
- Windows HVF support for persistent storage, display and input, networking,
  audio, guest integration, TPM/Secure Boot workflows, snapshots, and
  experimental 3D.
- An experimental Vulkan path and an experimental D3D11-compatible subset
  through virtio-gpu.
- Ad-hoc-signed `.dmg` and curl-installable `.tar.gz` packaging, with a
  documented macOS installation flow for builds distributed without Developer
  ID or notarization.
- A machine-readable Windows HVF capability registry backed by deterministic
  checks and dated live-gate evidence.

### Changed

- Defined the guest platform as a QEMU `virt`-compatible contract with
  documented deviations.
- Separated deterministic hosted checks from serialized, sealed-input live
  gates that require real Apple-silicon hardware or private Windows media.

### Security

- Guest-visible random values use the host CSPRNG and fail closed when entropy
  is unavailable.
- Release builds reject repository and `PATH` overrides for bundled helper
  binaries.
