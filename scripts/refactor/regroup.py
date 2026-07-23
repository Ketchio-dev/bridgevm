"""Regroup an already-split module directory by responsibility.

The line-count splitter produced modules named after whatever item happened to
land first (`magic_value.rs`) and `_impl_N.rs` slices of a single impl block.
This moves the same code into modules named for what it does. No function body
changes; only which file an item lives in and what that file is called.

Input spec (JSON):
  {"targets": {"<new-file>.rs": "<doc line>"},
   "items":   {"<item name>": "<new-file>.rs"},
   "methods": {"<Type>::<method>": "<new-file>.rs"}}

Every parsed item and method must appear in the spec: an unassigned one is an
error, never a silent drop. That rule exists because the mechanical splitter
deleted a `macro_rules!` and a `#[derive(Parser)]` by dropping what it had not
classified.

usage: regroup.py <module-dir> <spec.json> [--apply]
"""
import json, os, re, sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from braces import brace_delta
from flatten_use import flatten


def delta3(line):
    """({}, [], ()) deltas, ignoring comments and string/char literals."""
    out, i, n = [0, 0, 0], 0, len(line)
    pairs = {'{': (0, 1), '}': (0, -1), '[': (1, 1), ']': (1, -1),
             '(': (2, 1), ')': (2, -1)}
    while i < n:
        c = line[i]
        if c == '/' and i + 1 < n and line[i + 1] == '/':
            break
        if c == '"':
            i += 1
            while i < n and line[i] != '"':
                i += 2 if line[i] == '\\' else 1
        elif c == "'":
            j = i + 1
            while j < n and line[j] != "'":
                j += 2 if line[j] == '\\' else 1
            if j < n and j - i <= 3:
                i = j
        elif c in pairs:
            k, v = pairs[c]
            out[k] += v
        i += 1
    return out

D, SPEC = sys.argv[1], sys.argv[2]
APPLY = '--apply' in sys.argv
spec = json.load(open(SPEC))
targets, item_map, method_map = spec['targets'], spec['items'], spec['methods']
# Modules that are already named for their responsibility stay untouched.
ROOT = spec.get('root', 'mod.rs')   # crate roots declare their own siblings
SKIP = set(spec.get('keep', [])) | {ROOT, 'mod.rs'}

ATTR = ('#[', '///', '//!', '//')
ITEM = re.compile(r'^(pub(\([^)]*\))? )?(unsafe )?(async )?(extern "[^"]*" )?'
                  r'(fn|struct|enum|trait|const|static|type|union|mod)\s+([A-Za-z_]\w*)'
                  r'|^macro_rules!\s+([A-Za-z_]\w*)')
IMPL = re.compile(r'^(unsafe )?impl(<[^>]*>)?\s')
METHOD = re.compile(r'^    (pub(\([^)]*\))? )?(default )?(const )?(async )?(unsafe )?'
                    r'(extern "[^"]*" )?fn ([A-Za-z_]\w*)')
# associated consts and types are named members too, not anonymous filler
ASSOC = re.compile(r'^    (pub(\([^)]*\))? )?(const|type)\s+([A-Za-z_]\w*)')


def code_of(line):
    """The line with any trailing `//` comment removed (string-literal aware)."""
    i, n = 0, len(line)
    while i < n:
        c = line[i]
        if c == '/' and i + 1 < n and line[i + 1] == '/':
            return line[:i]
        if c == '"':
            i += 1
            while i < n and line[i] != '"':
                i += 2 if line[i] == '\\' else 1
        elif c == "'":
            j = i + 1
            while j < n and line[j] != "'":
                j += 2 if line[j] == '\\' else 1
            if j < n and j - i <= 3:
                i = j
        i += 1
    return line


def item_end(lines, j):
    """Index one past the item starting at `lines[j]`.

    A method signature can span many lines before its `{`, so brace depth alone
    ends the item on its first line. Track parens/brackets too, and only accept
    an end once everything is balanced and the line closes with `}` or `;`.
    """
    d, k = [0, 0, 0], j
    while k < len(lines):
        d = [a + b for a, b in zip(d, delta3(lines[k]))]
        k += 1
        # A trailing `// comment` used to hide the terminating `;`, so the item
        # ran on and swallowed everything up to the next line that happened to
        # end in punctuation -- `MASK_REG` absorbed the whole `CfgAddr` struct.
        if d == [0, 0, 0] and code_of(lines[k - 1]).rstrip().endswith(('}', ';')):
            return k
    return len(lines)


