"""Fail-closed plans for two reviewed historical performance campaign formats."""
import hashlib
import json
import os
from pathlib import Path
import re
import stat

FORMATS = {
    ('t15-hvf-boot-performance', '799dcf345a05dbf8b272354f987fcec8f2b23bd0'):
        'shipping-core-3d-boot-v1',
    ('t16-hvf-nvme-performance', '3ffb66b1608b1a69ad0d2dae579c9bb59f1aadba'):
        'windows-nvme-warm-seq-v1',
}


def require(ok, message):
    if not ok:
        raise ValueError(message)


def regular(path):
    s = path.lstat()
    require(stat.S_ISREG(s.st_mode) and path.resolve() == path,
            'not a direct regular file: ' + str(path))
    return s


def metadata(path):
    s = regular(path)
    require(s.st_nlink == 1, 'hardlinked output: ' + str(path))
    return [s.st_dev, s.st_ino, s.st_size, s.st_mtime_ns, s.st_blocks]


def sha(path):
    before = regular(path)
    h = hashlib.sha256()
    with path.open('rb') as f:
        for block in iter(lambda: f.read(4 * 1024 * 1024), b''):
            h.update(block)
    after = regular(path)
    require((before.st_ino, before.st_size, before.st_mtime_ns) ==
            (after.st_ino, after.st_size, after.st_mtime_ns), 'file changed while hashing')
    return h.hexdigest()


def env(path):
    regular(path)
    pairs = [line.split('=', 1) for line in path.read_text().splitlines() if '=' in line]
    require(len({x[0] for x in pairs}) == len(pairs), 'duplicate environment key')
    return dict(pairs)


def idle(queue):
    for name in ('done', 'queued', 'running'):
        p = queue / name
        require(p.is_dir() and p.resolve() == p, 'invalid queue directory: ' + name)
    require(not any((queue / 'queued').iterdir()) and not any((queue / 'running').iterdir()),
            'queue has pending or active work')
    fence = queue / 'worker-cleanup-required'
    require(not fence.exists() and not fence.is_symlink(), 'worker cleanup is unconfirmed')


def check_run(directory, campaign):
    regular(directory / 'receipt.json')
    receipt = json.loads((directory / 'receipt.json').read_text())
    require(receipt.get('campaign_id') == campaign, 'campaign mismatch')
    require(receipt.get('valid') is True and receipt.get('pass') is True, 'invalid or failed run')
    key = (receipt.get('tier'), receipt.get('commit'))
    require(key in FORMATS and receipt.get('workload_profile') == FORMATS[key],
            'unreviewed tier/harness format')
    job = env(directory / 'job.env')
    result = env(directory / 'result.env')
    require(job.get('job_id') == directory.name == receipt.get('job_id') and
            job.get('commit') == receipt['commit'] and job.get('tier') == receipt['tier'] and
            bool(job.get('finished_at')), 'job identity or completion mismatch')
    require(result.get('result') == 'pass' and result.get('exit_code') == '0', 'job did not finish')
    cleanup = directory / 'boot/cleanup.txt'
    regular(cleanup)
    require('cleanup_status=0' in cleanup.read_text().splitlines(), 'cleanup not confirmed')
    checks = [('input-manifest.tsv', 'input_manifest_sha256')]
    if receipt['tier'].startswith('t16'):
        checks += [('share/nvme-result.json', 'result_sha256'),
                   ('share/nvme-raw.json', 'raw_sha256'),
                   ('share/nvme-result.done', 'done_sha256'), ('power-source.log', 'power_log_sha256')]
    else:
        require(regular(directory / 'boot/boot-timer-report.tsv').st_size > 0, 'missing boot timing')
    for name, field in checks:
        require(sha(directory / name) == receipt.get(field), 'receipt hash mismatch: ' + name)
    inputs = {}
    for line in (directory / 'input-manifest.tsv').read_text().splitlines():
        row = line.split('\t')
        if len(row) != 3:
            continue
        name, source, digest = row
        require(name not in inputs and re.fullmatch('[0-9a-f]{64}', digest), 'invalid manifest')
        p = Path(source).resolve(strict=True)
        s = regular(p)
        inputs[name] = dict(path=str(p), recorded_sha256=digest,
                            identity=[s.st_dev, s.st_ino, s.st_size, s.st_mtime_ns],
                            freshly_hashed=False)
    for name in ('image', 'vars'):
        require(name in inputs and inputs[name]['recorded_sha256'] == receipt.get(name + '_sha256'),
                'input identity mismatch: ' + name)
    return receipt, inputs


