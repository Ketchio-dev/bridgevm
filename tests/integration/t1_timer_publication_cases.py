"""Complete T1 evidence must pass both retained-data and actual CLI routing."""
import shutil
from t1_timer_fixture import ROOT, SHA, json, subprocess, sys


class TimerPublicationCases:
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
        self.assert_actual_cli_routing(private)
        path = self.out / 'receipt.public.json';path.chmod(0o600);public['cancel_interval_us'] = 37;path.write_text(json.dumps(public))
        result = subprocess.run(argv, env=self.env, capture_output=True, text=True, timeout=10)
        self.assertNotEqual(result.returncode, 0)

    def assert_actual_cli_routing(self, private):
        for state in ('running', 'done'):
            with self.subTest(queue_state=state):
                queue = self.root / ('cli-' + state)
                job = queue / state / 'owned-t1'
                shutil.copytree(self.out, job)
                env = dict(self.env, BRIDGEVM_LIVE_ROOT=str(queue))
                argv = ['/bin/bash', str(ROOT / 'scripts/live-gates/bridgevm-live'), 'receipt', job.name]
                for ledger_present in (False, True):
                    with self.subTest(ledger_present=ledger_present):
                        if ledger_present:
                            entry = queue / 'job-ledger' / job.name / 'entry.env'
                            entry.parent.mkdir(parents=True)
                            entry.write_bytes((job / 'job.env').read_bytes())
                            entry.chmod(0o400)
                        result = subprocess.run(argv, env=env, capture_output=True, text=True, timeout=10)
                        self.assertEqual(result.returncode, 0, result.stdout+result.stderr)
                        self.assertEqual(json.loads(result.stdout), private)
                public = dict(private, cancel_interval_us=37)
                path = job / 'receipt.public.json';path.chmod(0o600);path.write_text(json.dumps(public))
                result = subprocess.run(argv, env=env, capture_output=True, text=True, timeout=10)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(result.stdout, '')
