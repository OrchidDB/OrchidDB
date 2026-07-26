#!/usr/bin/env python3
"""Re-import openCypher TCK `Given` graph setups into Ladybug tck cases.

The 933 cases under cases/cypher/ladybug/tck were imported from Kuzu's
tck test files with every setup statement dropped, so they all run on an
empty graph. This script matches each .case back to its upstream
openCypher TCK scenario (tck/features/**/*.feature), extracts the
`Given ... having executed:` Cypher CREATE setup, converts it to the
case runner's `--- graph_initializer` DSL (see
tests/cypher_case_runner/initializer.rs), and rewrites the case file.

Usage:
    python3 tests/import_tck_initializers.py <openCypher-repo-root> [--dry-run]

Only simple CREATE-only setups are converted (nodes, relationships,
scalar properties). Scenarios using MERGE/UNWIND/FOREACH/parameters,
list/map property values, or multi-label nodes are left untouched and
reported.
"""

import json
import os
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
CASES = REPO / "cases" / "cypher" / "ladybug" / "tck"

# ---------------------------------------------------------------------------
# Feature file discovery
# ---------------------------------------------------------------------------

CLAUSE_DIRS = {
    "match": "clauses/match",
    "match_where": "clauses/match-where",
    "return": "clauses/return",
    "return_orderby": "clauses/return-orderby",
    "return_skip_limit": "clauses/return-skip-limit",
    "with": "clauses/with",
    "with_orderby": "clauses/with-orderBy",
    "with_skip_limit": "clauses/with-skip-limit",
    "with_where": "clauses/with-where",
}


def feature_path_for_source(tck_root: Path, source: str):
    """Map e.g. `tck/match/match1.test` or
    `tck/expressions/aggregation/Aggregation1.test` to a .feature path."""
    parts = source.split("/")
    assert parts[0] == "tck"
    stem = parts[-1][: -len(".test")]
    if parts[1] == "expressions":
        d = tck_root / "features" / "expressions" / parts[2]
    else:
        d = tck_root / "features" / CLAUSE_DIRS[parts[1]]
    if not d.is_dir():
        return None
    want = re.sub(r"[^a-z0-9]", "", stem.lower())
    for f in sorted(d.glob("*.feature")):
        if re.sub(r"[^a-z0-9]", "", f.stem.lower()) == want:
            return f
    return None


# ---------------------------------------------------------------------------
# Gherkin (subset) parsing
# ---------------------------------------------------------------------------

class Scenario:
    def __init__(self, name, outline=False):
        self.name = name
        self.outline = outline
        self.setups = []          # list of doc-string Cypher blocks
        self.query = None
        self.has_params = False
        self.examples = []        # list of dict for outlines

    def expansions(self):
        """Yield (setup_blocks, query) per concrete instance."""
        if not self.outline:
            yield (self.setups, self.query)
            return
        for row in self.examples:
            def sub(text):
                for k, v in row.items():
                    text = text.replace("<%s>" % k, v)
                return text
            yield ([sub(s) for s in self.setups], sub(self.query or ""))


def parse_feature(path: Path):
    lines = path.read_text().splitlines()
    scenarios = []
    i = 0
    cur = None
    mode = None  # None | 'given' | 'when' | 'examples'
    pending_doc_target = None

    def read_docstring(idx):
        # lines[idx] is the `"""` opener
        buf = []
        idx += 1
        while idx < len(lines) and lines[idx].strip() != '"""':
            buf.append(lines[idx])
            idx += 1
        return "\n".join(buf), idx + 1

    while i < len(lines):
        line = lines[i].strip()
        if line.startswith("Scenario Outline:"):
            cur = Scenario(line.split(":", 1)[1].strip(), outline=True)
            scenarios.append(cur)
            mode = None
        elif line.startswith("Scenario:"):
            cur = Scenario(line.split(":", 1)[1].strip())
            scenarios.append(cur)
            mode = None
        elif cur is not None:
            if line.startswith("Given") or line.startswith("And having executed"):
                if "having executed" in line:
                    mode = "given"
                else:
                    mode = None
            elif line.startswith("And parameters are"):
                cur.has_params = True
                mode = "params"
            elif line.startswith("When executing"):
                mode = "when"
            elif line.startswith("Then") or line.startswith("And no side effects"):
                mode = None
            elif line.startswith("Examples:"):
                mode = "examples"
                header = None
                j = i + 1
                while j < len(lines):
                    t = lines[j].strip()
                    if t.startswith("|"):
                        cells = [c.strip() for c in t.strip("|").split("|")]
                        if header is None:
                            header = cells
                        else:
                            cur.examples.append(dict(zip(header, cells)))
                        j += 1
                    elif t == "" or t.startswith("#"):
                        j += 1
                    else:
                        break
                i = j
                continue
            elif line == '"""':
                doc, nxt = read_docstring(i)
                if mode == "given":
                    cur.setups.append(doc)
                elif mode == "when":
                    cur.query = doc
                i = nxt
                continue
        i += 1
    return scenarios


