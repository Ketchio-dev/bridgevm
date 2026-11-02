# Inactive Windows HVF resolution controls (2026-11-02)

The creation sheet exposed resolution choices for Windows HVF installation
and import, but the launch configuration did not consume the saved width and
height. The dashboard also displayed those stored dimensions as if active.

The two HVF creation modes now explain that the guest screen size is used,
and the HVF dashboard describes the guest screen rather than saved dimensions.
The selection and VMConfig data remain intact; no migration or runtime change
is made. This does not promise guest resolution changes or establish A8.

The resolution row was extracted to stay within existing structural budgets.
This small presentation correction adds no view-source tests or separate
build. The batch's exact final project and app compilation remain required.
