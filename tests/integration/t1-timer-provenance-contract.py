#!/usr/bin/env python3
"""Owned command stubs exercise T1 without launching Hypervisor.framework."""
from t1_timer_fixture import *


class TimerBoundary(TimerFixture):
    def test_resolves_per_job_cargo_target_directory(self):
        self.env['CARGO_TARGET_DIR'] = str(self.root / 'per-job-target')
        result = self.run_gate()
        self.assertEqual(result.returncode, 0, result.stdout+result.stderr)
        self.assertTrue((self.env_path() / 'debug/examples/hvf_vtimer_cancel_probe').is_file())

    def test_queue_receipt_retains_detailed_output_and_signed_binary(self):
        result = self.run_gate(tier=True)
        self.assertEqual(result.returncode, 0, result.stdout+result.stderr)
        value = json.loads((self.out / 'receipt.json').read_text())
        self.assertEqual(value['probe'], 'hvf_vtimer_cancel')
        self.assertEqual(value['iterations'], 10000)
        self.assertEqual(value['cancel_interval_us'], 0)
        self.assertTrue(value['recovery_enabled'])
        self.assertEqual(value['binary_source_commit'], SHA)
        self.assertEqual(value['probe_receipt_sha256'], hashlib.sha256((self.out / 'vtimer-cancel-receipt.json').read_bytes()).hexdigest())
        self.assertFalse(value['criterion_pass'])
        self.assertFalse(value['claim_eligible'])


    def verify(self):
        return subprocess.run([sys.executable, str(self.root / 'scripts/live-gates/t1_probe_receipt.py'), 'verify', str(self.out), str(self.out / 'receipt.json'), SHA, 'owned-t1'], env=self.env, capture_output=True, text=True, timeout=10)

    def good_receipt(self):
        result = self.run_gate(tier=True)
        self.assertEqual(result.returncode, 0, result.stdout+result.stderr)
        result = self.verify()
        self.assertEqual(result.returncode, 0, result.stdout+result.stderr)
        return json.loads((self.out / 'receipt.json').read_text())

    def test_signature_and_boolean_entitlement_refuse_before_probe(self):
        for knob in ['BAD_SIGN', 'BAD_ENTITLEMENT']:
            with self.subTest(knob=knob):
                self.env[knob] = '1'
                result = self.run_gate()
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse((self.out / 'vtimer-cancel-receipt.json').exists())
                self.env.pop(knob)

    def test_actual_binary_hash_is_after_signing(self):
        self.env['SIGN_MARKER'] = '1'
        value = self.good_receipt()
        binary = self.root / 'target/debug/examples/hvf_vtimer_cancel_probe'
        self.assertIn('# signed fixture', binary.read_text())
        self.assertEqual(value['binary_hash'], hashlib.sha256(binary.read_bytes()).hexdigest())
        calls = [json.loads(line) for line in self.record.read_text().splitlines()]
        self.assertIn(['codesign', '--verify', '--strict', str(binary.resolve())], calls)
        self.assertIn(['codesign', '-d', '--entitlements', '-', '--xml', str(binary.resolve())], calls)

    def test_cached_run_requires_matching_previous_build_seal(self):
        self.good_receipt()
        self.out = self.root / 'cached-run'
        result = self.run_gate(extra=['--skip-build'])
        self.assertEqual(result.returncode, 0, result.stdout+result.stderr)
        for p in (self.root / 'target/debug/examples').glob('*.t1-build.json'):p.unlink()
        self.out = self.root / 'unsealed-cache'
        result = self.run_gate(extra=['--skip-build'])
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse((self.out / 'vtimer-cancel-receipt.json').exists())

    def test_failed_counter_verdict_and_actual_return_remain_failed(self):
        self.env['PROBE_FAIL'] = '1'
        result = self.run_gate(tier=True)
        self.assertEqual(result.returncode, 1, result.stdout+result.stderr)
        value = json.loads((self.out / 'receipt.json').read_text())
        self.assertFalse(value['pass'])
        self.assertEqual(value['command_exit_code'], 1)
        self.assertEqual(value['outcome'], 'stalled')
        self.assertEqual(value['timer_wakes'], 9999)
        self.assertEqual(self.verify().returncode, 0)

    def test_receipt_tampering_does_not_pass_on_its_boolean(self):
        value = self.good_receipt()
        for key, replacement in [('trace_overflow', False), ('pass', False), ('commit', '3'*40), ('job_id', 'foreign'), ('cancel_interval_us', 37), ('recovery_enabled', False), ('schema_version', True), ('command_exit_code', 1), ('criterion_pass', True), ('binary_hash', 'bad'), ('source_tree', 'bad')]:
            with self.subTest(key=key):
                changed = dict(value);changed[key] = replacement
                path = self.out / 'receipt.json';path.chmod(0o600);path.write_text(json.dumps(changed))
                self.assertNotEqual(self.verify().returncode, 0)
        path.write_text(json.dumps(value))
        self.assertEqual(self.verify().returncode, 0)

    def test_missing_changed_and_symlinked_evidence_is_refused(self):
        self.good_receipt()
        for filename in ['vtimer-cancel-receipt.json', 't1-probe.log', 't1-build-seal.json', 't1-run-config.json']:
            with self.subTest(filename=filename):
                path = self.out / filename;original = path.read_bytes();path.chmod(0o600)
                path.write_bytes(original+b'changed')
                self.assertNotEqual(self.verify().returncode, 0)
                path.unlink()
                self.assertNotEqual(self.verify().returncode, 0)
                target = self.root / 'foreign-evidence';target.write_bytes(original);path.symlink_to(target)
                self.assertNotEqual(self.verify().returncode, 0)
                path.unlink();path.write_bytes(original)
        self.assertEqual(self.verify().returncode, 0)

    def test_publication_and_reader_bind_retained_private_evidence(self):
        self.good_receipt()
        result = subprocess.run(['/bin/bash', str(self.root / 'scripts/live-gates/publish-receipt.sh'), 't1-vtimer', str(self.out), str(self.root), SHA], env=self.env, capture_output=True, text=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stdout+result.stderr)
        private = json.loads((self.out / 'receipt.json').read_text())
        public = json.loads((self.out / 'receipt.public.json').read_text())
        self.assertEqual(public, private)
        (self.out / 'job.env').write_text(f'job_id=owned-t1\ntier=t1-vtimer\ncommit={SHA}\n')
        argv = [sys.executable, str(ROOT / 'scripts/live-gates/bridgevm_live_receipt_legacy.py'), str(self.out), 'owned-t1']
        result = subprocess.run(argv, env=self.env, capture_output=True, text=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stdout+result.stderr)
        self.assertEqual(json.loads(result.stdout), private)
        path = self.out / 'receipt.public.json';path.chmod(0o600);public['cancel_interval_us'] = 37;path.write_text(json.dumps(public))
        result = subprocess.run(argv, env=self.env, capture_output=True, text=True, timeout=10)
        self.assertNotEqual(result.returncode, 0)

    def test_nondefault_experiment_is_diagnostic_not_default_gate_evidence(self):
        result = self.run_gate(tier=False, extra=['--job-id', 'owned-t1', '--iterations', '2000', '--cancel-interval-us', '9'])
        self.assertEqual(result.returncode, 0, result.stdout+result.stderr)
        value = json.loads((self.out / 'receipt.json').read_text())
        self.assertEqual(value['iterations'], 2000)
        self.assertEqual(value['cancel_interval_us'], 9)
        result = self.verify()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('unchanged default gate configuration', result.stderr)


    def test_dirty_source_refuses_before_any_build_or_probe(self):
        self.env['DIRTY_SOURCE'] = '1'
        result = self.run_gate()
        self.assertNotEqual(result.returncode, 0)
        calls = [json.loads(line) for line in self.record.read_text().splitlines()]
        self.assertFalse(any(call[0] == 'cargo' for call in calls))
        self.assertFalse((self.out / 'vtimer-cancel-receipt.json').exists())

    def test_binary_change_after_known_probe_return_withholds_receipt(self):
        self.env['CHANGE_BINARY'] = '1'
        result = self.run_gate(tier=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertTrue((self.out / 'vtimer-cancel-receipt.json').exists())
        self.assertFalse((self.out / 'receipt.json').exists())


    def test_actual_cross_target_output_is_selected_over_stale_default(self):
        stale = self.root / 'target/debug/examples/hvf_vtimer_cancel_probe'
        stale.parent.mkdir(parents=True);stale.write_text('#!/bin/sh\nexit 97\n');stale.chmod(0o700)
        self.env['CARGO_BUILD_TARGET'] = 'owned-native-target'
        value = self.good_receipt()
        actual = self.root / 'target/owned-native-target/debug/examples/hvf_vtimer_cancel_probe'
        self.assertEqual(value['binary_hash'], hashlib.sha256(actual.read_bytes()).hexdigest())
        self.assertEqual(stale.read_text(), '#!/bin/sh\nexit 97\n')

    def test_foreign_job_tier_cannot_route_through_copied_t1_receipt(self):
        value = self.good_receipt()
        (self.out / 'receipt.public.json').write_text(json.dumps(value))
        argv = [sys.executable, str(ROOT / 'scripts/live-gates/bridgevm_live_receipt_legacy.py'), str(self.out), 'owned-t1']
        for tier in ['t20-a19-native-snapshot-restore', 't23-a19-lifecycle-campaign', 'd10-t22-owned-pair-preparation', 'd11-native-fixture-preparation', 'foreign-tier']:
            with self.subTest(tier=tier):
                (self.out / 'job.env').write_text(f'job_id=owned-t1\ntier={tier}\ncommit={SHA}\n')
                result = subprocess.run(argv, env=self.env, capture_output=True, text=True, timeout=10)
                self.assertNotEqual(result.returncode, 0, result.stdout+result.stderr)


    def test_missing_or_ambiguous_actual_build_output_refuses_before_sign_and_probe(self):
        for knob in ['NO_ARTIFACT', 'DOUBLE_ARTIFACT']:
            with self.subTest(knob=knob):
                self.env[knob] = '1'
                result = self.run_gate()
                self.assertNotEqual(result.returncode, 0)
                self.assertIn('missing or ambiguous', result.stderr)
                calls = [json.loads(line) for line in self.record.read_text().splitlines()]
                self.assertFalse(any(call[0] == 'codesign' for call in calls))
                self.assertFalse((self.out / 'vtimer-cancel-receipt.json').exists())
                self.env.pop(knob)


if __name__ == '__main__':
    if '--self-test' in sys.argv:sys.argv.remove('--self-test')
    unittest.main()
