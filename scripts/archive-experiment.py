#!/usr/bin/env python3
"""Plan, then explicitly apply archival of reviewed completed output clones."""
import argparse
from contextlib import contextmanager
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

import experiment_retention as retention


def write_new(path, value):
    with path.open('x', encoding='utf-8') as f:
        os.chmod(path, 0o600)
        json.dump(value, f, indent=2)
        f.write('\n')
        f.flush()
        os.fsync(f.fileno())


@contextmanager
def worker_lock(queue):
    lock = queue / 'worker.lock'
    lock.mkdir(mode=0o700)  # Never steal even a stale worker lock.
    try:
        (lock / 'pid').write_text(str(os.getpid()) + '\n')
        yield
    finally:
        (lock / 'pid').unlink()
        lock.rmdir()


def apply_plan(queue, plan_path, expected_hash, journal_path):
    retention.require(len(expected_hash) == 64 and retention.sha(plan_path) == expected_hash,
                      'plan hash mismatch')
    original = json.loads(plan_path.read_text())
    retention.require(original.get('queue') == str(queue), 'queue differs from the plan')
    with worker_lock(queue):
        current = retention.plan(queue, original['campaign'])
        retention.require(current == original, 'plan is stale or modified; prepare a new plan')
        paths = [x['path'] for x in current['targets']]
        if paths:
            opened = subprocess.run(['lsof', '-t', '--', *paths], capture_output=True, text=True)
            retention.require(opened.returncode == 1 and not opened.stdout.strip() and not opened.stderr.strip(),
                              'output is open or open-file check failed')
        before = os.statvfs(queue).f_bavail * os.statvfs(queue).f_frsize
        with journal_path.open('x', encoding='utf-8') as journal:
            os.chmod(journal_path, 0o600)
            def event(value):
                journal.write(json.dumps(value) + '\n')
                journal.flush()
                os.fsync(journal.fileno())
            event(dict(event='start', plan_sha256=expected_hash, free_bytes=before))
            for target in current['targets']:
                retention.idle(queue)
                p = Path(target['path'])
                retention.require(retention.metadata(p) == target['identity'], 'output changed')
                event(dict(event='intent', **target))
                p.unlink()
                event(dict(event='deleted', path=str(p)))
            for name, digest in current['preserved'].items():
                retention.require(retention.sha(Path(name)) == digest, 'preserved evidence changed: ' + name)
            # Regenerate to verify input/representative/receipt survival, too.
            final = retention.plan(queue, original['campaign'])
            retention.require(not final['targets'], 'unexpected output remains')
            after = os.statvfs(queue).f_bavail * os.statvfs(queue).f_frsize
            result = dict(event='complete', deleted=len(paths), preserved=len(current['preserved']),
                          free_bytes=after, free_bytes_change=after-before)
            event(result)
            return result


def main():
    if sys.argv[1:] == ['--self-test']:
        return subprocess.call([sys.executable, str(Path(__file__).resolve().parent.parent /
                               'tests/integration/experiment-retention-test.py')])
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--queue-root', type=Path, required=True)
    sub = parser.add_subparsers(dest='action', required=True)
    planning = sub.add_parser('plan', help='read-only inspection; write a new private plan, never delete')
    planning.add_argument('--campaign', required=True)
    planning.add_argument('--out', type=Path, required=True)
    applying = sub.add_parser('apply', help='permanently unlink only outputs in the revalidated plan')
    applying.add_argument('--plan', type=Path, required=True)
    applying.add_argument('--plan-sha256', required=True)
    applying.add_argument('--journal', type=Path, required=True)
    args = parser.parse_args()
    try:
        queue = args.queue_root.resolve(strict=True)
        output = (args.out if args.action == 'plan' else args.journal).absolute()
        retention.require(not output.is_relative_to(queue) and not output.resolve().is_relative_to(queue),
                          'audit output must be outside the live queue')
        if args.action == 'plan':
            data = retention.plan(queue, args.campaign)
            write_new(output, data)
            print(json.dumps(dict(candidates=len(data['targets']), preserved=len(data['preserved']),
                                  plan=str(output), sha256=hashlib.sha256(output.read_bytes()).hexdigest())))
        else:
            print(json.dumps(apply_plan(queue, args.plan.absolute(), args.plan_sha256, output)))
        return 0
    except (OSError, ValueError, KeyError, TypeError) as error:
        print('archive refused: ' + str(error), file=sys.stderr)
        return 1


if __name__ == '__main__':
    raise SystemExit(main())
