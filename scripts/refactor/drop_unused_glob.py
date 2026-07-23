"""Delete the `use` lines rustc reports as unused, verifying as we go.

cargo fix leaves glob imports alone, and the splitter adds `use super::*;` /
`use crate::*;` plus the file header to every generated part whether or not they
are needed. An import can also be unused by the lib target while the test module
in the same file still needs it, so a bulk deletion is verified against a full
`--all-targets` check and rolled back per line if it broke anything.
"""
import json, re, subprocess, sys, collections

pkg = sys.argv[1]
feat = sys.argv[2:]
WHOLE_USE = re.compile(r'\s*(pub(\([^)]*\))? )?use [^{}]+;')


def check(fmt='short'):
    r = subprocess.run(['cargo', 'check', '-p', pkg, '--all-targets',
                        '--message-format', fmt] + feat, capture_output=True, text=True)
    return r.stdout + r.stderr


def errors():
    return sum(1 for l in check().splitlines() if ': error' in l)


hits = collections.defaultdict(set)
for line in check('json').splitlines():
    try:
        m = json.loads(line)
    except Exception:
        continue
    msg = m.get('message') or {}
    if (msg.get('code') or {}).get('code') != 'unused_imports':
        continue
    for s in msg.get('spans', []):
        if s['file_name'].endswith('/mod.rs') or s['file_name'].endswith('lib.rs') \
                or s['file_name'].endswith('main.rs'):
            continue
        if s.get('text') and WHOLE_USE.fullmatch(s['text'][0]['text']):
            hits[s['file_name']].add(s['line_start'])

if not hits:
    print('dropped 0 unused imports')
    raise SystemExit

base = errors()
orig = {f: open(f).read() for f in hits}


def apply(f, drop):
    lines = orig[f].split('\n')
    open(f, 'w').write('\n'.join(l for i, l in enumerate(lines, 1) if i not in drop))


for f, ls in hits.items():
    apply(f, ls)

if errors() <= base:
    print(f'dropped {sum(len(v) for v in hits.values())} unused imports in {len(hits)} files')
    raise SystemExit

# Something in the batch was load-bearing for a target that did not report it
# (typically an import the lib does not use but the test module in the same file
# does). Restoring one line at a time costs one cargo check per line, which is
# untenable at 400+; instead read back the names rustc now says are missing and
# restore only the lines that provide them.
for _ in range(6):
    out = check()
    wanted = set(re.findall(r'cannot find \w+ `(\w+)`', out))
    wanted |= set(re.findall(r'unresolved import `[\w:]*::(\w+)`', out))
    wanted |= set(re.findall(r'failed to resolve: use of unresolved module or unlinked crate `(\w+)`', out))
    wanted |= set(re.findall(r'cannot find (?:type|value|function|trait|macro) `(\w+)`', out))
    if not wanted or errors() <= base:
        break
    restored = 0
    for f, ls in hits.items():
        src = orig[f].split('\n')
        keep = {ln for ln in ls
                if not any(re.search(rf'\b{re.escape(w)}\b', src[ln - 1]) for w in wanted)}
        if keep != ls:
            hits[f] = keep
            restored += len(ls) - len(keep)
    if not restored:
        break
    for f, ls in hits.items():
        apply(f, ls)

if errors() > base:
    for f in hits:
        open(f, 'w').write(orig[f])
    print('rolled back: batch deletion could not be made safe')
else:
    print(f'dropped {sum(len(v) for v in hits.values())} unused imports in {len(hits)} files')
