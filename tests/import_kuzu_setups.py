#!/usr/bin/env python3
"""Re-extract setup (write) statements for "broken-import" Ladybug cases.

Usage:
    python3 tests/import_kuzu_setups.py <kuzu-repo-root> [--only SUBSTR]
        [--dry-run] [--list-broken] [--verbose]

For every `.case` file under cases/cypher/ladybug whose metadata names the
empty fixture, that has no `--- graph_initializer` / `--- setup_statements`
section, and whose expectation is non-error rows, this script:

  1. locates the upstream Kuzu test file (metadata.source) and -CASE
     (metadata.source_case),
  2. finds the statement matching the case's `--- query`,
  3. collects all preceding statements of that -CASE (with
     -DEFINE_STATEMENT_BLOCK / -INSERT_STATEMENT_BLOCK splicing and any
     file-level prelude statements),
  4. converts them into a `--- graph_initializer` section (structured DSL
     for CREATE-only setups) and/or a `--- setup_statements` section (raw
     Cypher, one statement per line), inserted between `--- query` and
     `--- expected`.

Cases whose setup needs unsupported machinery (COPY/LOAD FROM, CALL,
transactions, loops/substitutions, relationship CREATEs that reference
matched nodes, ...) are left unchanged and counted per blocker category.

Stdlib only.
"""

import json
import os
import re
import sys
from collections import Counter

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CASES_ROOT = os.path.join(REPO_ROOT, "cases", "cypher", "ladybug")

EMPTY_PREFIXES = ("csv", "parquet", "npy", "json", "binary", "tsv")


# ---------------------------------------------------------------------------
# Case file handling
# ---------------------------------------------------------------------------

def is_empty_dataset(dataset):
    d = dataset.strip().lower()
    if d == "empty":
        return True
    parts = d.split()
    return len(parts) == 2 and parts[0] in EMPTY_PREFIXES and parts[1] == "empty"


def looks_like_expected_error(line):
    lower = line.lower()
    return (
        "exception" in lower
        or lower.startswith("syntaxerror:")
        or lower.startswith("syntax error")
        or lower.startswith("error:")
    )


def parse_case_file(path):
    """Split a .case file into ordered (section_name, [lines]) pairs."""
    with open(path, encoding="utf-8") as f:
        raw = f.read()
    sections = []
    current = None
    prelude = []
    for line in raw.splitlines():
        if line.startswith("--- "):
            current = (line[4:].strip(), [])
            sections.append(current)
        elif current is None:
            prelude.append(line)
        else:
            current[1].append(line)
    return raw, prelude, sections


def section(sections, name):
    for sec_name, lines in sections:
        if sec_name == name:
            return lines
    return None


def find_broken_cases(only=None):
    broken = []
    for dirpath, _dirnames, filenames in os.walk(CASES_ROOT):
        for fn in sorted(filenames):
            if not fn.endswith(".case"):
                continue
            path = os.path.join(dirpath, fn)
            if only and only not in path:
                continue
            try:
                _raw, _prelude, sections = parse_case_file(path)
                meta_lines = section(sections, "metadata")
                if not meta_lines:
                    continue
                meta = json.loads("\n".join(meta_lines).strip())
            except Exception:
                continue
            if not is_empty_dataset(str(meta.get("dataset", ""))):
                continue
            if section(sections, "graph_initializer") is not None:
                continue
            if section(sections, "setup_statements") is not None:
                continue
            expected = section(sections, "expected") or []
            first = next((l.strip() for l in expected if l.strip()), None)
            if first is None:
                continue  # empty expectation is fine as-is
            if looks_like_expected_error(first):
                continue
            # Sources with bespoke defaults/overlays in the Rust runner
            # (default_graph_initializer / apply_default_graph_overlay):
            # adding sections here would fight the built-in fixtures.
            if meta.get("source") in (
                "recursive_join/semantic_empty.test",
                "function/uuid.test",
                "agg/hash.test",
                "function/cast.test",
            ):
                continue
            query_lines = section(sections, "query") or []
            broken.append(
                {
                    "path": path,
                    "meta": meta,
                    "query": "\n".join(query_lines).strip(),
                    "expected": [l for l in expected],
                }
            )
    return broken


# ---------------------------------------------------------------------------
# Kuzu .test file parsing
# ---------------------------------------------------------------------------

