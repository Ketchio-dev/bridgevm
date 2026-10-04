#!/usr/bin/env python3
"""One owned ExFAT fixture; no user media, sudo, force detach, or source changes."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import platform
import re
import shutil
import signal
import stat
import tempfile
import time

from a19_exfat_fixture_io import Commands, content, save, write
from a19_exfat_fixture_mount import Mount, identity

SOURCE = Path(__file__).resolve().parents[2]
SHA = TREE = BASE = None


def imported(path):
    spec = importlib.util.spec_from_file_location("owned_group", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def check_source(commands, *, cleanup=False):
    def git(*args):
        code, raw, _ = commands.run(['/usr/bin/git', '-C', str(SOURCE), *args], 10, cleanup=cleanup)
        if code != 0:
            raise ValueError('source identity query refused')
        return raw.decode().strip()
    result = {'sha': git('rev-parse', 'HEAD'), 'tree': git('rev-parse', 'HEAD^{tree}'),
              'status': git('status', '--porcelain=v1')}
    if result != {'sha': SHA, 'tree': TREE, 'status': ''}:
        raise ValueError('source not exact clean focused commit')
    return result


def pair(root, name, first, second):
    paths = [root / f'{name}-disk', root / f'{name}-vars']
    for path, byte in zip(paths, (first, second)):
        write(path, bytes([byte]) * 4096)
    return paths, [content(p, 4096) for p in paths]


def snapshot(path):
    if not os.path.lexists(path):
        return {'present': False}
    if path.is_symlink() or not path.is_dir():
        return {'present': True, 'safe_directory': False}
    result = {'present': True, 'identity': identity(path), 'entries': {}}
    for p in sorted(path.iterdir()):
        result['entries'][p.name] = content(p, 65536) if stat.S_ISREG(p.lstat().st_mode) else {'unsafe': True}
    manifest = path / 'manifest.json'
    if manifest.exists() and manifest.lstat().st_size <= 65536 and not manifest.is_symlink():
        result['manifest'] = json.loads(manifest.read_bytes())
    return result


def validated(value, expected, *, exact=False):
    members = {'disk.raw', 'vars.fd', 'manifest.json'}
    if not members.issubset(value.get('entries', {})):
        raise ValueError('snapshot missing product members')
    if exact and set(value['entries']) != members:
        raise ValueError('snapshot has extra members; exchange admission unproven')
    m = value.get('manifest', {})
    for field, wanted in zip(('disk', 'vars'), expected):
        actual = value['entries'][field + ('.raw' if field == 'disk' else '.fd')]
        if actual['sha256'] != wanted['sha256'] or actual['bytes'] != 4096:
            raise ValueError('snapshot pair differs from source')
        if m.get(field + '_bytes') != 4096 or m.get(field + '_sha256') != wanted['sha256']:
            raise ValueError('snapshot manifest differs from pair')
    return m


def invoke(commands, cli, result, root, name, destination, quota, sources):
    staged = destination.parent / f'.{destination.name}.staging'
    record = {'name': name, 'before': snapshot(destination), 'staging_before': snapshot(staged)}
    result['cases'].append(record)
    try:
        code, _, error = commands.run([str(cli), 'create', *map(str, sources), str(destination),
                                      'exfat-plan', str(quota)])
        record.update(returncode=code, stderr_preview=error[:4096].decode('utf-8', 'replace'),
                      stderr_sha256=hashlib.sha256(error).hexdigest())
    except BaseException as error:
        record['command_error'] = {'type': type(error).__name__, 'message': str(error)}
        raise
    finally:
        for label, path in [('after', destination), ('staging_after', staged)]:
            try:
                record[label] = snapshot(path)
            except BaseException as error:
                record[label] = {'readback_error': type(error).__name__, 'present': os.path.lexists(path)}
        save(root / f'case-{name}.private.json', record)
    return record


def execute():
    started = time.monotonic()
    write(BASE / 'exfat-fixture-execution-intent.private.json',
          (json.dumps({'source': SHA, 'single_attempt': True, 'utc_time_ns': time.time_ns()}) + '\n').encode())
    root = Path(tempfile.mkdtemp(prefix='exfat-fixture-one-run-', dir=BASE))
    root.chmod(0o700)
    group_path = SOURCE / 'scripts/live-gates/guest_input_owned_group.py'
    commands = Commands(root, imported(group_path).stop, started)
    result = {'schema': 'bridgevm.owned-exfat-development-fixture.v1', 'root': str(root),
              'source_expected': SHA, 'hardware': platform.platform(), 'cases': [],
              'success': False, 'cleanup_verified': False, 'claims': {'A19_pass': False, 'release': False},
              'runner_hashes': {p.name: content(p, 65536) for p in Path(__file__).parent.glob('a19_exfat_fixture_*.py')},
              'owned_group_source': content(group_path, 65536), 'budget_seconds': 300}
    mount = None
    cli = None
    paths = new_paths = None
    expected = new_expected = None
    previous = {}
    try:
        for number in (signal.SIGINT, signal.SIGTERM):
            previous[number] = signal.signal(number, lambda n, f: (_ for _ in ()).throw(InterruptedError(str(n))))
        result['source_before'] = check_source(commands)
        cargo = shutil.which('cargo')
        if not cargo:
            raise ValueError('source env toolchain unavailable')
        build_env = {key: os.environ[key] for key in ('PATH', 'HOME', 'RUSTUP_HOME', 'CARGO_HOME',
                     'DEVELOPER_DIR', 'TMPDIR') if key in os.environ}
        code, _, _ = commands.run([cargo, '+1.97.0', 'build', '-j2', '-p', 'bridgevm-hvf',
                                  '--manifest-path', str(SOURCE / 'Cargo.toml'), '--example', 'snapshot_pair_cli',
                                  '--locked', '--target-dir', str(root / 'target')], 180, env=build_env, cwd=SOURCE)
        if code != 0:
            raise ValueError('owned helper build refused')
        cli = root / 'target/debug/examples/snapshot_pair_cli'
        result['helper_before'] = content(cli, 128 * 1024**2)
        paths, expected = pair(root, 'original', 0x31, 0x72)
        result['source_pair_before'] = expected
        mount = Mount(root, commands)
        mount.create_attach()
        result['mount'] = mount.records
        refused = mount.path / 'quota-empty'
        refused.mkdir()
        low = invoke(commands, cli, result, root, 'one-byte-under', refused, 8191, paths)
        if low['returncode'] != 1 or low['before'] != low['after'] or low['staging_after']['present']:
            raise ValueError('exact one-byte-under refusal contract failed')
        for name in ('empty', 'absent'):
            dest = mount.path / name
            if name == 'empty':
                dest.mkdir()
            record = invoke(commands, cli, result, root, name, dest, 8192, paths)
            if record['returncode'] != 0:
                raise ValueError(f'{name} full product create refused; errno site unannotated')
            validated(record['after'], expected)
            code, _, _ = commands.run([str(cli), 'verify', str(dest)])
            if code != 0 or record['staging_after']['present']:
                raise ValueError('product verification or staging cleanup refused')
        new_paths, new_expected = pair(root, 'replacement', 0x14, 0x95)
        replacement = invoke(commands, cli, result, root, 'complete-replacement', mount.path / 'empty', 8192, new_paths)
        replacement['before_exact_shape'] = set(replacement['before']['entries']) == {'disk.raw', 'vars.fd', 'manifest.json'}
        if replacement['returncode'] == 0:
            validated(replacement['after'], new_expected)
            replacement['interpretation'] = 'supported replacement; no assumed unsupported exchange'
        else:
            validated(replacement['after'], expected)
            replacement['interpretation'] = 'old selection preserved; errno does not identify syscall'
            staged_value = replacement['staging_after']
            if staged_value['present']:
                validated(staged_value, new_expected)
                replacement['pair_state'] = 'old destination / complete new staging'
            else:
                replacement['pair_state'] = 'old destination / absent staging; earlier refusal possible'
            if not replacement['before_exact_shape']:
                replacement['interpretation'] = 'extra-member admission refusal; exchange unobserved'
        result['source_pair_after'] = [content(p, 4096) for p in paths]
        result['replacement_pair_after'] = [content(p, 4096) for p in new_paths]
        if result['source_pair_after'] != expected or result['replacement_pair_after'] != new_expected:
            raise ValueError('synthetic source pair changed')
        result['helper_after'] = content(cli, 128 * 1024**2)
        if result['helper_before'] != result['helper_after']:
            raise ValueError('built helper changed')
        result['source_after'] = check_source(commands)
        result['success'] = True
    except BaseException as error:
        result['failure'] = {'type': type(error).__name__, 'message': str(error)}
    finally:
        for label, owned_paths, wanted in [('original', paths, expected), ('replacement', new_paths, new_expected)]:
            if owned_paths is not None:
                try:
                    after = [content(p, 4096) for p in owned_paths]
                    result[label + '_final_pair'] = after
                    result[label + '_unchanged'] = after == wanted
                    if after != wanted:
                        result['success'] = False
                except BaseException as error:
                    result['success'] = False
                    result[label + '_readback_error'] = type(error).__name__
        if cli is not None and cli.exists():
            try:
                result['helper_final'] = content(cli, 128 * 1024**2)
                result['helper_unchanged'] = result['helper_final'] == result.get('helper_before')
                result['success'] &= result['helper_unchanged']
            except BaseException as error:
                result['success'] = False
                result['helper_readback_error'] = type(error).__name__
        try:
            released = mount.release() if mount is not None else True
            result['cleanup_verified'] = released and commands.cleanup_verified
        except BaseException as error:
            result['cleanup_error'] = {'type': type(error).__name__, 'message': str(error)}
        if mount is not None:
            result['mount'] = mount.records
        try:
            result['source_terminal'] = check_source(commands, cleanup=True)
        except BaseException as error:
            result['success'] = False
            result['source_terminal_error'] = {'type': type(error).__name__, 'message': str(error)}
        result['success'] &= result['cleanup_verified']
        result['elapsed_seconds'] = time.monotonic() - started
        result['commands'] = commands.records
        result['image_retained'] = mount is not None and mount.image.exists()
        result['limits'] = ['GitHub-hosted deterministic filesystem evidence only',
                            'No physical power loss, Windows, security or criterion evidence',
                            'Nonempty errno attribution remains unannotated without auxiliary syscall proof']
        try:
            save(root / 'result.private.json', result)
            result['receipt_saved'] = True
        except BaseException as error:
            result['success'] = False
            result['receipt_saved'] = False
            result['receipt_save_error'] = type(error).__name__
        finally:
            for number, handler in previous.items():
                signal.signal(number, handler)
    print(json.dumps({'result': str(root / 'result.private.json'), 'success': result['success'],
                      'cleanup_verified': result['cleanup_verified'], 'elapsed': result['elapsed_seconds'],
                      'receipt_saved': result['receipt_saved'], 'receipt_save_error': result.get('receipt_save_error')}))
    return 0 if result['success'] else 1


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--execute', action='store_true')
    parser.add_argument('--source-sha', required=True)
    parser.add_argument('--source-tree', required=True)
    parser.add_argument('--output-root', required=True, type=Path)
    args = parser.parse_args()
    if not args.execute:
        parser.error('Explicit --execute required; no device mutation by default')
    if not all(re.fullmatch(r'[0-9a-f]{40}', value) for value in (args.source_sha, args.source_tree)):
        parser.error('Exact lowercase source SHA and tree are required')
    runner_temp = Path(os.environ['RUNNER_TEMP']).resolve(strict=True)
    if not args.output_root.is_absolute() or args.output_root.parent.resolve(strict=True) != runner_temp:
        parser.error('Fresh output must be an immediate child of RUNNER_TEMP')
    if not args.output_root.name.startswith('a19-exfat-') or os.path.lexists(args.output_root):
        parser.error('Owned output namespace must be new')
    SHA, TREE = args.source_sha, args.source_tree
    args.output_root.mkdir(mode=0o700)
    BASE = args.output_root.resolve(strict=True)
    raise SystemExit(execute())
