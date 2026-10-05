#!/usr/bin/env python3
"""Owned command stubs exercise T1 without launching Hypervisor.framework."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
SHA = '1' * 40
TREE = '2' * 40
PROBE = '''#!/usr/bin/env python3
import json,sys
from pathlib import Path
args=sys.argv[1:];n=int(args[args.index('--iterations')+1])
d={'probe':'hvf_vtimer_cancel','iterations':n,'timer_wakes':n,'canceled_exits':19,'surplus_canceled':19,'masked_past_deadline':3,'recoveries':3,'swallowed_unrecovered':0,'vtimer_exits':0,'trace_overflow':0,'outcome':'completed','elapsed_ms':7,'pass':True,'failures':[]}
if __import__('os').environ.get('PROBE_FAIL')=='1':d.update(timer_wakes=n-1,outcome='stalled',pass_=False);d['pass']=False;d.pop('pass_',None);d['failures']=['run outcome was stalled',f'timer_wakes {n-1} != iterations {n}']
if '--receipt' in args:Path(args[args.index('--receipt')+1]).write_text(json.dumps(d))
print(json.dumps(d))
if __import__('os').environ.get('CHANGE_BINARY')=='1':p=Path(sys.argv[0]);p.write_text(p.read_text()+'\\n# changed after execution\\n')
sys.exit(0 if d['pass'] else 1)
'''


class TimerFixture(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        for relative in ['scripts/run-hvf-vtimer-cancel-gate.sh', 'scripts/live-gates/run-tier.sh']:
            p = self.root / relative
            p.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(ROOT / relative, p)
            p.chmod(0o700)
        copies = [*(ROOT / 'scripts/live-gates').glob('t1_probe_*.py'), *(ROOT / 'scripts/live-gates').glob('t1-probe-*.sh')]
        copies += [ROOT / 'scripts/live-gates' / name for name in ['publish-receipt.sh', 'verify-live-receipt.sh', 'verify-live-receipt-legacy.sh', 'redact-receipt.py', 'receipt_public_fields.py', 'receipt_extended_fields.py', 'development-queue-receipt-dispatch.sh']]
        for p in copies:
            target = self.root / 'scripts/live-gates' / p.name
            shutil.copyfile(p, target);target.chmod(0o700)
        (self.root / 'apps/macos').mkdir(parents=True)
        (self.root / 'apps/macos/HvfRunner.entitlements').write_text('fixture')
        self.tools = self.root / 'tools'
        self.tools.mkdir()
        self.record = self.root / 'commands.jsonl'
        self.stub('git', "if a[:2]==['status','--porcelain'] and os.environ.get('DIRTY_SOURCE')=='1':print(' M source.rs')\nelif a[:2]==['rev-parse','HEAD']:print('"+SHA+"')\nelif a[:2]==['rev-parse','HEAD^{tree}']:print('"+TREE+"')\nelif 'rev-parse' in a:print('"+SHA+"')")
        self.stub('cargo', "import os\np=Path(os.environ.get('CARGO_TARGET_DIR','target')).resolve()\nif a[0]=='metadata':print(json.dumps({'target_directory':str(p)}))\nelse:\n b=p/os.environ['CARGO_BUILD_TARGET'] if os.environ.get('CARGO_BUILD_TARGET') else p; b=b/'debug/examples/hvf_vtimer_cancel_probe';b.parent.mkdir(parents=True,exist_ok=True);b.write_text("+repr(PROBE)+");b.chmod(0o700)\n row={'reason':'compiler-artifact','target':{'name':'hvf_vtimer_cancel_probe','kind':['example']},'executable':str(b)}\n if not os.environ.get('NO_ARTIFACT'):print(json.dumps(row))\n if os.environ.get('DOUBLE_ARTIFACT'):print(json.dumps(row))")
        self.stub('codesign', "import plistlib\nif '--sign' in a and os.environ.get('SIGN_MARKER')=='1':\n p=Path(a[-1]);p.write_text(p.read_text()+'\\n# signed fixture\\n')\nif '--verify' in a and os.environ.get('BAD_SIGN')=='1':sys.exit(1)\nif '-d' in a:\n if '--xml' in a:sys.stdout.buffer.write(plistlib.dumps({'com.apple.security.hypervisor':os.environ.get('BAD_ENTITLEMENT')!='1'}))\n else:sys.stdout.write('[Dict]\\n [Key] com.apple.security.hypervisor\\n [Bool] true\\n')")
        self.stub('rustc', "print('rustc fixture-toolchain')")
        self.stub('sysctl', "print('owned-fixture-host')")
        self.stub('sw_vers', "print('owned-fixture-os')")
        self.env = dict(os.environ, PATH=str(self.tools)+':'+os.environ['PATH'], COMMAND_RECORD=str(self.record))
        self.env.pop('CARGO_TARGET_DIR', None)
        self.out = self.root / 'owned-t1'

    def stub(self, name, body):
        p = self.tools / name
        p.write_text('#!'+sys.executable+'\nimport json,os,sys\nfrom pathlib import Path\na=sys.argv[1:]\nwith open(os.environ["COMMAND_RECORD"],"a") as f:f.write(json.dumps([Path(sys.argv[0]).name,*a])+"\\n")\n'+body+'\n')
        p.chmod(0o700)

    def run_gate(self, tier=False, extra=()):
        argv = ['/bin/bash', str(self.root / ('scripts/live-gates/run-tier.sh' if tier else 'scripts/run-hvf-vtimer-cancel-gate.sh'))]
        if tier:argv += ['t1-vtimer', '--job-id', 'owned-t1']
        return subprocess.run([*argv, '--out', str(self.out), *extra], env=self.env, capture_output=True, text=True, timeout=15)

    def env_path(self):
        return Path(self.env['CARGO_TARGET_DIR'])