class Stmt:
    __slots__ = ("text", "result_kind", "result_rows", "flags")

    def __init__(self, text, result_kind, result_rows, flags):
        self.text = text  # joined single-line statement text
        self.result_kind = result_kind  # "ok" | "error" | int | other
        self.result_rows = result_rows  # list[str] for row results
        self.flags = flags  # set of strings, e.g. {"loop", "batch"}


DIRECTIVE_RE = re.compile(r"^-([A-Z][A-Z0-9_]*)(\s+(.*))?$")


def parse_kuzu_test(path):
    with open(path, encoding="utf-8") as f:
        lines = f.read().splitlines()

    blocks = {}
    cases = {}
    prelude = []
    current_case = None
    current_block = None
    in_loop = 0

    i = 0
    n = len(lines)
    while i < n:
        stripped = lines[i].strip()
        if current_block is not None and stripped == "]":
            current_block = None
            i += 1
            continue
        if not stripped or stripped.startswith("#") or stripped == "--":
            i += 1
            continue
        m = DIRECTIVE_RE.match(stripped)
        if not m:
            i += 1
            continue
        name, rest = m.group(1), (m.group(3) or "").strip()
        if name == "CASE":
            current_case = []
            cases.setdefault(rest, current_case)
            current_block = None
            i += 1
        elif name == "DEFINE_STATEMENT_BLOCK":
            bname = rest.rstrip("[").strip()
            blocks[bname] = []
            current_block = (bname, blocks[bname])
            i += 1
        elif name == "INSERT_STATEMENT_BLOCK":
            target = (
                current_block[1]
                if current_block is not None
                else (current_case if current_case is not None else prelude)
            )
            spliced = blocks.get(rest)
            if spliced is None:
                target.append(Stmt("__UNKNOWN_BLOCK__ " + rest, "ok", [], {"unknown_block"}))
            else:
                target.extend(spliced)
            i += 1
        elif name == "STATEMENT":
            stmt_lines = [rest]
            i += 1
            while i < n and not lines[i].strip().startswith("----"):
                stmt_lines.append(lines[i].strip())
                i += 1
            result_kind, result_rows = "ok", []
            if i < n:
                marker = lines[i].strip()[4:].strip()
                i += 1
                if marker == "ok":
                    pass
                elif marker.startswith("error"):
                    result_kind = "error"
                    while i < n:
                        s = lines[i].strip()
                        if DIRECTIVE_RE.match(s) or s == "--" or (current_block and s == "]"):
                            break
                        i += 1
                elif marker == "hash":
                    result_kind = "hash"
                    if i < n:
                        i += 1
                elif marker.isdigit():
                    count = int(marker)
                    result_kind = count
                    for _ in range(count):
                        if i < n:
                            result_rows.append(lines[i])
                            i += 1
                else:
                    result_kind = marker
            text = " ".join(p for p in (s.strip() for s in stmt_lines) if p)
            cm = re.match(r"^\[\w+\]\s+(.*)$", text)
            if cm:
                text = cm.group(1)
            flags = set()
            if in_loop:
                flags.add("loop")
            stmt = Stmt(text, result_kind, result_rows, flags)
            if current_block is not None:
                current_block[1].append(stmt)
            elif current_case is not None:
                current_case.append(stmt)
            else:
                prelude.append(stmt)
        elif name == "LOOP":
            in_loop += 1
            i += 1
        elif name == "ENDLOOP":
            in_loop = max(0, in_loop - 1)
            i += 1
        elif name in ("BATCH_STATEMENTS", "IMPORT_DATABASE", "CREATE_DATASET_SCHEMA",
                      "INSERT_DATASET_BY_ROW", "BEGIN_CONCURRENT_EXECUTION"):
            target = current_case if current_case is not None else prelude
            target.append(Stmt("__DIRECTIVE__ " + name, "ok", [], {"directive"}))
            i += 1
        else:
            i += 1
    return prelude, cases


# ---------------------------------------------------------------------------
# Statement matching
# ---------------------------------------------------------------------------

def norm_query(text):
    return re.sub(r"\s+", " ", text.strip().rstrip(";").strip())


