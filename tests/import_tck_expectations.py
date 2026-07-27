#!/usr/bin/env python3
"""Regenerate poisoned `--- expected` sections of Ladybug tck cases from
the upstream openCypher TCK, in the TCK's own neutral result syntax.

Two importer artifacts are repaired:

  A. Cases whose expected rows contain Kuzu-internal node renders such as
     `{_ID: 0:0, _LABEL: End, ID: 0, num: 42}` (detected via `_ID:`).
  B. Cases marked `"ordered":true` with an ORDER BY query whose stored
     expected rows were lexicographically re-sorted at import time: the
     stored row sequence differs from the upstream sequence but equals it
     as a multiset.

For each affected case the upstream scenario is located with the same
matching logic as tests/import_tck_initializers.py, the scenario's result
table is copied verbatim (header dropped, cells joined by `|`), the
metadata `ordered` flag is set from the Then step ("in any order" ->
false, "in order" -> true), and an `expected_provenance` key is added.
Scenarios expecting errors are skipped. Category-A cases lacking a
`--- graph_initializer` also get one via the initializer converter.

Usage:
    python3 tests/import_tck_expectations.py <openCypher-repo-root> [--dry-run]
"""

import json
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import import_tck_initializers as base  # noqa: E402

REPO = Path(__file__).resolve().parent.parent
CASES = REPO / "cases" / "cypher" / "ladybug" / "tck"

# ---------------------------------------------------------------------------
# Gherkin parsing (extends the initializer importer's subset with the
# `Then` result step and its table)
# ---------------------------------------------------------------------------

THEN_KINDS = [
    ("the result should be, in any order", "rows_any"),
    ("the result should be, in order (ignoring element order for lists)", "rows_ordered_lists"),
    ("the result should be, in order", "rows_ordered"),
    ("the result should be (ignoring element order for lists)", "rows_lists_any"),
    ("the result should be empty", "empty"),
]


def classify_then(line):
    body = line[len("Then"):].strip()
    for prefix, kind in THEN_KINDS:
        if body.rstrip(":").strip() == prefix or body.startswith(prefix):
            return kind
    if re.match(r"^an? \S+ should be raised", body):
        return "error"
    return "unknown"


class Scenario:
    def __init__(self, name, outline=False):
        self.name = name
        self.outline = outline
        self.setups = []
        self.query = None
        self.has_params = False
        self.examples = []
        self.then_kind = None      # first result/error Then step kind
        self.table = []            # rows (list of cell lists), incl. header

    def expansions(self):
        """Yield (setups, query, table) per concrete instance."""
        if not self.outline:
            yield (self.setups, self.query, self.table)
            return
        for row in self.examples:
            def sub(text):
                for k, v in row.items():
                    text = text.replace("<%s>" % k, v)
                return text
            yield (
                [sub(s) for s in self.setups],
                sub(self.query or ""),
                [[sub(c) for c in r] for r in self.table],
            )


def split_table_row(line):
    """Split a gherkin `| a | b |` row into cells, honouring `\\|` escapes."""
    body = line.strip()
    assert body.startswith("|") and body.endswith("|")
    body = body[1:-1]
    cells = []
    cur = []
    i = 0
    while i < len(body):
        c = body[i]
        if c == "\\" and i + 1 < len(body) and body[i + 1] in ("|", "\\"):
            cur.append(body[i + 1])
            i += 2
        elif c == "|":
            cells.append("".join(cur).strip())
            cur = []
            i += 1
        else:
            cur.append(c)
            i += 1
    cells.append("".join(cur).strip())
    return cells


def parse_feature(path):
    lines = path.read_text().splitlines()
    scenarios = []
    i = 0
    cur = None
    mode = None

    def read_docstring(idx):
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
                mode = "given" if "having executed" in line else None
            elif line.startswith("And parameters are"):
                cur.has_params = True
                mode = "params"
            elif line.startswith("When executing"):
                mode = "when"
            elif line.startswith("Then"):
                kind = classify_then(line)
                if cur.then_kind is None and kind != "unknown":
                    cur.then_kind = kind
                    mode = "then_table" if kind.startswith("rows") else None
                else:
                    mode = None
            elif line.startswith("And the side effects") or line.startswith("And no side effects"):
                mode = None
            elif line.startswith("Examples:"):
                mode = "examples"
                header = None
                j = i + 1
                while j < len(lines):
                    t = lines[j].strip()
                    if t.startswith("|"):
                        cells = split_table_row(t)
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
            elif line.startswith("|") and mode == "then_table":
                cur.table.append(split_table_row(line))
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
# Initializer conversion: extends the base converter to resolve property
# references to previously created nodes (e.g. `CREATE (a {id: 0}),
# (:B {num: a.id})`).
# ---------------------------------------------------------------------------

