#!/usr/bin/env python3
"""Deterministic destructive-path tests use only disposable synthetic fixtures."""
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

SCRIPTS = Path(__file__).resolve().parents[2] / 'scripts'
sys.path.insert(0, str(SCRIPTS))
import experiment_retention as retention

spec = importlib.util.spec_from_file_location('archive_cli', SCRIPTS / 'archive-experiment.py')
cli = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cli)


class RetentionTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        self.queue = self.root / 'lab/queue'
        for name in ('done', 'queued', 'running'):
            (self.queue / name).mkdir(parents=True)
        self.campaign = 'a' * 32
        self.image, self.vars = self.root / 'source.raw', self.root / 'vars.fd'
        self.image.write_bytes(b'original disk')
        self.vars.write_bytes(b'original vars')
        self.jobs = []
        for ordinal in range(1, 21):
            d = self.queue / 'done' / ('job-' + str(ordinal).zfill(3))
            (d / 'media').mkdir(parents=True)
            (d / 'boot').mkdir()
            (d / 'media/target.raw').write_bytes(b'output ' + bytes([ordinal]))
            (d / 'media/vars.fd').write_bytes(b'output vars')
            (d / 'boot/cleanup.txt').write_text('cleanup_status=0\n')
            (d / 'boot/boot-timer-report.tsv').write_text('desktop\t123\n')
            manifest = ''.join(name + '\t' + str(p) + '\t' + retention.sha(p) + '\n'
                               for name, p in [('image', self.image), ('vars', self.vars)])
            (d / 'input-manifest.tsv').write_text(manifest)
            commit = '799dcf345a05dbf8b272354f987fcec8f2b23bd0'
            (d / 'job.env').write_text('job_id=' + d.name + '\ncommit=' + commit +
                                      '\ntier=t15-hvf-boot-performance\nfinished_at=fixture\n')
            (d / 'result.env').write_text('result=pass\nexit_code=0\n')
            r = dict(campaign_id=self.campaign, job_id=d.name, valid=True, **{'pass': True},
                     tier='t15-hvf-boot-performance', commit=commit,
                     workload_profile='shipping-core-3d-boot-v1', campaign_ordinal=ordinal,
                     campaign_expected_runs=20, input_manifest_sha256=retention.sha(d / 'input-manifest.tsv'),
                     image_sha256=retention.sha(self.image), vars_sha256=retention.sha(self.vars))
            (d / 'receipt.json').write_text(json.dumps(r))
            self.jobs.append(d)

    def plan(self):
        return retention.plan(self.queue, self.campaign)

    def apply(self, data):
        p = self.root / 'plan.json'
        cli.write_new(p, data)
        with patch.object(cli.subprocess, 'run', return_value=subprocess.CompletedProcess([], 1, '', '')):
            return cli.apply_plan(self.queue, p, retention.sha(p), self.root / 'journal.jsonl')

    def test_plan_is_nondestructive_and_apply_preserves_sources_and_evidence(self):
        data = self.plan()
        self.assertEqual(len(data['targets']), 19)
        self.assertTrue(all((d / 'media/target.raw').exists() for d in self.jobs))
        result = self.apply(data)
        self.assertEqual(result['deleted'], 19)
        self.assertEqual(self.image.read_bytes(), b'original disk')
        self.assertTrue((self.jobs[0] / 'media/target.raw').exists())
        self.assertFalse(any((d / 'media/target.raw').exists() for d in self.jobs[1:]))
        self.assertTrue(all((d / 'media/vars.fd').exists() for d in self.jobs))
        self.assertFalse((self.queue / 'worker.lock').exists())
        self.assertEqual(self.plan()['targets'], [])

    def test_modified_plan_cannot_delete_source(self):
        data = self.plan()
        data['targets'][0]['path'] = str(self.image)
        with self.assertRaisesRegex(ValueError, 'stale or modified'):
            self.apply(data)
        self.assertTrue(self.image.exists())

    def test_changed_evidence_stops_all_deletions(self):
        data = self.plan()
        (self.jobs[-1] / 'boot/boot-timer-report.tsv').write_text('changed\n')
        with self.assertRaises(ValueError):
            self.apply(data)
        self.assertTrue(all((d / 'media/target.raw').exists() for d in self.jobs))

    def test_source_change_after_plan_is_rejected(self):
        data = self.plan()
        self.image.write_bytes(b'changed source')
        with self.assertRaises(ValueError):
            self.apply(data)

    def test_active_queue_and_worker_lock_are_protected(self):
        data = self.plan()
        (self.queue / 'queued/new-job').mkdir()
        with self.assertRaises(ValueError):
            self.apply(data)
        (self.queue / 'queued/new-job').rmdir()
        (self.queue / 'worker.lock').mkdir()
        with self.assertRaises(FileExistsError):
            with cli.worker_lock(self.queue):
                self.fail('must not acquire another worker lock')
        self.assertTrue((self.queue / 'worker.lock').exists())

    def test_symlink_output_is_rejected(self):
        p = self.jobs[1] / 'media/target.raw'
        p.unlink()
        p.symlink_to(self.image)
        with self.assertRaises(ValueError):
            self.plan()

    def test_hardlink_output_is_rejected(self):
        import os
        p = self.jobs[1] / 'media/target.raw'
        p.unlink()
        os.link(self.image, p)
        with self.assertRaises(ValueError):
            self.plan()

    def test_cross_campaign_input_dependency_is_rejected(self):
        (self.queue.parent / 'another-manifest.tsv').write_text(
            'image\t' + str(self.jobs[1] / 'media/target.raw') + '\t' + 'a' * 64 + '\n')
        with self.assertRaisesRegex(ValueError, 'referenced'):
            self.plan()

    def test_invalid_or_incomplete_campaign_is_rejected(self):
        (self.jobs[-1] / 'result.env').write_text('result=fail\nexit_code=1\n')
        with self.assertRaises(ValueError):
            self.plan()
        (self.jobs[-1] / 'receipt.json').unlink()
        with self.assertRaisesRegex(ValueError, 'incomplete'):
            self.plan()

    def test_wrong_plan_hash_is_rejected(self):
        p = self.root / 'plan.json'
        cli.write_new(p, self.plan())
        with self.assertRaisesRegex(ValueError, 'hash mismatch'):
            cli.apply_plan(self.queue, p, '0' * 64, self.root / 'journal.jsonl')

    def test_open_output_is_rejected(self):
        p = self.root / 'plan.json'
        cli.write_new(p, self.plan())
        with patch.object(cli.subprocess, 'run', return_value=subprocess.CompletedProcess([], 0, '123\n', '')):
            with self.assertRaisesRegex(ValueError, 'output is open'):
                cli.apply_plan(self.queue, p, retention.sha(p), self.root / 'journal.jsonl')
        self.assertTrue(all((d / 'media/target.raw').exists() for d in self.jobs))

    def test_t16_raw_measurement_hash_is_checked(self):
        for d in self.jobs:
            p = d / 'receipt.json'
            r = json.loads(p.read_text())
            r.update(tier='t16-hvf-nvme-performance',
                     commit='3ffb66b1608b1a69ad0d2dae579c9bb59f1aadba',
                     workload_profile='windows-nvme-warm-seq-v1')
            (d / 'job.env').write_text('job_id=' + d.name + '\ncommit=' + r['commit'] +
                                      '\ntier=' + r['tier'] + '\nfinished_at=fixture\n')
            (d / 'share').mkdir()
            for name, key in [('share/nvme-result.json', 'result_sha256'),
                              ('share/nvme-raw.json', 'raw_sha256'),
                              ('share/nvme-result.done', 'done_sha256'),
                              ('power-source.log', 'power_log_sha256')]:
                (d / name).write_text('fixture\n')
                r[key] = retention.sha(d / name)
            p.write_text(json.dumps(r))
        self.assertEqual(len(self.plan()['targets']), 19)
        (self.jobs[-1] / 'share/nvme-raw.json').write_text('corrupted\n')
        with self.assertRaisesRegex(ValueError, 'hash mismatch'):
            self.plan()

    def test_partial_failure_preserves_journal_and_stops(self):
        data = self.plan()
        original_unlink = Path.unlink
        failing = self.jobs[2] / 'media/target.raw'
        def fail_one(p, *args, **kwargs):
            if p == failing:
                raise OSError('injected unlink failure')
            return original_unlink(p, *args, **kwargs)
        with patch.object(Path, 'unlink', fail_one):
            with self.assertRaisesRegex(OSError, 'injected'):
                self.apply(data)
        records = [json.loads(line) for line in (self.root / 'journal.jsonl').read_text().splitlines()]
        self.assertEqual(sum(r['event'] == 'deleted' for r in records), 1)
        self.assertFalse(any(r['event'] == 'complete' for r in records))
        self.assertTrue(failing.exists())
        self.assertTrue(all((d / 'media/target.raw').exists() for d in self.jobs[2:]))


if __name__ == '__main__':
    unittest.main()
