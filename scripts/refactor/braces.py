"""Shared brace counter: `{` minus `}`, ignoring // comments and string/char
literals. Naive counting made `"{}"` in a format string and `'{'` char literals
shift the depth, which hid top-level items from the visibility widener and made
whole segments look unbalanced."""


def brace_delta(line):
    d, i, n = 0, 0, len(line)
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
            if j < n and j - i <= 3:          # char literal, not a lifetime
                i = j
        elif c == '{':
            d += 1
        elif c == '}':
            d -= 1
        i += 1
    return d