def plan(queue, campaign):
    queue = queue.resolve(strict=True)
    require(re.fullmatch('[0-9a-f]{28,64}', campaign), 'invalid campaign ID')
    idle(queue)
    runs, targets, preserved = [], [], {}
    for directory in sorted((queue / 'done').iterdir()):
        receipt_path = directory / 'receipt.json'
        if not receipt_path.is_file():
            continue
        regular(receipt_path)
        receipt = json.loads(receipt_path.read_text())
        if receipt.get('campaign_id') != campaign:
            continue
        receipt, inputs = check_run(directory, campaign)
        ordinal = receipt.get('campaign_ordinal')
        require(type(ordinal) is int and 1 <= ordinal <= 20 and receipt.get('campaign_expected_runs') == 20,
                'unsupported campaign size or ordinal')
        disk = directory / 'media/target.raw'
        exists = disk.exists() or disk.is_symlink()
        if exists:
            identity = metadata(disk)
            require(identity[:2] != inputs['image']['identity'][:2], 'output is the source image')
        require(ordinal != 1 or exists, 'representative disk is missing')
        if ordinal != 1 and exists:
            targets.append(dict(path=str(disk), identity=identity))
        runs.append(dict(job_id=directory.name, ordinal=ordinal, receipt_sha256=sha(receipt_path),
                         inputs=inputs, action='keep' if ordinal == 1 else 'delete' if exists else 'already absent'))
        for current, dirs, files in os.walk(directory, followlinks=False):
            require('.git' not in dirs and '.git' not in files, 'nested Git repository in job')
            for name in dirs:
                require(not (Path(current) / name).is_symlink(), 'symlink directory in job')
            for name in files:
                p = Path(current) / name
                if p == disk:
                    continue
                require(regular(p).st_size <= 256 * 1024 * 1024, 'large unclassified evidence file: ' + str(p))
                preserved[str(p)] = sha(p)
    require(len(runs) == 20 and sorted(r['ordinal'] for r in runs) == list(range(1, 21)),
            'campaign is incomplete or has duplicate ordinals')
    candidate_ids = {tuple(t['identity'][:2]) for t in targets}
    for run in runs:
        require(not any(tuple(v['identity'][:2]) in candidate_ids for v in run['inputs'].values()),
                'output clone is another run input')
    # Scan declarations beyond this campaign, resolving existing aliases by inode.
    for current, dirs, files in os.walk(queue.parent, followlinks=False):
        dirs[:] = [d for d in dirs if d != '.git']
        for name in files:
            if not name.endswith('.tsv') or ('manifest' not in name and 'manifests' not in current):
                continue
            p = Path(current) / name
            require(regular(p).st_size <= 8 * 1024 * 1024, 'unscannable input manifest')
            for line in p.read_text(errors='replace').splitlines():
                row = line.split('\t')
                if len(row) < 2 or not row[1].startswith('/'):
                    continue
                ref = Path(row[1])
                if ref.exists():
                    s = ref.stat()
                    require((s.st_dev, s.st_ino) not in candidate_ids, 'output referenced by ' + str(p))
                for target in targets:
                    require(row[1].replace('/running/', '/done/').endswith(
                            str(Path(target['path']).relative_to(queue / 'done'))) is False,
                            'declared output dependency')
    return dict(version=1, queue=str(queue), campaign=campaign, runs=runs,
                targets=targets, preserved=preserved,
                limitation='Source hashes are recorded, not freshly verified. Final disk states are not recoverable.')
