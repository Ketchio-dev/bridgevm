"""Give each regrouped module the right re-export, and let tests say what they use.

Three shapes come out of a regroup, and they need different mod.rs lines:

  module defines `pub` items          -> `pub use m::*;`
  module defines only pub(crate) items-> `pub(crate) use m::*;`
  module defines only impl blocks     -> no re-export at all

A `pub(crate) use m::*;` whose items only the test module consumes is reported
unused by the lib target, and deleting it breaks the tests. Rather than suppress
the lint or widen the items to `pub`, drop the glob, make the module
`pub(crate) mod`, and add an explicit `use super::super::m::*;` to the test file
that needs it -- the test then states its own dependency.

usage: fix_mod_reexports.py <pkg> <module-dir> [--features ...]
"""
import os, re, subprocess, sys

pkg, D = sys.argv[1], sys.argv[2]
rest = sys.argv[3:]
# A crate root declares its own siblings, so the file to fix is main.rs/lib.rs.
ROOT = rest[0] if rest and rest[0].endswith('.rs') else 'mod.rs'
feat = rest[1:] if ROOT != 'mod.rs' else rest
MOD = os.path.join(D, ROOT)


def check():
    r = subprocess.run(['cargo', 'check', '-p', pkg, '--all-targets',
                        '--message-format', 'short'] + feat, capture_output=True, text=True)
    return r.stdout + r.stderr


def module_kind(name):
    p = os.path.join(D, f'{name}.rs')
    if not os.path.exists(p):
        return 'pub'
    t = open(p).read()
    if re.search(r'^pub (struct|enum|fn|trait|const|static|type|union)\s', t, re.M):
        return 'pub'
    if re.search(r'^pub\(crate\) (struct|enum|fn|trait|const|static|type|union)\s', t, re.M):
        return 'pub(crate)'
    return None            # impl-only module: nothing to re-export


src = open(MOD).read().split('\n')
out = []
for l in src:
    m = re.match(r'^(pub(\([^)]*\))? )?use (\w+)::\*;$', l)
    if m:
        kind = module_kind(m.group(3))
        if kind is None:
            continue
        l = f'{kind} use {m.group(3)}::*;'
    out.append(l)
open(MOD, 'w').write('\n'.join(out))

# Whatever is still flagged is a glob only the tests consume.
for _ in range(4):
    unused = set(re.findall(rf'{re.escape(ROOT)}:\d+:\d+: warning: unused import: `(\w+)::\*`', check()))
    if not unused:
        break
    src = open(MOD).read().split('\n')
    out = []
    for l in src:
        m = re.match(r'^(pub(\([^)]*\))? )?use (\w+)::\*;$', l)
        if m and m.group(3) in unused:
            continue
        m2 = re.match(r'^(#\[macro_use\]\n)?mod (\w+);$', l)
        if m2 and m2.group(2) in unused:
            l = f'pub(crate) mod {m2.group(2)};'
        out.append(l)
    open(MOD, 'w').write('\n'.join(out))

    # repair the tests that lost those names
    missing = re.findall(r'((?:tests|tests_split)/[\w./]+\.rs):\d+:\d+: error\[E04\d\d\]: '
                         r'cannot find \w+ `(\w+)`', check())
    if not missing:
        break
    by_file = {}
    for rel, name in missing:
        for mod in unused:
            p = os.path.join(D, f'{mod}.rs')
            if os.path.exists(p) and re.search(rf'\b(fn|struct|enum|const|static|type)\s+{name}\b',
                                               open(p).read()):
                by_file.setdefault(os.path.join(D, rel), set()).add(mod)
    for f, mods in by_file.items():
        t = open(f).read()
        # tests/mod.rs sits one level shallower than tests/part_N.rs, so the
        # number of `super`s differs; `super::super` from mod.rs is the crate.
        up = 'crate' if ROOT != 'mod.rs' else ('super' if os.path.basename(f) == 'mod.rs' else 'super::super')
        add = ''.join(f'use {up}::{m}::*;\n' for m in sorted(mods)
                      if f'use {up}::{m}::*;' not in t)
        if add:
            i = t.index('\n', t.index('use ')) + 1 if 'use ' in t else 0
            open(f, 'w').write(t[:i] + add + t[i:])
    print(f'tests now import directly: {sorted({m for v in by_file.values() for m in v})}')
print('mod.rs re-exports settled')
