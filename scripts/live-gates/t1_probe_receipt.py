"""Bind legacy T1 observations; no Hypervisor.framework calls or product promotion."""
import json
from pathlib import Path
import re
import subprocess
import sys
from datetime import datetime, timezone

from t1_probe_artifact import load, save_new, snapshot, source

GATE = 't1-legacy-recovery-experiment-v1'
DETAIL_KEYS = {'probe', 'iterations', 'timer_wakes', 'canceled_exits', 'surplus_canceled',
               'masked_past_deadline', 'recoveries', 'swallowed_unrecovered', 'vtimer_exits',
               'trace_overflow', 'outcome', 'elapsed_ms', 'pass', 'failures'}
CONFIG_KEYS = {'arm_ticks', 'cancel_interval_us', 'stall_timeout_ms', 'recovery_enabled', 'quiesce_probe'}
BINDINGS = {'gate_id', 'schema_version', 'tier', 'job_id', 'commit', 'source_tree', 'binary_hash',
            'binary_source_commit', 'binary_profile', 'binary_features', 'rust_toolchain',
            'artifact_signing_class', 'probe_receipt_sha256', 'run_log_sha256', 'build_seal_sha256',
            'config_sha256', 'command_exit_code', 'host_model', 'macos_version', 'finished_at',
            'criterion_pass', 'claim_eligible', 'capability_promotion'}


def validate_detail(value):
    if not isinstance(value, dict) or set(value) != DETAIL_KEYS or value['probe'] != 'hvf_vtimer_cancel':
        raise ValueError('T1 detailed schema differs')
    for key in DETAIL_KEYS - {'probe', 'outcome', 'pass', 'failures'}:
        if type(value[key]) is not int or value[key] < 0:raise ValueError('T1 counter is not an unsigned integer')
    failures = []
    if value['outcome'] not in ['completed', 'stalled', 'faulted']:raise ValueError('T1 outcome differs')
    if value['outcome'] != 'completed':failures.append(f"run outcome was {value['outcome']}")
    if value['timer_wakes'] != value['iterations']:failures.append(f"timer_wakes {value['timer_wakes']} != iterations {value['iterations']}")
    missing = max(value['masked_past_deadline'] - value['recoveries'], 0)
    if missing:failures.append(f'swallowed_unrecovered {missing}')
    if value['trace_overflow']:failures.append(f"trace_overflow {value['trace_overflow']}")
    if value['vtimer_exits']:failures.append(f"vtimer_exits {value['vtimer_exits']} (in-kernel GIC delivery expected)")
    if value['iterations'] == 0 or value['swallowed_unrecovered'] != missing or type(value['pass']) is not bool or value['pass'] != (not failures) or value['failures'] != failures:
        raise ValueError('T1 detailed verdict differs from counters')
    return value


def validate(directory, value, commit, job_id, require_default=False):
    if not isinstance(value, dict) or set(value) != DETAIL_KEYS | CONFIG_KEYS | BINDINGS:
        raise ValueError('T1 receipt schema differs')
    validate_detail({key: value[key] for key in DETAIL_KEYS});detail = validate_detail(load(directory / 'vtimer-cancel-receipt.json'))
    if any(value[key] != detail[key] for key in DETAIL_KEYS):raise ValueError('T1 detailed fields differ')
    if value['gate_id'] != GATE or type(value['schema_version']) is not int or value['schema_version'] != 1 or value['tier'] != 't1-vtimer' or value['commit'] != commit or value['job_id'] != job_id:
        raise ValueError('T1 queue identity differs')
    if not re.fullmatch('[0-9a-f]{40}', commit) or not re.fullmatch('[A-Za-z0-9][A-Za-z0-9._-]{0,127}', job_id):raise ValueError('T1 identity is malformed')
    for key in ['binary_hash', 'probe_receipt_sha256', 'run_log_sha256', 'build_seal_sha256', 'config_sha256']:
        if not isinstance(value[key], str) or not re.fullmatch('[0-9a-f]{64}', value[key]):raise ValueError('T1 hash is malformed')
    if not isinstance(value['source_tree'], str) or not re.fullmatch('[0-9a-f]{40}', value['source_tree']):raise ValueError('T1 tree is malformed')
    for key in ['criterion_pass', 'claim_eligible', 'capability_promotion']:
        if value[key] is not False:raise ValueError('T1 cannot promote product evidence')
    for key in ['recovery_enabled', 'quiesce_probe']:
        if type(value[key]) is not bool:raise ValueError('T1 mode is not boolean')
    for key in ['arm_ticks', 'cancel_interval_us', 'stall_timeout_ms']:
        if type(value[key]) is not int or value[key] < 0:raise ValueError('T1 timing is not unsigned')
    if value['arm_ticks'] == 0 or type(value['command_exit_code']) is not int or value['command_exit_code'] != (0 if detail['pass'] else 1):raise ValueError('T1 actual return differs')
    config = load(directory / 't1-run-config.json')
    for key in CONFIG_KEYS | {'iterations', 'commit', 'source_tree', 'job_id', 'binary_hash'}:
        if value[key] != config[key]:raise ValueError('T1 executed configuration differs')
    seal = load(directory / 't1-build-seal.json')
    for key in ['binary_hash', 'binary_source_commit', 'source_tree', 'binary_profile', 'binary_features', 'rust_toolchain', 'artifact_signing_class']:
        if value[key] != seal[key]:raise ValueError('T1 signed build seal differs')
    if seal['binary_source_commit'] != commit or seal['strict_signature_verified'] is not True or seal['hypervisor_entitlement_true'] is not True or seal['artifact_signing_class'] != 'ad-hoc' or seal['binary_profile'] != 'debug' or seal['binary_features'] != '':raise ValueError('T1 build provenance differs')
    for key, filename in [('probe_receipt_sha256', 'vtimer-cancel-receipt.json'), ('run_log_sha256', 't1-probe.log'), ('build_seal_sha256', 't1-build-seal.json'), ('config_sha256', 't1-run-config.json')]:
        if value[key] != snapshot(directory / filename)[1]:raise ValueError('T1 retained evidence changed')
    defaults = dict(iterations=10000, arm_ticks=1000, cancel_interval_us=0, stall_timeout_ms=5000, recovery_enabled=True, quiesce_probe=False)
    if require_default and any(value[key] != expected for key, expected in defaults.items()):raise ValueError('T1 publication requires unchanged default gate configuration')
    return value