def match_statement(stmts, case):
    """Find the index of the statement matching the case's query.

    Prefer the 1-based ordinal encoded in the file name prefix (NNNN_)
    when its text matches; otherwise match by normalized text, using the
    expected rows to disambiguate duplicates."""
    want = norm_query(case["query"])
    candidates = [i for i, s in enumerate(stmts) if norm_query(s.text) == want]
    if not candidates:
        return None
    if len(candidates) == 1:
        return candidates[0]
    # disambiguate via filename ordinal
    fn = os.path.basename(case["path"])
    m = re.match(r"^(\d+)_", fn)
    if m:
        ordinal = int(m.group(1)) - 1
        if ordinal in candidates:
            return ordinal
    # disambiguate via expected rows
    want_rows = [l for l in case["expected"] if l.strip()]
    for i in candidates:
        got = [l for l in stmts[i].result_rows if l.strip()]
        if got == want_rows:
            return i
    return candidates[0]


# ---------------------------------------------------------------------------
# Setup conversion
# ---------------------------------------------------------------------------

DDL_RE = re.compile(
    r"^\s*(create\s+(node\s+table|rel\s+table|table|sequence|rdfgraph|type|macro)"
    r"|alter\s|drop\s|comment\s+on\s)",
    re.IGNORECASE,
)
CALL_CONFIG_RE = re.compile(r"^\s*call\s+[\w.]+\s*=", re.IGNORECASE)
BLOCKER_PATTERNS = [
    ("copy", re.compile(r"^\s*copy\s", re.IGNORECASE)),
    ("load_from", re.compile(r"\bload\s+(with\s+headers\s.*)?from\b", re.IGNORECASE)),
    ("import_export", re.compile(r"^\s*(import|export)\s+database\b", re.IGNORECASE)),
    ("call", re.compile(r"^\s*call\s", re.IGNORECASE)),
    ("attach", re.compile(r"^\s*(attach|detach|use)\s", re.IGNORECASE)),
    ("directive", re.compile(r"^__DIRECTIVE__|^__UNKNOWN_BLOCK__")),
]
WRITE_KEYWORD_RE = re.compile(r"\b(create|set|delete|merge|insert)\b", re.IGNORECASE)


def classify_setup(stmts):
    """Split preceding statements into (setup_writes, blocker) where
    blocker is None or a category string.

    Transaction control is folded away: a committed transaction is
    equivalent to running its statements directly, and a rolled-back
    transaction is equivalent to not running them at all. A transaction
    still open when the case query runs sees its own writes, so a
    dangling BEGIN keeps its statements too."""
    setup = []
    txn_buffer = None  # list while inside BEGIN..COMMIT/ROLLBACK
    for s in stmts:
        text = s.text.strip()
        if not text:
            continue
        if re.match(r"^begin\b", text, re.IGNORECASE):
            if txn_buffer is not None:
                setup.extend(txn_buffer)  # nested begin: flush conservatively
            txn_buffer = []
            continue
        if re.match(r"^commit\b", text, re.IGNORECASE):
            if txn_buffer is not None:
                setup.extend(txn_buffer)
                txn_buffer = None
            continue
        if re.match(r"^rollback\b", text, re.IGNORECASE):
            txn_buffer = None  # discard writes since BEGIN
            continue
        if "${" in text:
            return None, "substitution"
        if "loop" in s.flags:
            return None, "loop"
        if DDL_RE.match(text):
            continue  # schemaless engine: DDL dropped
        if CALL_CONFIG_RE.match(text):
            continue  # config knob, droppable
        blocked = None
        for cat, pat in BLOCKER_PATTERNS:
            if pat.search(text):
                blocked = cat
                break
        if blocked:
            return None, blocked
        if not WRITE_KEYWORD_RE.search(text):
            continue  # pure read (MATCH..RETURN etc.) — drop
        # A statement with RETURN but no write clause is a read too
        if re.match(r"^\s*(match|with|unwind|return|explain|profile)\b", text, re.IGNORECASE) \
                and not re.search(r"\b(create|set|delete|merge)\b", text, re.IGNORECASE):
            continue
        target = txn_buffer if txn_buffer is not None else setup
        target.append(text.rstrip(";").strip())
    if txn_buffer is not None:
        setup.extend(txn_buffer)  # query runs inside the open transaction
    return setup, None


# --- Cypher CREATE pattern parsing (for graph_initializer) -----------------

SIMPLE_STRING_RE = re.compile(r"""^(?:'(?:[^'\\]|\\.)*'|"(?:[^"\\]|\\.)*")$""")
INT_RE = re.compile(r"^[+-]?\d+$")
FLOAT_RE = re.compile(r"^[+-]?(\d+\.\d*|\.\d+|\d+(\.\d*)?[eE][+-]?\d+)$")