# ---------------------------------------------------------------------------
# Cypher CREATE-block -> initializer DSL conversion
# ---------------------------------------------------------------------------

class Unconvertible(Exception):
    pass


TOKEN_RE = re.compile(
    r"\s*(?:"
    r"(?P<str>'(?:[^'\\]|\\.)*'|\"(?:[^\"\\]|\\.)*\")"
    r"|(?P<num>-?\d+\.\d+(?:[eE][-+]?\d+)?|-?\d+[eE][-+]?\d+|-?\d+)"
    r"|(?P<word>[A-Za-z_][A-Za-z0-9_]*|`[^`]+`)"
    r"|(?P<sym><-|->|[(){}\[\],:.=<>-])"
    r")"
)


def tokenize(text):
    toks = []
    i = 0
    while i < len(text):
        if text[i].isspace():
            i += 1
            continue
        m = TOKEN_RE.match(text, i)
        if not m or m.end() == i:
            raise Unconvertible("cannot tokenize at %r" % text[i : i + 20])
        toks.append(m.group().strip())
        i = m.end()
    return toks


BANNED = {
    "MERGE", "UNWIND", "FOREACH", "WITH", "SET", "DELETE", "DETACH",
    "REMOVE", "CALL", "MATCH", "RETURN", "LOAD", "USING",
}


class SetupParser:
    """Parses one `having executed` block: CREATE clauses that share a
    variable scope. Emits initializer DSL lines."""

    def __init__(self):
        self.dsl = []
        self.aliases = {}   # cypher var -> dsl alias (per setup block)
        self.used = set()   # all dsl aliases ever emitted
        self.anon = 0

    def new_block_scope(self):
        # Each `having executed` block is a separate query: variables do
        # not carry across blocks (a bare `(a)` in a later block creates
        # a new node).
        self.aliases = {}

    def fresh(self, prefix="_n"):
        self.anon += 1
        return "%s%d" % (prefix, self.anon)

    def convert_block(self, block):
        toks = tokenize(block)
        for t in toks:
            if t.upper() in BANNED and t.upper() != "CREATE":
                raise Unconvertible("setup uses %s" % t.upper())
            if t == "=":
                raise Unconvertible("path assignment / SET in setup")
        pos = 0
        while pos < len(toks):
            if toks[pos].upper() != "CREATE":
                raise Unconvertible("expected CREATE, got %r" % toks[pos])
            pos += 1
            pos = self.parse_pattern_list(toks, pos)

    def parse_pattern_list(self, toks, pos):
        while True:
            pos = self.parse_pattern(toks, pos)
            if pos < len(toks) and toks[pos] == ",":
                pos += 1
                continue
            return pos

    def parse_pattern(self, toks, pos):
        left, pos = self.parse_node(toks, pos)
        while pos < len(toks) and toks[pos] in ("-", "<-"):
            rel, direction, pos = self.parse_rel(toks, pos)
            right, pos = self.parse_node(toks, pos)
            rtype, props = rel
            src, dst = (left, right) if direction == "out" else (right, left)
            self.dsl.append(
                "edge %s -[:%s%s]-> %s"
                % (src, rtype, self.props_text(props), dst)
            )
            left = right
        return pos

    def parse_node(self, toks, pos):
        if pos >= len(toks) or toks[pos] != "(":
            raise Unconvertible("expected ( at %r" % toks[pos : pos + 3])
        pos += 1
        var = None
        labels = []
        props = {}
        if pos < len(toks) and re.match(r"^[A-Za-z_`]", toks[pos]):
            var = toks[pos].strip("`")
            pos += 1
        while pos < len(toks) and toks[pos] == ":":
            pos += 1
            labels.append(toks[pos].strip("`"))
            pos += 1
        if pos < len(toks) and toks[pos] == "{":
            props, pos = self.parse_props(toks, pos)
        if pos >= len(toks) or toks[pos] != ")":
            raise Unconvertible("expected ) at %r" % toks[pos : pos + 3])
        pos += 1

        if var and var in self.aliases:
            if labels or props:
                raise Unconvertible("re-declaration of %s with body" % var)
            return self.aliases[var], pos
        if len(labels) > 1:
            raise Unconvertible("multi-label node")
        label = labels[0] if labels else "N"
        alias = var if var else self.fresh()
        while alias in self.used:
            alias = self.fresh()
        if var:
            self.aliases[var] = alias
        self.used.add(alias)
        self.dsl.append("node %s:%s%s" % (alias, label, self.props_text(props)))
        return alias, pos

    def parse_rel(self, toks, pos):
        # -[...]-> | <-[...]- | -[...]-(undirected: reject)
        direction = None
        if toks[pos] == "<-":
            direction = "in"
            pos += 1
        elif toks[pos] == "-":
            pos += 1
        if pos >= len(toks) or toks[pos] != "[":
            raise Unconvertible("bare -- relationship")
        pos += 1
        # optional var
        if pos < len(toks) and re.match(r"^[A-Za-z_`]", toks[pos]):
            pos += 1
        if toks[pos] != ":":
            raise Unconvertible("relationship without type")
        pos += 1
        rtype = toks[pos].strip("`")
        pos += 1
        props = {}
        if pos < len(toks) and toks[pos] == "{":
            props, pos = self.parse_props(toks, pos)
        if toks[pos] != "]":
            raise Unconvertible("expected ] in relationship")
        pos += 1
        if direction is None:
            if pos < len(toks) and toks[pos] == "->":
                direction = "out"
                pos += 1
            else:
                raise Unconvertible("undirected relationship in CREATE")
        else:
            if pos < len(toks) and toks[pos] == "-":
                pos += 1
            else:
                raise Unconvertible("malformed incoming relationship")
        return (rtype, props), direction, pos

    def parse_props(self, toks, pos):
        assert toks[pos] == "{"
        pos += 1
        props = {}
        while toks[pos] != "}":
            key = toks[pos].strip("`")
            pos += 1
            if toks[pos] != ":":
                raise Unconvertible("bad property map")
            pos += 1
            val, pos = self.parse_value(toks, pos)
            props[key] = val
            if toks[pos] == ",":
                pos += 1
        return props, pos + 1

    def parse_value(self, toks, pos):
        t = toks[pos]
        if t in ("[", "{"):
            raise Unconvertible("list/map property value")
        if t.startswith("'") or t.startswith('"'):
            inner = t[1:-1]
            # normalize to double-quoted DSL string
            inner = inner.replace('\\"', '"').replace("\\'", "'")
            inner = inner.replace("\\", "\\\\").replace('"', '\\"')
            return '"%s"' % inner, pos + 1
        if re.match(r"^-?\d", t):
            return t, pos + 1
        low = t.lower()
        if low in ("true", "false", "null"):
            return low, pos + 1
        raise Unconvertible("unsupported property value %r" % t)

    def props_text(self, props):
        if not props:
            return ""
        return " {%s}" % ", ".join("%s: %s" % (k, v) for k, v in props.items())