def prepare_run(directory, binary, seal_path, job_id, config, iterations, argv):
    seal = load(seal_path)
    if source() != (seal['binary_source_commit'], seal['source_tree']) or snapshot(binary)[1] != seal['binary_hash']:raise ValueError('T1 prepared source or binary differs')
    base = ['--iterations', str(iterations), '--cancel-interval-us', str(config['cancel_interval_us']), '--arm-ticks', str(config['arm_ticks']), '--stall-timeout-ms', str(config['stall_timeout_ms'])]
    if argv[:8] != base or argv[-2:] != ['--receipt', str(directory / 'vtimer-cancel-receipt.json')]:raise ValueError('T1 actual argv differs')
    flags = argv[8:-2]
    if any(flag not in ['--no-recover', '--quiesce-probe'] for flag in flags) or config['recovery_enabled'] != ('--no-recover' not in flags) or config['quiesce_probe'] != ('--quiesce-probe' in flags):raise ValueError('T1 actual mode differs')
    value = dict(config, iterations=iterations, job_id=job_id, commit=seal['binary_source_commit'], source_tree=seal['source_tree'], binary_hash=seal['binary_hash'], argv=argv)
    save_new(directory / 't1-run-config.json', value)


def finalize(directory, binary, seal_path, job_id, status, config):
    directory = Path(directory)
    seal = load(seal_path)
    if source() != (seal['binary_source_commit'], seal['source_tree']) or snapshot(binary)[1] != seal['binary_hash']:raise ValueError('T1 source or binary changed while running')
    save_new(directory / 't1-build-seal.json', seal)
    value = dict(validate_detail(load(directory / 'vtimer-cancel-receipt.json')))
    value.update(config)
    for key in ['binary_hash', 'binary_source_commit', 'source_tree', 'binary_profile', 'binary_features', 'rust_toolchain', 'artifact_signing_class']:value[key] = seal[key]
    value.update(gate_id=GATE, schema_version=1, tier='t1-vtimer', job_id=job_id,
                 commit=seal['binary_source_commit'], command_exit_code=status,
                 host_model=subprocess.check_output(['sysctl', '-n', 'hw.model'], text=True).strip(),
                 macos_version=subprocess.check_output(['sw_vers', '-productVersion'], text=True).strip(),
                 finished_at=datetime.now(timezone.utc).isoformat(), criterion_pass=False,
                 claim_eligible=False, capability_promotion=False)
    for key, filename in [('probe_receipt_sha256', 'vtimer-cancel-receipt.json'), ('run_log_sha256', 't1-probe.log'), ('build_seal_sha256', 't1-build-seal.json'), ('config_sha256', 't1-run-config.json')]:value[key] = snapshot(directory / filename)[1]
    validate(directory, value, value['commit'], job_id)
    save_new(directory / 'receipt.json', value)


if __name__ == '__main__':
    try:
        mode, directory = sys.argv[1], Path(sys.argv[2])
        if mode in ['prepare-run', 'finalize']:
            numbers = [int(v) for v in sys.argv[7:10]]
            config = dict(zip(['arm_ticks', 'cancel_interval_us', 'stall_timeout_ms'], numbers))
            config.update(recovery_enabled=sys.argv[10] == '1', quiesce_probe=sys.argv[11] == '1')
            if mode == 'prepare-run':prepare_run(directory, sys.argv[3], sys.argv[4], sys.argv[5], config, int(sys.argv[6]), sys.argv[12:])
            else:finalize(directory, sys.argv[3], sys.argv[4], sys.argv[5], int(sys.argv[6]), config)
        else:
            receipt = Path(sys.argv[3]) if mode == 'verify' else directory / 'receipt.public.json'
            commit, job = sys.argv[4:6]
            value = validate(directory, load(receipt), commit, job, require_default=True)
            if mode == 'read':json.dump(value, sys.stdout, indent=2);print()
            elif mode != 'verify':raise ValueError('unknown T1 receipt command')
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        raise SystemExit(f'T1 evidence refused: {error}') from None