def parse_literal(text):
    """Return canonical initializer literal or None if unsupported."""
    t = text.strip()
    if t.lower() == "null":
        return "null"
    if t.lower() in ("true", "false"):
        return t.lower()
    if INT_RE.match(t):
        return t
    if FLOAT_RE.match(t):
        return t
    if SIMPLE_STRING_RE.match(t):
        inner = t[1:-1]
        # unescape then re-escape for double quotes
        out = []
        it = iter(range(len(inner)))
        i = 0
        buf = []
        while i < len(inner):
            c = inner[i]
            if c == "\\" and i + 1 < len(inner):
                nxt = inner[i + 1]
                mapping = {"n": "\n", "t": "\t", "r": "\r", "'": "'", '"': '"', "\\": "\\"}
                buf.append(mapping.get(nxt, nxt))
                i += 2
            else:
                buf.append(c)
                i += 1
        s = "".join(buf)
        s = s.replace("\\", "\\\\").replace('"', '\\"').replace("\n", "\\n") \
             .replace("\t", "\\t").replace("\r", "\\r")
        return '"' + s + '"'
    return None


def split_top_commas(text):
    parts, depth, start = [], 0, 0
    in_s = in_d = False
    i = 0
    while i < len(text):
        c = text[i]
        if in_s:
            if c == "\\":
                i += 2
                continue
            if c == "'":
                in_s = False
        elif in_d:
            if c == "\\":
                i += 2
                continue
            if c == '"':
                in_d = False
        elif c == "'":
            in_s = True
        elif c == '"':
            in_d = True
        elif c in "([{":
            depth += 1
        elif c in ")]}":
            depth -= 1
        elif c == "," and depth == 0:
            parts.append(text[start:i])
            start = i + 1
        i += 1
    parts.append(text[start:])
    return parts


def parse_props(body):
    """Parse `{k: v, ...}` inner body -> list[(key, literal)] or None."""
    out = []
    for entry in split_top_commas(body):
        entry = entry.strip()
        if not entry:
            continue
        m = re.match(r"^`?([A-Za-z_][\w]*)`?\s*:\s*(.+)$", entry, re.DOTALL)
        if not m:
            return None
        lit = parse_literal(m.group(2))
        if lit is None:
            return None
        out.append((m.group(1), lit))
    return out


NODE_RE = re.compile(
    r"^\(\s*(?P<alias>[A-Za-z_]\w*)?\s*(?::\s*(?P<label>`?[A-Za-z_]\w*`?))?\s*"
    r"(?P<props>\{.*\})?\s*\)$",
    re.DOTALL,
)


def tokenize_pattern_element(elem):
    """Split one pattern element like (a)-[:R {x:1}]->(b)<-[:S]-(c) into
    a list of node-strings and rel dicts. Returns None on parse failure."""
    elem = elem.strip()
    items = []
    i = 0
    n = len(elem)
    while i < n:
        if elem[i] != "(":
            return None
        depth = 0
        j = i
        in_s = in_d = False
        while j < n:
            c = elem[j]
            if in_s:
                if c == "\\":
                    j += 2
                    continue
                if c == "'":
                    in_s = False
            elif in_d:
                if c == "\\":
                    j += 2
                    continue
                if c == '"':
                    in_d = False
            elif c == "'":
                in_s = True
            elif c == '"':
                in_d = True
            elif c == "(":
                depth += 1
            elif c == ")":
                depth -= 1
                if depth == 0:
                    break
            j += 1
        if j >= n:
            return None
        items.append(("node", elem[i : j + 1]))
        i = j + 1
        while i < n and elem[i].isspace():
            i += 1
        if i >= n:
            break
        # relationship: -[...]-> or <-[...]- or -[...]-
        m = re.match(r"^(<-|-)\s*\[(.*?)\]\s*(->|-)", elem[i:], re.DOTALL)
        if not m:
            return None
        left, body, right = m.group(1), m.group(2), m.group(3)
        if left == "<-" and right == "->":
            return None
        if left == "-" and right == "-":
            return None  # undirected create — invalid anyway
        direction = "fwd" if right == "->" else "back"
        items.append(("rel", body.strip(), direction))
        i += m.end()
        while i < n and elem[i].isspace():
            i += 1
    return items


