# SPARQL assertion rules

Both compared engines receive the same original W3C fixtures. The fixture
parser preserves numeric lexical forms; assertions never modify engine inputs.
Raw expected and actual terms remain recorded in result JSON.

SELECT results preserve variable sets, multiplicity, unbound values, RDF
datatypes, language tags, and a global bijection for blank nodes. Ordering is
asserted when the expected DAWG result artifact contains `rs:index`, following
the [W3C test instructions](https://www.w3.org/2009/sparql/docs/tests/).
Manifests declaring `mf:LaxCardinality` permit duplicate elimination for
REDUCED results. All other solution assertions preserve multiplicity.

Numeric result literals of the **same datatype** are normalized with pinned
PyOxigraph 0.5.11's Store implementation. This follows the normalization in
[Oxigraph 0.5.11's W3C runner](https://github.com/oxigraph/oxigraph/blob/v0.5.11/testsuite/src/sparql_evaluator.rs#L525).
For example, `2E-1` and `2.0E-1` denote the same double. This is necessary even
for the original MIN fixtures, whose expected result spells a selected input
value differently. There is no numeric epsilon, cross-datatype equality, or
string normalization. STR results and ill-typed literals retain lexical checks.

Graph results use RDF graph isomorphism. Update assertions compare the complete
dataset, including empty graph names and blank-node identity across graphs.
No expected output is provided to either execution engine.