def mask_raw_strings(lines):
    """Blank the interior of `r"..."` / `r#"..."#` literals.

    Raw strings span lines and routinely contain braces -- bridgevm-config
    embeds its whole JSON Schema in one -- so a per-line brace counter reads
    them as code and every item boundary after them is wrong. Boundaries are
    computed on the masked copy; emitted text always comes from the original.

    The `r` only opens a raw string at a token boundary: the literal
    `"...--apple-vz-runner"` ends in `r"`, and treating that as an opener
    swallowed the rest of the file.
    """
    out, depth = [], 0
    for l in lines:
        if depth:
            close = re.compile('"' + '#' * (depth - 1) + r'(?!#)')
            c = close.search(l)
            if c:
                out.append(' ' * c.end() + l[c.end():]); depth = 0
            else:
                out.append(' ' * len(l))
            continue
        masked, i, n = '', 0, len(l)
        while i < n:
            ch = l[i]
            if ch == '/' and i + 1 < n and l[i + 1] == '/':
                masked += l[i:]; break
            m = re.match(r'r(#*)"', l[i:])
            if m and (i == 0 or not (l[i - 1].isalnum() or l[i - 1] == '_')):
                depth = len(m.group(1)) + 1
                masked += ' ' * m.end(); i += m.end()
                close = re.compile('"' + '#' * (depth - 1) + r'(?!#)')
                c = close.search(l, i)
                if c:
                    masked += ' ' * (c.end() - i); i = c.end(); depth = 0
                else:
                    masked += ' ' * (n - i); i = n
                continue
            if ch == '"':                      # ordinary string: copy verbatim
                j = i + 1
                while j < n and l[j] != '"':
                    j += 2 if l[j] == '\\' else 1
                masked += l[i:min(j + 1, n)]; i = min(j + 1, n)
                continue
            masked += ch; i += 1
        out.append(masked)
    return out