class SetupParser(base.SetupParser):
    def __init__(self):
        super().__init__()
        self.node_props = {}  # dsl alias -> {prop: literal}

    def parse_node(self, toks, pos):
        self._last_props = None
        before = len(self.dsl)
        alias, pos = super().parse_node(toks, pos)
        if len(self.dsl) > before:
            # Capture the node's literal props (recorded by parse_props
            # during the super() call) for later `var.prop` resolution.
            self.node_props[alias] = dict(self._last_props or {})
        return alias, pos

    def parse_props(self, toks, pos):
        props, pos = super().parse_props(toks, pos)
        self._last_props = props
        return props, pos

    _last_props = None

    def parse_value(self, toks, pos):
        t = toks[pos]
        if (
            re.match(r"^[A-Za-z_`]", t)
            and t.lower() not in ("true", "false", "null")
            and pos + 2 < len(toks)
            and toks[pos + 1] == "."
        ):
            var = t.strip("`")
            prop = toks[pos + 2].strip("`")
            alias = self.aliases.get(var)
            val = self.node_props.get(alias, {}).get(prop)
            if val is None:
                raise base.Unconvertible(
                    "unresolvable property reference %s.%s" % (var, prop))
            return val, pos + 3
        return super().parse_value(toks, pos)


def convert_setups(setup_blocks):
    parser = SetupParser()
    for block in setup_blocks:
        if not block.strip():
            continue
        parser.new_block_scope()
        parser.convert_block(block)
    return parser.dsl


# ---------------------------------------------------------------------------
# Scenario matching (mirrors import_tck_initializers.main)
# ---------------------------------------------------------------------------

def match_scenario(scenarios, meta, raw_case_query):
    candidates = []
    for normalizer in (base.norm_query, base.norm_query_loose):
        case_query = normalizer(raw_case_query)
        for si, sc in enumerate(scenarios):
            for setups, query, table in sc.expansions():
                if normalizer(query) == case_query:
                    candidates.append((si, sc, setups, table))
        if candidates:
            break

    if not candidates:
        m = re.match(r"Scenario(\d+)", meta.get("source_case", ""))
        if m and 0 <= int(m.group(1)) - 1 < len(scenarios):
            si = int(m.group(1)) - 1
            sc = scenarios[si]
            if sc.query is not None and base.template_matches(sc.query, raw_case_query):
                exps = list(sc.expansions())
                exact = [
                    (si, sc, setups, table)
                    for setups, q, table in exps
                    if base.norm_query_loose(q) == base.norm_query_loose(raw_case_query)
                ]
                if exact:
                    candidates = exact[:1]
                elif len(exps) == 1:
                    candidates = [(si, sc, exps[0][0], exps[0][2])]
                else:
                    tset = {json.dumps(t) for _, _, t in exps}
                    sset = {"\n".join(s) for s, _, _ in exps}
                    if len(tset) == 1 and len(sset) == 1:
                        candidates = [(si, sc, exps[0][0], exps[0][2])]

    if len(candidates) == 1:
        return candidates[0], None
    if not candidates:
        return None, "unmatched"
    # Prefer scenario whose 1-based index matches ScenarioN.
    m = re.match(r"Scenario(\d+)", meta.get("source_case", ""))
    if m:
        want = int(m.group(1)) - 1
        idx_matches = [c for c in candidates if c[0] == want]
        if idx_matches:
            uniq = {json.dumps(c[3]) for c in idx_matches}
            if len(uniq) == 1:
                return idx_matches[0], None
            return None, "ambiguous"
    uniq = {json.dumps(c[3]) for c in candidates}
    kinds = {c[1].then_kind for c in candidates}
    if len(uniq) == 1 and len(kinds) == 1:
        return candidates[0], None
    return None, "ambiguous"


