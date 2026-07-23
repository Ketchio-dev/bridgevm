"""Flatten nested `use a::{b::C, d::{E, F}};` into one line per leaf symbol.
Makes duplicate detection and symbol-precise cleanup exact."""
import re

def _split_top(s):
    out, depth, cur = [], 0, ''
    for ch in s:
        if ch == '{': depth += 1
        elif ch == '}': depth -= 1
        if ch == ',' and depth == 0:
            out.append(cur); cur = ''
        else:
            cur += ch
    if cur.strip(): out.append(cur)
    return [x.strip() for x in out if x.strip()]

def flatten(stmt):
    """stmt: full `use ...;` text (may span lines). -> list of `use x::Y;`"""
    m = re.match(r'^(\s*)(pub(\([^)]*\))?\s+)?use\s+(.*);\s*$', stmt.strip().replace('\n', ' '), re.S)
    if not m: return [stmt]
    indent, vis, body = m.group(1) or '', (m.group(2) or '').strip(), m.group(4).strip()
    def expand(prefix, text):
        text = text.strip()
        if not text.startswith('{'):
            if '::{' not in text:
                return [f'{prefix}{text}']
            head, rest = text.split('::{', 1)
            return expand(f'{prefix}{head}::', '{' + rest)
        inner = text[1:text.rfind('}')]
        res = []
        for part in _split_top(inner):
            res += expand(prefix, part)
        return res
    leaves = expand('', body)
    pre = (vis + ' ') if vis else ''
    fixed = []
    for leaf in leaves:
        # `use a::b::{self, C}` flattens to `a::b::self`; that path means `a::b`
        if leaf.endswith('::self'): leaf = leaf[:-len('::self')]
        if leaf == 'self': continue
        fixed.append(leaf)
    return [f'{indent}{pre}use {leaf};' for leaf in fixed]

def flatten_text(text):
    lines = text.split('\n'); out = []; i = 0
    while i < len(lines):
        l = lines[i]
        if re.match(r'^\s*(pub(\([^)]*\))?\s+)?use\s', l):
            d = l.count('{') - l.count('}'); blk = [l]
            while d > 0 and i + 1 < len(lines):
                i += 1; blk.append(lines[i]); d += lines[i].count('{') - lines[i].count('}')
            out += flatten('\n'.join(blk))
        else:
            out.append(l)
        i += 1
    return '\n'.join(out)