class InitBuilder:
    def __init__(self):
        self.node_lines = []  # (alias, label, props list)
        self.edge_lines = []
        self.alias_map = {}  # global alias -> emitted alias

    def add_create_statement(self, text, stmt_idx):
        """Parse `CREATE <patterns>` into node/edge decls. Returns None on
        success or a blocker category string."""
        m = re.match(r"^\s*create\s+(.*)$", text, re.IGNORECASE | re.DOTALL)
        if not m:
            return "not_create"
        body = m.group(1)
        local = {}  # alias in this statement -> emitted alias
        pending_edges = []
        new_nodes = []
        anon = [0]

        def emit_alias(raw_alias):
            if raw_alias:
                key = raw_alias
            else:
                anon[0] += 1
                key = "__anon%d" % anon[0]
            if key in local:
                return local[key], False
            emitted = "s%d_%s" % (stmt_idx, key.lstrip("_"))
            local[key] = emitted
            return emitted, True

        for elem in split_top_commas(body):
            items = tokenize_pattern_element(elem)
            if items is None:
                return "unparseable_create"
            prev_alias = None
            prev_new = False
            k = 0
            while k < len(items):
                kind = items[k][0]
                if kind == "node":
                    nm = NODE_RE.match(items[k][1].strip())
                    if not nm:
                        return "unparseable_create"
                    raw_alias = nm.group("alias")
                    label = nm.group("label")
                    props_text = nm.group("props")
                    emitted, fresh = emit_alias(raw_alias)
                    if fresh:
                        if not label:
                            return "unlabeled_node"
                        label = label.strip("`")
                        props = []
                        if props_text:
                            props = parse_props(props_text[1:-1])
                            if props is None:
                                return "complex_props"
                        self.node_lines.append((emitted, label, props))
                    else:
                        if props_text:
                            return "unparseable_create"
                    cur = emitted
                    if k > 0:
                        rel = items[k - 1]
                        body_ = rel[1]
                        direction = rel[2]
                        rm = re.match(
                            r"^([A-Za-z_]\w*)?\s*:\s*(`?[A-Za-z_]\w*`?)\s*(\{.*\})?\s*$",
                            body_,
                            re.DOTALL,
                        )
                        if not rm:
                            return "unparseable_rel"
                        rel_type = rm.group(2).strip("`")
                        rprops = []
                        if rm.group(3):
                            rprops = parse_props(rm.group(3)[1:-1])
                            if rprops is None:
                                return "complex_props"
                        if direction == "fwd":
                            pending_edges.append((prev_alias, rel_type, rprops, cur))
                        else:
                            pending_edges.append((cur, rel_type, rprops, prev_alias))
                    prev_alias = cur
                    k += 2 if k + 1 < len(items) else 1
                else:
                    return "unparseable_create"
        # cross-statement references: an alias used but never freshly
        # created in this statement means it referenced a MATCH variable.
        self.edge_lines.extend(pending_edges)
        return None

    def render(self):
        out = []
        for alias, label, props in self.node_lines:
            if props:
                body = ", ".join("%s: %s" % (k, v) for k, v in props)
                out.append("node %s:%s {%s}" % (alias, label, body))
            else:
                out.append("node %s:%s" % (alias, label))
        for src, rel_type, props, dst in self.edge_lines:
            if props:
                body = ", ".join("%s: %s" % (k, v) for k, v in props)
                out.append("edge %s -[:%s {%s}]-> %s" % (src, rel_type, body, dst))
            else:
                out.append("edge %s -[:%s]-> %s" % (src, rel_type, dst))
        return out


def query_label_case_map(query):
    """label lowercased -> casing used in the query (labels and props)."""
    out = {}
    for m in re.finditer(r":\s*`?([A-Za-z_]\w*)`?", query):
        out.setdefault(m.group(1).lower(), m.group(1))
    return out


def query_prop_case_map(query):
    out = {}
    for m in re.finditer(r"\.\s*`?([A-Za-z_]\w*)`?", query):
        out.setdefault(m.group(1).lower(), m.group(1))
    for m in re.finditer(r"[{,]\s*`?([A-Za-z_]\w*)`?\s*:", query):
        out.setdefault(m.group(1).lower(), m.group(1))
    return out


HAS_REL_PATTERN_RE = re.compile(r"\)\s*(<-|-)\s*\[")


def literal_eq(a, b):
    """Compare two canonical initializer literals for equality."""
    if a == b:
        return True
    try:
        return float(a) == float(b)
    except (TypeError, ValueError):
        return False


