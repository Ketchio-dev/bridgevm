"""Round-trip check for regroup.py's parser.

Parse every file in a module directory, reassemble the items in their original
order, and compare the non-blank lines with the original. Anything the parser
cannot see would vanish in a regroup, so this must be exact before any move.

usage: roundtrip.py <module-dir-or-rust-file> [...]
"""
import importlib.util, os, sys

SP = os.path.dirname(os.path.abspath(__file__))
spec = importlib.util.spec_from_file_location('regroup_src', f'{SP}/regroup.py')
src = open(f'{SP}/regroup.py').read().split('header, parsed = [], []')[0]
src = src.replace("D, SPEC = sys.argv[1], sys.argv[2]", "D = SPEC = None")
src = src.replace("APPLY = '--apply' in sys.argv", "APPLY = False")
src = src.replace("spec = json.load(open(SPEC))", "spec = {'targets': {}, 'items': {}, 'methods': {}}")
src = src.replace("sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))",
                  f"sys.path.insert(0, {SP!r})")
g = {'__name__': 'regroup_src'}
exec(src, g)
parse = g['parse']

bad = 0
for source in sys.argv[1:]:
    if os.path.isfile(source):
        paths = [source] if source.endswith('.rs') else []
    else:
        paths = [os.path.join(source, fn) for fn in sorted(os.listdir(source))
                 if fn.endswith('.rs') and fn != 'mod.rs']
    for p in paths:
        header, items = parse(p)
        out = list(header)
        for kind, name, body, methods, attrs in items:
            if kind == 'item':
                out += body
            else:
                out += attrs + body
                for _, mbody in methods:
                    out += mbody
                out += ['}']
        # the file-level //! doc is deliberately replaced per target
        orig = [l.rstrip() for l in open(p).read().split('\n')
                if l.strip() and not l.startswith('//!')]
        got = [l.rstrip() for l in out if l.strip() and not l.startswith('//!')]
        if orig != got:
            bad += 1
            print(f'MISMATCH {p}: {len(orig)} original vs {len(got)} reassembled')
            for i, (a, b) in enumerate(zip(orig, got)):
                if a != b:
                    print(f'   first diff at non-blank line {i}:')
                    print(f'     orig: {a[:90]}')
                    print(f'     got : {b[:90]}')
                    break
            else:
                extra = orig[len(got):] or got[len(orig):]
                print(f'   tail differs, e.g. {extra[0][:90]!r}')
print('ROUND-TRIP OK' if not bad else f'{bad} file(s) mismatched')
sys.exit(1 if bad else 0)
