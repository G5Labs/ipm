"""Keep one local proof witness; preserve all type/spec definitions verbatim.

This is proof-fixture isolation, not a verifier admission. In particular it
removes unrelated `by(compute)` proof bodies, which Verus lowers even when a
different --verify-function selector is requested. Imported modules are not
edited. Both the positive neighbor and its mutant must use this same transform.
"""
import re

TOKEN = re.compile(r'//[^\n]*|/\*.*?\*/|"(?:\\.|[^"\\])*"|\b[A-Za-z_][A-Za-z_0-9]*\b|[^\s]', re.S)


def isolate(source: str, selected: str):
    tokens = [(m.group(), m.start(), m.end()) for m in TOKEN.finditer(source)
              if not m.group().startswith('//') and not m.group().startswith('/*')]
    spans = []
    depth = 0
    i = 0
    while i < len(tokens):
        value, start, end = tokens[i]
        if value == '{':
            depth += 1
        elif value == '}':
            depth -= 1
            assert depth >= 0, 'unbalanced input braces'
        if [t[0] for t in tokens[i:i+3]] == ['pub', 'proof', 'fn']:
            name = tokens[i+3][0]
            base_depth = depth
            j = i + 4
            local_depth = base_depth
            function_end = None
            while j < len(tokens):
                t = tokens[j][0]
                if t == '{':
                    local_depth += 1
                elif t == '}':
                    local_depth -= 1
                    # Contracts can contain if/else/struct braces. Only the
                    # final function-body brace is followed by another item
                    # or the enclosing verus! close (comments are tokenized out).
                    following = tokens[j+1][0] if j+1 < len(tokens) else None
                    if local_depth == base_depth and following in ('pub', 'fn', '}', '#', None):
                        function_end = tokens[j][2]
                        break
                j += 1
            assert function_end is not None, ('could not find proof body', name)
            spans.append((name, start, function_end))
            i = j
            depth = base_depth
        i += 1
    assert depth == 0, 'unbalanced input braces'
    assert sum(name == selected for name, _, _ in spans) == 1, ('not one selected proof', selected)
    result = source
    for name, start, end in reversed(spans):
        if name != selected:
            result = result[:start] + result[end:]
    remaining = re.findall(r'\bpub\s+proof\s+fn\s+(\w+)', result)
    assert remaining == [selected], remaining
    before = next(source[start:end] for name, start, end in spans if name == selected)
    assert before in result, 'selected proof changed'
    # Every removed interval is exactly a local proof function. Concatenating
    # untouched intervals is the construction, so other declarations cannot drift.
    return result, {'selected': selected, 'removed_proofs': [name for name, _, _ in spans if name != selected],
                    'selected_body_unchanged': True, 'type_spec_text_unchanged': True}