def parse_match_create(text):
    """Parse `MATCH (a:L {..}), (b:L) [WHERE a.p = lit AND ...]
    CREATE (a)-[r:T {props}]->(b)`.

    Returns (constraints, edges) where constraints maps alias ->
    (label, [(prop, canonical_literal)]) and edges is a list of
    (src_alias, rel_type, props, dst_alias); or None if unsupported."""
    m = re.match(r"^\s*match\s+(.*?)\s+create\s+(.*)$", text,
                 re.IGNORECASE | re.DOTALL)
    if not m:
        return None
    match_part, create_part = m.group(1), m.group(2).rstrip(";").strip()
    # additional MATCH keywords act like commas
    match_part = re.sub(r"\bmatch\b", ",", match_part, flags=re.IGNORECASE)
    where_split = re.split(r"\bwhere\b", match_part, flags=re.IGNORECASE)
    if len(where_split) > 2:
        return None
    patterns_text = where_split[0]
    where_text = where_split[1] if len(where_split) == 2 else None

    constraints = {}
    for elem in split_top_commas(patterns_text):
        elem = elem.strip().strip(",").strip()
        if not elem:
            continue
        nm = NODE_RE.match(elem)
        if not nm or not nm.group("alias") or not nm.group("label"):
            return None
        alias = nm.group("alias")
        label = nm.group("label").strip("`")
        conds = []
        if nm.group("props"):
            props = parse_props(nm.group("props")[1:-1])
            if props is None:
                return None
            conds = list(props)
        constraints[alias] = (label, conds)

    if where_text:
        for cond in re.split(r"\band\b", where_text, flags=re.IGNORECASE):
            cm = re.match(r"^\s*(\w+)\s*\.\s*`?(\w+)`?\s*=\s*(.+?)\s*$", cond)
            if not cm:
                return None
            alias, prop, rhs = cm.group(1), cm.group(2), cm.group(3)
            if alias not in constraints:
                return None
            lit = parse_literal(rhs)
            if lit is None:
                return None
            constraints[alias][1].append((prop, lit))

    items = tokenize_pattern_element(create_part)
    if not items:
        return None
    # expect alternating node/rel starting and ending with a bare alias node
    edges = []
    prev_alias = None
    for k, item in enumerate(items):
        if item[0] == "node":
            nm = NODE_RE.match(item[1].strip())
            if not nm or not nm.group("alias") or nm.group("label") or nm.group("props"):
                return None
            alias = nm.group("alias")
            if alias not in constraints:
                return None
            if k > 0:
                rel = items[k - 1]
                rm = re.match(
                    r"^([A-Za-z_]\w*)?\s*:\s*(`?[A-Za-z_]\w*`?)\s*(\{.*\})?\s*$",
                    rel[1],
                    re.DOTALL,
                )
                if not rm:
                    return None
                rel_type = rm.group(2).strip("`")
                rprops = []
                if rm.group(3):
                    rprops = parse_props(rm.group(3)[1:-1])
                    if rprops is None:
                        return None
                if rel[2] == "fwd":
                    edges.append((prev_alias, rel_type, rprops, alias))
                else:
                    edges.append((alias, rel_type, rprops, prev_alias))
            prev_alias = alias
        # rel items are consumed when the following node is handled
    if not edges:
        return None
    return constraints, edges


def resolve_match_create(builder, parsed):
    """Resolve a parsed MATCH..CREATE against the nodes declared so far.
    Returns None on success or a blocker category."""
    constraints, edges = parsed
    resolved = {}
    for alias, (label, conds) in constraints.items():
        candidates = []
        for emitted, lbl, props in builder.node_lines:
            if lbl.lower() != label.lower():
                continue
            ok = True
            for ck, cv in conds:
                hit = False
                for pk, pv in props:
                    if pk.lower() == ck.lower() and literal_eq(pv, cv):
                        hit = True
                        break
                if not hit:
                    ok = False
                    break
            if ok:
                candidates.append(emitted)
        if len(candidates) != 1:
            return "match_create_ambiguous"
        resolved[alias] = candidates[0]
    for src, rel_type, props, dst in edges:
        builder.edge_lines.append((resolved[src], rel_type, props, resolved[dst]))
    return None