def convert_setups(setup_blocks):
    parser = SetupParser()
    for block in setup_blocks:
        if not block.strip():
            continue
        parser.new_block_scope()
        parser.convert_block(block)
    return parser.dsl


# ---------------------------------------------------------------------------
# Case handling
# ---------------------------------------------------------------------------

def norm_query(q):
    """Whitespace-insensitive canonical form."""
    return re.sub(r"\s+", "", q).strip()


def norm_query_loose(q):
    """Also direction-insensitive (Kuzu rewrote some undirected patterns
    as directed)."""
    return norm_query(q).replace("<", "").replace(">", "")


def template_matches(template, query):
    """True when `query` looks like an instantiation of an outline
    `template` (placeholders `<var>` treated as wildcards)."""
    q = norm_query_loose(query)
    parts = re.split(r"(<[^<>\s]+>)", template)
    pattern = "".join(
        ".*?"
        if part.startswith("<") and part.endswith(">")
        else re.escape(norm_query_loose(part))
        for part in parts
        if part
    )
    return re.fullmatch(pattern, q) is not None


def parse_case(path):
    raw = path.read_text()
    sections = {}
    order = []
    cur = None
    for line in raw.splitlines():
        if line.startswith("--- "):
            cur = line[4:].strip()
            sections[cur] = []
            order.append(cur)
        elif cur is not None:
            sections[cur].append(line)
    meta = json.loads("\n".join(sections["metadata"]).strip())
    return raw, sections, order, meta


def insert_initializer(raw, dsl_lines):
    """Insert a `--- graph_initializer` section before `--- expected`."""
    out = []
    inserted = False
    for line in raw.splitlines():
        if line.strip() == "--- expected" and not inserted:
            out.append("--- graph_initializer")
            out.extend(dsl_lines)
            inserted = True
        out.append(line)
    if not inserted:
        return None
    return "\n".join(out) + "\n"