def parse(path):
    """-> (header use-lines, [(kind, name, lines)])  kind: 'item' | ('impl', ty)"""
    lines = open(path).read().split('\n')
    scan = mask_raw_strings(lines)
    i, header, inner = 0, [], []
    while i < len(lines):
        l = scan[i]
        if l.startswith('use ') or l.startswith('pub use ') or re.match(r'^pub\([^)]*\) use ', l):
            d = brace_delta(l); blk = [l]
            while d > 0 and i + 1 < len(lines):
                i += 1; blk.append(lines[i]); d += brace_delta(scan[i])
            header += blk
        elif l.startswith('#!['):
            inner.append(l)         # must lead every target file, in source order
        elif l.startswith('//!') or not l.strip():
            pass
        else:
            # `#[...]` and `///` here belong to the FIRST ITEM, not to the file.
            # Swallowing them in the header scan is how the earlier splitter
            # deleted a `#[derive(Parser)]`; the round-trip check catches it.
            break
        i += 1

    out, pend, pend_at = [], [], None
    while i < len(lines):
        l = scan[i]
        if not l.strip():
            i += 1; continue
        if l.startswith(ATTR):
            if pend_at is None: pend_at = i
            # `#[command(\n .. \n)]` spans lines; its continuation lines do not
            # start with `#[`, so a line-at-a-time scan ended the attribute run
            # early and the item that followed came out nameless.
            if l.startswith('#[') and delta3(l) != [0, 0, 0]:
                # An attribute ends when its own brackets balance. It closes with
                # `)]`, which item_end() rejects, so reusing item_end here made
                # the attribute swallow the struct that followed it.
                d, k = [0, 0, 0], i
                while k < len(lines):
                    d = [a + b for a, b in zip(d, delta3(scan[k]))]
                    k += 1
                    if d == [0, 0, 0]:
                        break
                pend += lines[i:k]; i = k; continue
            pend.append(l); i += 1; continue
        # `i - len(pend)` was wrong whenever a blank line sat inside the run of
        # attributes/comments, which silently cut off section banners.
        start = pend_at if pend_at is not None else i
        m_impl = IMPL.match(l)
        if m_impl:
            ty = re.sub(r'^(unsafe )?impl(<[^>]*>)?\s+', '', l)
            ty = re.sub(r'\s*(where.*)?\{.*$', '', ty).strip()
            hdr_end, d = i, 0
            while hdr_end < len(lines):
                d += brace_delta(scan[hdr_end])
                if d > 0: break
                hdr_end += 1
            end, d = i, 0
            while end < len(lines):
                d += brace_delta(scan[end])
                end += 1
                if d <= 0 and end > i: break
            body, mpend, j, methods = lines[hdr_end + 1:end - 1], [], 0, []
            sbody = scan[hdr_end + 1:end - 1]
            while j < len(body):
                bl = sbody[j]
                if not bl.strip():
                    j += 1; continue
                if bl.strip().startswith(ATTR):
                    mpend.append(bl); j += 1; continue
                mm = METHOD.match(bl)
                k = item_end(sbody, j)
                if mm:
                    mname = mm.group(8)
                else:
                    ma = ASSOC.match(bl)
                    mname = ma.group(4) if ma else None
                methods.append((mname, mpend + body[j:k]))
                mpend, j = [], k
            if methods:
                out.append(('impl', ty, lines[i:hdr_end + 1], methods, lines[start:i]))
            else:
                # A marker impl (`unsafe impl Send for X {}`) has no methods, so
                # the method loop yields nothing and it would vanish. Treat it
                # as an item keyed by its trait-and-type.
                out.append(('item', f'impl {ty}', lines[start:end], None, None))
            i = end
            pend, pend_at = [], None
            continue
        m = ITEM.match(l)
        name = (m.group(7) or m.group(8)) if m else None
        if name is None and re.match(r'^(pub(\([^)]*\))? )?use ', l):
            # a `use` sitting among the items (aliased imports do this); key it
            # by its own text so it is assignable like anything else
            name = 'use ' + re.sub(r'^(pub(\([^)]*\))? )?use\s+', '', l).rstrip(';').strip()
        # An item ends when braces AND brackets AND parens are all balanced and
        # the line closes it. Tracking only `{}` cut multi-line items such as
        # `const X: &[u8] = &[\n 1, 2,\n];` after their first line.
        end = item_end(scan, i)
        out.append(('item', name, lines[start:end], None, None))
        i = end
        pend, pend_at = [], None
    if pend and out:
        # A trailing banner with no item after it -- the mechanical splitter cut
        # a section header off from its section. Keep it with the last item so
        # the round-trip stays exact; it is dropped only when its file is.
        kind, name, body, methods, attrs = out[-1]
        out[-1] = (kind, name, body if kind == 'impl' else body + pend, methods,
                   attrs if kind != 'impl' else attrs)
        if kind == 'impl':
            out.append(('item', '__trailing_comment__', pend, None, None))
    return inner + header, out


header, parsed = [], []
for fn in sorted(os.listdir(D)):
    if not fn.endswith('.rs') or fn in SKIP:
        continue
    h, items = parse(os.path.join(D, fn))
    header += h
    parsed.append((fn, items))

seen, missing, buckets = set(), [], {t: [] for t in targets}
for fn, items in parsed:
    for kind, name, body, methods, attrs in items:
        if kind == 'item':
            if name is None:
                missing.append(f'{fn}: unparsed item starting {body[0][:60]!r}')
                continue
            t = item_map.get(name)
            if not t:
                missing.append(f'{fn}: item {name}')
                continue
            buckets[t].append('\n'.join(body))
            seen.add(name)
        else:
            ty = name
            for mname, mbody in methods:
                key = f'{ty}::{mname}' if mname else None
                t = method_map.get(key) if key else item_map.get(f'{ty}::assoc')
                if not t:
                    missing.append(f'{fn}: method {key or (ty + "::<assoc> " + repr(mbody[0][:70]))}')
                    continue
                buckets[t].append(('IMPL', ty, '\n'.join(attrs + body), '\n'.join(mbody)))
                seen.add(key)

if missing:
    print(f'UNASSIGNED ({len(missing)}):')
    for m in (missing if '--all' in sys.argv else missing[:60]):
        print('  ' + m)
    sys.exit(1)

extra = [k for k in list(item_map) + list(method_map) if k not in seen]
if extra:
    print(f'SPEC NAMES NOT FOUND ({len(extra)}): ' + ', '.join(extra[:20]))
    sys.exit(1)

