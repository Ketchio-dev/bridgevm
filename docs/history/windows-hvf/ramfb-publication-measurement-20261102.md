# Measure RAMFB publication at completion (2026-11-02)

Main c41a8390 CI41412661026/job125169016347 failed the original observer
median: 759 tests passed and one failed with 10.249083 ms against 1 ms.
All 21 frames published, but producer completion data was absent, so the
original hosted delay cannot be assigned to observation or exporter wake.
The failed receipt remains a failure; it is not relabelled as success.

A local diagnostic did not reproduce it. A controlled delayed observer then
failed the original median at 11.786 ms while all 20 actual producer
completions were 30.375–74.042 microseconds and existing wake checks passed.
This proves the old observer measurement can reject timely publication;
it does not establish the original hosted scheduling cause.

The fixture now pairs each frame with the existing Instant captured after
publish_changed returns, an upper bound on completed publication. The start
stays before request-file open, and the original busy observer loop stays.
The 20 samples, 1 ms median, late>=11, 21 publications, pixels/sequence and
one-second liveness checks remain. Observer and earlier header-stamp values
remain failure context. The artificial delay was removed.

A test-only accessor reads existing timing state: no production behavior,
clock, hook or timing-record grammar changes. The final request suite passed
4/4; applicable CI1.97 production clippy, formatting and budgets passed.
These tests do not prove live display smoothness or relax a release criterion.
The integrated remediation head still requires every exact project/PR gate
and post-merge main CI before it can be called complete.