def main():
    if len(sys.argv) < 2:
        print(__doc__)
        sys.exit(2)
    oc_root = Path(sys.argv[1])
    dry = "--dry-run" in sys.argv
    tck_root = oc_root / "tck"

    stats = {
        "total": 0, "no_feature": 0, "unmatched": 0, "params": 0,
        "no_setup_needed": 0, "unconvertible": 0, "imported": 0,
        "already": 0,
    }
    unconv_reasons = {}
    feature_cache = {}

    for path in sorted(CASES.rglob("*.case")):
        stats["total"] += 1
        raw, sections, order, meta = parse_case(path)
        if "graph_initializer" in sections:
            stats["already"] += 1
            continue
        source = meta["source"]
        fpath = feature_path_for_source(tck_root, source)
        if fpath is None:
            stats["no_feature"] += 1
            continue
        if fpath not in feature_cache:
            feature_cache[fpath] = parse_feature(fpath)
        scenarios = feature_cache[fpath]

        raw_case_query = "\n".join(sections["query"])

        # Collect candidate concrete scenario instances by query text,
        # trying progressively looser normalizations.
        candidates = []
        for normalizer in (norm_query, norm_query_loose):
            case_query = normalizer(raw_case_query)
            for si, sc in enumerate(scenarios):
                for setups, query in sc.expansions():
                    if normalizer(query) == case_query:
                        candidates.append((si, sc, setups))
            if candidates:
                break

        # Index + template fallback: Kuzu instantiated some Scenario
        # Outlines with its own example values. `ScenarioN` / `ScenarioN_K`
        # names the Nth scenario in feature-file order; validate the match
        # by turning the outline template into a regex (`<var>` -> `.+?`).
        if not candidates:
            m = re.match(r"Scenario(\d+)", meta.get("source_case", ""))
            if m and 0 <= int(m.group(1)) - 1 < len(scenarios):
                si = int(m.group(1)) - 1
                sc = scenarios[si]
                if sc.query is not None and template_matches(sc.query, raw_case_query):
                    # Prefer an expansion whose query matches exactly;
                    # otherwise accept the raw setup when it needs no
                    # substitution or when all expansions agree on it.
                    exps = list(sc.expansions())
                    exact = [
                        (si, sc, setups)
                        for setups, q in exps
                        if norm_query_loose(q) == norm_query_loose(raw_case_query)
                    ]
                    if exact:
                        candidates = exact[:1]
                    elif not any("<" in s for s in sc.setups):
                        candidates = [(si, sc, sc.setups)]
                    else:
                        setup_set = {"\n".join(s) for s, _ in exps}
                        if len(setup_set) == 1:
                            candidates = [(si, sc, exps[0][0])]

        chosen = None
        if len(candidates) == 1:
            chosen = candidates[0]
        elif len(candidates) > 1:
            # Prefer the scenario whose 1-based index matches ScenarioN.
            m = re.match(r"Scenario(\d+)", meta.get("source_case", ""))
            if m:
                want = int(m.group(1)) - 1
                idx_matches = [c for c in candidates if c[0] == want]
                if len(idx_matches) >= 1:
                    chosen = idx_matches[0]
            if chosen is None:
                # If every candidate has identical setup, ambiguity is
                # harmless.
                setups_set = {"\n".join(c[2]) for c in candidates}
                if len(setups_set) == 1:
                    chosen = candidates[0]
        if chosen is None:
            stats["unmatched"] += 1
            continue

        si, sc, setups = chosen
        if sc.has_params:
            stats["params"] += 1
            continue
        if not any(s.strip() for s in setups):
            stats["no_setup_needed"] += 1
            continue
        try:
            dsl = convert_setups(setups)
        except Unconvertible as e:
            stats["unconvertible"] += 1
            unconv_reasons[str(e).split("%")[0][:40]] = (
                unconv_reasons.get(str(e).split("%")[0][:40], 0) + 1
            )
            continue
        if not dsl:
            stats["no_setup_needed"] += 1
            continue
        new_raw = insert_initializer(raw, dsl)
        if new_raw is None:
            stats["unconvertible"] += 1
            continue
        if not dry:
            path.write_text(new_raw)
        stats["imported"] += 1

    print("TCK re-import summary:")
    for k, v in stats.items():
        print("  %-16s %d" % (k, v))
    if unconv_reasons:
        print("  unconvertible reasons:")
        for k, v in sorted(unconv_reasons.items(), key=lambda kv: -kv[1]):
            print("    %4d  %s" % (v, k))


if __name__ == "__main__":
    main()