plan = {}
for t, entries in buckets.items():
    chunks, impls = [], {}
    for e in entries:
        if isinstance(e, tuple):
            _, ty, ihdr, mbody = e
            impls.setdefault((ty, ihdr), []).append(mbody)
        else:
            chunks.append(e)
    for (ty, ihdr), ms in impls.items():
        chunks.append(ihdr + '\n' + '\n\n'.join(ms) + '\n}')
    body = '\n\n'.join(chunks)
    # The header is the union of every source file's imports, so it can name a
    # symbol twice (E0252) or import one that this target now *defines* (E0255).
    defined = set(re.findall(r'^(?:pub(?:\([^)]*\))? )?(?:struct|enum|trait|const|static|type|fn|union)'
                             r'\s+(\w+)', body, re.M))
    # Flatten to one leaf per statement first: the header is a list of LINES,
    # and deduping those directly tore a multi-line `use crate::{ .. };` apart.
    stmts, cur = [], ''
    for h in header:
        if h.startswith('#!'):
            stmts.append(h); continue
        cur += ('\n' if cur else '') + h
        if brace_delta(cur) == 0 and cur.rstrip().endswith(';'):
            stmts += [x.strip() for x in flatten(cur)]
            cur = ''
    if cur:
        stmts.append(cur)
    hdr, seen_leaf = [], set()
    for h in stmts:
        leaf = re.sub(r'\s+as\s+\w+;?$', '', re.sub(r';\s*$', '', h)).split('::')[-1].strip()
        if h.startswith('#!'):
            hdr.insert(0, h); continue
        # A glob's leaf is `*`, so de-duplicating by leaf collapsed every
        # `use crate::*;` in the crate down to one module's copy.
        if leaf != '*' and (leaf in defined or leaf in seen_leaf):
            continue
        if leaf == '*' and h in hdr:
            continue
        seen_leaf.add(leaf)
        hdr.append(h)
    # Every generated module must be able to see its siblings: through the
    # crate root for a crate-root split, through the parent for a mod.rs one.
    reach = 'use crate::*;' if ROOT != 'mod.rs' else 'use super::*;'
    if reach not in hdr:
        hdr.insert(len([h for h in hdr if h.startswith('#!')]), reach)
    text = f'//! {targets[t]}\n\n' + '\n'.join(hdr) + '\n\n' + body + '\n'
    plan[t] = text

for t in sorted(plan):
    print(f'{t}: {len(plan[t].splitlines())} lines')

if not APPLY:
    print('\n(dry run -- pass --apply to write)')
    sys.exit(0)

for fn, _ in parsed:
    os.remove(os.path.join(D, fn))
for t, text in plan.items():
    open(os.path.join(D, t), 'w').write(text)

mod = open(os.path.join(D, ROOT)).read().split('\n')
kept_mods = {f[:-3] for f in SKIP if f != 'mod.rs'}
keep = [l for l in mod
        if not re.match(r'^(#\[macro_use\]\s*)?(pub(\([^)]*\))? )?(mod|use) \w+(::\*)?;$', l)
        or 'tests' in l
        or any(re.search(rf'\b{k}\b', l) for k in kept_mods)]
names = [t[:-3] for t in sorted(plan)]
# macro_rules! is resolved in textual order, so a module that defines one must
# be declared before every module that expands it.
macro_mods = [n for n in names if 'macro_rules!' in plan[f'{n}.rs']]
decls = (''.join(f'#[macro_use]\nmod {n};\n' for n in macro_mods)
         + ''.join(f'mod {n};\n' for n in names if n not in macro_mods))
# `pub use`, not `pub(crate) use`: a glob re-export cannot widen an item beyond
# its own visibility, and a public re-export is not linted as unused when only
# the test target consumes it -- which is what flagged display::* here.
reex = ''.join(f'pub use {n}::*;\n' for n in names)
root_text = ('\n'.join(l for l in keep if l.strip()).rstrip('\n')
             if ROOT == 'mod.rs' else '\n'.join(keep).rstrip('\n'))
open(os.path.join(D, ROOT), 'w').write(root_text + '\n\n' + decls + '\n' + reex)
print(f'\nwrote {len(plan)} modules into {D}')
