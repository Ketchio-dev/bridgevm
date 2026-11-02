# Visible notice when the runtime stops sharing (2026-11-02)

The resident share service stops sharing for the rest of a boot generation
in two cases: a guest agent older than the share protocol (`agent-too-old`)
and repeated host write failures (`disabled failed-destination-limit`).
Both printed one raw line that only the advanced feed showed.

The session's share notice now also takes those exact lines and shows,
in the runtime card and dashboard, that this run will not synchronize the
shared folder, with the reason and next step. A stop outranks the selected-
root deferral notice, keeps its first reason, ignores command output and
clears on a new start, an admitted stop and a guest reset record, when the
next runtime generation tries sharing again. The runtime is unchanged.

Evidence is automated tests only; no live guest or operator folder was used,
and no criterion or product state changes.
