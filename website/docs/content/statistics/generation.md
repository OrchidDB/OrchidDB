# Generating statistics

Generation collects a snapshot for the sources in a graph mapping. It uses the
existing database connection and its current data visibility. One generation can
make several read requests.

OrchidDB first requests available source metadata, then samples mapped columns.
Large sources with usable row estimates use block sampling; other sources use
bounded reads. The collector chooses the requests and processes their results in
shared Rust code.

## What is collected

| Data | Collected information |
| --- | --- |
| Sources | Row and byte estimates, sample size and collection method |
| Columns | Null frequency, distinct values, frequent values, ranges, histograms and average width |
| Column combinations | Distinct tuples and frequent combinations of selected columns |
| Collections | List lengths, empty and null lists, element distributions and values occurring within parents |
| Relationships | Distinct source and target tuples, endpoint pairs and frequently occurring endpoints |
| RDF mappings | Sampled subjects, objects, predicates and term descriptions associated with mapping rules |

These statistics describe different parts of query cost. Value frequencies help
estimate how many rows a filter selects. Key distributions help estimate join
size. List lengths describe the work introduced by expanding a collection.
Column widths help estimate the amount of data a source contributes.

Statistics on column combinations preserve information that individual columns
miss. For a composite key, the distribution of complete tuples can differ from
what separate distinct counts suggest.

## One collection for the mapping

Source, relationship and RDF summaries share the same samples. A physical source
used by several graph mappings is collected as part of the same snapshot. The
result can then be reused by queries in any supported graph language.

Collection is bounded automatically. There are no sample-size settings to choose.
Generate once after setting up the mapping, reuse the snapshot, and regenerate
when the data distribution changes enough to affect planning.