def convert_case(case, stmts, idx, notes):
    """Return (init_lines or None, setup_lines or None, blocker or None)."""
    preceding = stmts[:idx]
    setup, blocker = classify_setup(preceding)
    if blocker:
        return None, None, blocker
    if not setup:
        return None, None, "no_setup_writes"

    # --- Sequential method-A attempt: CREATEs (incl. resolvable
    # MATCH..CREATE rels) into the initializer, trailing SET/DELETEs into
    # setup_statements.
    builder = InitBuilder()
    mutates = []
    blocker = None
    for i, text in enumerate(setup):
        head_m = re.match(r"^\s*(\w+)", text)
        head = head_m.group(1).lower() if head_m else ""
        if head == "merge" or re.search(r"\bmerge\b", text, re.IGNORECASE):
            blocker = "merge"
            break
        if head == "create":
            if mutates:
                blocker = "mutate_before_create"
                break
            err = builder.add_create_statement(text, i)
            if err:
                blocker = err
                break
        elif re.search(r"\bcreate\b", text, re.IGNORECASE):
            if mutates:
                blocker = "mutate_before_create"
                break
            parsed = parse_match_create(text)
            if parsed is None:
                blocker = "match_create"
                break
            err = resolve_match_create(builder, parsed)
            if err:
                blocker = err
                break
        else:
            mutates.append(text)

    if blocker is None:
        if not builder.node_lines:
            # nothing but SET/DELETE against an empty graph: express as
            # raw setup statements (order preserved).
            return None, fix_setup_label_case(mutates, case), None
        init_lines = fix_label_case(builder.render(), case)
        setup_lines = (
            fix_setup_label_case(mutates, case, init_lines) if mutates else None
        )
        return init_lines, setup_lines, None

    # --- Method-B fallback: works when no statement needs a relationship
    # pattern or MERGE — node-only CREATEs (any literal exprs) plus
    # SET/DELETE in original order.
    fallback_ok = True
    for text in setup:
        head_m = re.match(r"^\s*(\w+)", text)
        head = head_m.group(1).lower() if head_m else ""
        if re.search(r"\bmerge\b", text, re.IGNORECASE):
            fallback_ok = False
            break
        if re.search(r"\bcreate\b", text, re.IGNORECASE):
            if head != "create" or HAS_REL_PATTERN_RE.search(text):
                fallback_ok = False
                break
    if fallback_ok:
        return None, fix_setup_label_case(setup, case), None
    return None, None, blocker


def fix_label_case(init_lines, case):
    """Align label/property casing with the query (Kuzu is case-insensitive,
    the engine is not)."""
    lmap = query_label_case_map(case["query"])
    pmap = query_prop_case_map(case["query"])
    out = []
    for line in init_lines:
        def fix_label(m):
            word = m.group(1)
            return ":" + lmap.get(word.lower(), word)
        line = re.sub(r":([A-Za-z_]\w*)", fix_label, line, count=1)
        if line.startswith("edge "):
            pass  # rel type: fix via lmap too (first sub already did it)
        def fix_prop(m):
            word = m.group(2)
            return m.group(1) + pmap.get(word.lower(), word) + ":"
        line = re.sub(r"([{,]\s*)([A-Za-z_]\w*):", fix_prop, line)
        out.append(line)
    return out


def fix_setup_label_case(setup_lines, case, init_lines=None):
    """Align label and property-name casing in raw setup Cypher with the
    query / initializer (Kuzu resolves names case-insensitively, the
    engine does not)."""
    lmap = query_label_case_map(case["query"])
    pmap = query_prop_case_map(case["query"])
    # properties already materialised by the initializer win over raw text
    for line in init_lines or []:
        for m in re.finditer(r"[{,]\s*([A-Za-z_]\w*):", line):
            pmap[m.group(1).lower()] = m.group(1)
    out = []
    for line in setup_lines:
        def fix_label(m):
            word = m.group(1)
            return ":" + lmap.get(word.lower(), word)

        def fix_prop_key(m):
            word = m.group(2)
            return m.group(1) + pmap.get(word.lower(), word) + ":"

        def fix_prop_ref(m):
            word = m.group(1)
            return "." + pmap.get(word.lower(), word)

        line = re.sub(r":\s*`?([A-Za-z_]\w*)`?(?!\s*:)", fix_label, line)
        line = re.sub(r"([{,]\s*)`?([A-Za-z_]\w*)`?\s*:", fix_prop_key, line)
        line = re.sub(r"\.\s*`?([A-Za-z_]\w*)`?", fix_prop_ref, line)
        out.append(line)
    return out