# ---------------------------------------------------------------------------
# Case rewriting
# ---------------------------------------------------------------------------

def rows_to_lines(table):
    """Drop the header row; join each data row's cells with `|`."""
    return ["|".join(r) for r in table[1:]]


def rebuild(sections, order):
    out = []
    for name in order:
        out.append("--- %s" % name)
        out.extend(sections[name])
    while out and out[-1] == "":
        out.pop()
    return "\n".join(out) + "\n"


def main():
    if len(sys.argv) < 2:
        print(__doc__)
        sys.exit(2)
    oc_root = Path(sys.argv[1]).resolve()
    dry = "--dry-run" in sys.argv
    tck_root = oc_root / "tck"

    stats = {
        "a_total": 0, "a_rewritten": 0, "b_rewritten": 0,
        "init_added": 0, "init_unconvertible": 0,
        "skip_error_expectation": 0, "skip_unmatched": 0,
        "skip_ambiguous": 0, "skip_no_feature": 0, "skip_params": 0,
        "skip_unknown_then": 0,
    }
    skipped = []
    feature_cache = {}

    for path in sorted(CASES.rglob("*.case")):
        raw, sections, order, meta = base.parse_case(path)
        expected_lines = [l for l in sections.get("expected", []) if l != ""]
        raw_case_query = "\n".join(sections["query"])

        cat_a = any("_ID:" in l for l in expected_lines)
        maybe_b = (
            not cat_a
            and meta.get("ordered") is True
            and re.search(r"\bORDER\s+BY\b", raw_case_query, re.I)
        )
        if not cat_a and not maybe_b:
            continue
        if cat_a:
            stats["a_total"] += 1

        fpath = base.feature_path_for_source(tck_root, meta["source"])
        if fpath is None:
            stats["skip_no_feature"] += 1
            skipped.append((str(path), "no feature file"))
            continue
        if fpath not in feature_cache:
            feature_cache[fpath] = parse_feature(fpath)
        chosen, why = match_scenario(feature_cache[fpath], meta, raw_case_query)
        if chosen is None:
            stats["skip_%s" % why] += 1
            skipped.append((str(path), why))
            continue
        si, sc, setups, table = chosen
        if sc.then_kind == "error":
            stats["skip_error_expectation"] += 1
            continue
        if sc.then_kind is None or sc.then_kind == "unknown":
            stats["skip_unknown_then"] += 1
            skipped.append((str(path), "unrecognized Then step"))
            continue
        if sc.has_params:
            stats["skip_params"] += 1
            skipped.append((str(path), "parameterized scenario"))
            continue

        new_rows = [] if sc.then_kind == "empty" else rows_to_lines(table)
        ordered = sc.then_kind in ("rows_ordered", "rows_ordered_lists")

        if maybe_b:
            # Only rewrite if the stored rows were re-sorted at import:
            # sequence differs from upstream, multiset identical.
            if expected_lines == new_rows:
                continue
            if sorted(expected_lines) != sorted(new_rows):
                skipped.append((str(path), "B candidate: rows not a permutation of upstream"))
                continue

        meta["ordered"] = ordered
        meta["expected_provenance"] = "openCypher %s#%s" % (
            fpath.relative_to(oc_root), sc.name)
        sections["metadata"] = [json.dumps(meta, sort_keys=True, separators=(",", ":"))]
        sections["expected"] = new_rows

        if cat_a and "graph_initializer" not in sections:
            if any(s.strip() for s in setups):
                try:
                    dsl = convert_setups(setups)
                except base.Unconvertible as e:
                    dsl = None
                    stats["init_unconvertible"] += 1
                    skipped.append((str(path), "initializer unconvertible: %s" % e))
                if dsl:
                    order.insert(order.index("expected"), "graph_initializer")
                    sections["graph_initializer"] = dsl
                    stats["init_added"] += 1

        if not dry:
            path.write_text(rebuild(sections, order))
        stats["b_rewritten" if maybe_b else "a_rewritten"] += 1

    print("TCK expectation re-import summary:")
    for k, v in stats.items():
        print("  %-24s %d" % (k, v))
    if skipped:
        print("skipped cases:")
        for p, why in skipped:
            print("  %s  (%s)" % (p, why))


if __name__ == "__main__":
    main()