# ---------------------------------------------------------------------------
# File rewriting
# ---------------------------------------------------------------------------

def rewrite_case_file(case, init_lines, setup_lines, dry_run=False):
    path = case["path"]
    with open(path, encoding="utf-8") as f:
        raw = f.read()
    lines = raw.splitlines(keepends=True)
    insert_at = None
    for i, line in enumerate(lines):
        if line.rstrip("\n") == "--- expected":
            insert_at = i
            break
    if insert_at is None:
        return False
    meta = case["meta"]
    comment = "# re-extracted from kuzu test/test_files/%s case %s" % (
        meta.get("source", "?"),
        meta.get("source_case", "?"),
    )
    block = []
    if init_lines:
        block.append("--- graph_initializer\n")
        block.append(comment + "\n")
        block.extend(l + "\n" for l in init_lines)
    if setup_lines:
        block.append("--- setup_statements\n")
        block.append(comment + "\n")
        block.extend(l + "\n" for l in setup_lines)
    new_lines = lines[:insert_at] + block + lines[insert_at:]
    if not dry_run:
        with open(path, "w", encoding="utf-8") as f:
            f.write("".join(new_lines))
    return True


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main():
    args = sys.argv[1:]
    if not args:
        print("usage: import_kuzu_setups.py <kuzu-root> [--only SUBSTR] "
              "[--dry-run] [--list-broken] [--verbose]")
        return 2
    kuzu_root = args[0]
    only = None
    dry_run = "--dry-run" in args
    list_broken = "--list-broken" in args
    verbose = "--verbose" in args
    if "--only" in args:
        only = args[args.index("--only") + 1]

    test_root = os.path.join(kuzu_root, "test", "test_files")
    if not os.path.isdir(test_root):
        print("kuzu test_files not found at", test_root)
        return 2

    broken = find_broken_cases(only)
    if list_broken:
        for c in broken:
            print(c["path"])
        print("total:", len(broken))
        return 0

    stats = Counter()
    blockers = Counter()
    examples = {}
    parsed_tests = {}

    for case in broken:
        meta = case["meta"]
        source = meta.get("source", "")
        src_path = os.path.join(test_root, source)
        if not os.path.isfile(src_path):
            stats["no_upstream_file"] += 1
            blockers["no_upstream_file"] += 1
            examples.setdefault("no_upstream_file", case["path"])
            continue
        if src_path not in parsed_tests:
            try:
                parsed_tests[src_path] = parse_kuzu_test(src_path)
            except Exception as e:
                parsed_tests[src_path] = None
                if verbose:
                    print("parse failure", src_path, e)
        parsed = parsed_tests[src_path]
        if parsed is None:
            stats["upstream_parse_failure"] += 1
            blockers["upstream_parse_failure"] += 1
            continue
        prelude, cases = parsed
        stmts = cases.get(meta.get("source_case", ""))
        if stmts is None:
            stats["no_upstream_case"] += 1
            blockers["no_upstream_case"] += 1
            examples.setdefault("no_upstream_case", case["path"])
            continue
        all_stmts = prelude + stmts
        idx = match_statement(all_stmts, case)
        if idx is None:
            stats["no_query_match"] += 1
            blockers["no_query_match"] += 1
            examples.setdefault("no_query_match", case["path"])
            continue
        init_lines, setup_lines, blocker = convert_case(
            case, all_stmts, idx, None
        )
        if blocker:
            stats["blocked"] += 1
            blockers[blocker] += 1
            examples.setdefault(blocker, case["path"])
            if verbose:
                print("BLOCKED", blocker, case["path"])
            continue
        if rewrite_case_file(case, init_lines, setup_lines, dry_run):
            if init_lines and setup_lines:
                stats["converted_both"] += 1
            elif init_lines:
                stats["converted_init"] += 1
            else:
                stats["converted_setup"] += 1
            if verbose:
                print("CONVERTED", case["path"])
        else:
            stats["rewrite_failed"] += 1

    print("broken cases considered:", len(broken))
    for k, v in sorted(stats.items()):
        print("  %s: %d" % (k, v))
    print("blocker categories:")
    for k, v in blockers.most_common():
        print("  %-24s %4d  e.g. %s" % (k, v, examples.get(k, "")))
    return 0


if __name__ == "__main__":
    sys.exit(main())
