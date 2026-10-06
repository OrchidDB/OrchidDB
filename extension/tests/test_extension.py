"""Actual extension integration tests; no mocked compiler or SQL executor.

Run with the test environment described in ../README.md. Set
ORCHID_EXTERNAL_TESTS=1 to include real Iceberg snapshots and Lance datasets.
"""
import os
from pathlib import Path
import tempfile
import unittest

import duckdb

EXTENSION = Path(__file__).resolve().parents[1] / "build/orchid.duckdb_extension"


def quote(value):
    return "'" + str(value).replace("'", "''") + "'"


def connect(path=":memory:"):
    db = duckdb.connect(str(path), config={"allow_unsigned_extensions": "true"})
    db.execute(f"LOAD {quote(EXTENSION)}")
    return db


GRAPH = """
CREATE PROPERTY GRAPH social
VERTEX TABLES (people KEY (id) LABEL Person PROPERTIES (name, age))
EDGE TABLES (
    follows KEY (id)
    SOURCE KEY (src) REFERENCES people (id)
    DESTINATION KEY (dst) REFERENCES people (id)
    LABEL FOLLOWS PROPERTIES (since)
)
"""


def fixture(db):
    db.execute("""
        CREATE TABLE people(id BIGINT PRIMARY KEY, name VARCHAR, age INTEGER);
        INSERT INTO people VALUES (1, 'Alice', 30), (2, 'Bob', 40), (3, 'Cara', NULL);
        CREATE TABLE follows(id BIGINT, src BIGINT, dst BIGINT, since INTEGER);
        INSERT INTO follows VALUES (10,1,2,2020), (11,1,2,2021), (12,2,3,2022);
    """)
    db.execute(GRAPH)


class CypherTests(unittest.TestCase):
    def setUp(self):
        self.db = connect()
        fixture(self.db)

    def tearDown(self):
        self.db.close()

    def test_native_query_and_parallel_edges(self):
        self.assertEqual(self.db.execute("""
            CYPHER social MATCH (a:Person)-[e:FOLLOWS]->(b:Person)
            RETURN a.name AS src, b.name AS dst, e.since AS since ORDER BY since
        """).fetchall(), [("Alice", "Bob", 2020), ("Alice", "Bob", 2021), ("Bob", "Cara", 2022)])

    def test_optional_match_and_aggregation(self):
        self.assertEqual(self.db.execute("""
            CYPHER social MATCH (a:Person) OPTIONAL MATCH (a)-[e:FOLLOWS]->(b)
            RETURN a.name AS name, count(e) AS degree ORDER BY name
        """).fetchall(), [("Alice", 2), ("Bob", 1), ("Cara", 0)])

    def test_bounded_paths_and_existing_scalar_lowering(self):
        self.assertEqual(self.db.execute("""
            CYPHER social MATCH (a:Person)-[:FOLLOWS*2..2]->(b:Person)
            RETURN toUpper(a.name), b.name
        """).fetchall(), [("ALICE", "Cara"), ("ALICE", "Cara")])

    def test_native_comments_and_ordinary_sql_division(self):
        self.assertEqual(self.db.execute("""
            SELECT 8 // 2;
            CYPHER social // 'comment; $ignored
            MATCH (p:Person) RETURN count(p)
        """).fetchone(), (3,))
        self.assertEqual(self.db.execute("CYPHER social RETURN 'it\\'s ok' AS value").fetchone(), ("it's ok",))

    def test_parameters_are_bound_and_rebound(self):
        query = "CYPHER social MATCH (p:Person) WHERE p.name = $name RETURN p.age AS age"
        self.assertEqual(self.db.execute(query, {"name": "Alice"}).fetchall(), [(30,)])
        self.assertEqual(self.db.execute(query, {"name": "Bob"}).fetchall(), [(40,)])
        self.assertEqual(self.db.execute(query, {"name": "' OR true; DROP TABLE people; --"}).fetchall(), [])
        self.assertEqual(self.db.execute("SELECT count(*) FROM people").fetchone(), (3,))

    def test_host_transaction_visibility(self):
        self.db.execute("BEGIN; INSERT INTO people VALUES (4,'Dan',50)")
        self.assertEqual(self.db.execute("CYPHER social MATCH (p:Person) RETURN count(p) AS n").fetchone(), (4,))
        self.db.execute("ROLLBACK")
        self.assertEqual(self.db.execute("CYPHER social MATCH (p:Person) RETURN count(p) AS n").fetchone(), (3,))

    def test_explain_and_sql_composition(self):
        plan = self.db.execute("EXPLAIN CYPHER social MATCH (p:Person) WHERE p.age > 35 RETURN p.name").fetchall()
        plan = "\n".join(str(row) for row in plan)
        self.assertIn("SEQ_SCAN", plan)
        self.assertIn("age>35", plan.replace(" ", ""))
        self.assertNotIn("ORCHID_CYPHER", plan.upper())
        self.assertEqual(self.db.execute("""
            SELECT q.name, p.age FROM orchid_cypher('social',
                'MATCH (n:Person) RETURN n.name AS name') q
            JOIN people p USING(name) WHERE p.age > 35
        """).fetchall(), [("Bob", 40)])

    def test_atomic_definition_replace_drop_and_rollback(self):
        with self.assertRaisesRegex(duckdb.Error, "missing|unknown|not found"):
            self.db.execute(GRAPH.replace("CREATE PROPERTY", "CREATE OR REPLACE PROPERTY").replace("KEY (id) LABEL", "KEY (missing) LABEL"))
        self.assertEqual(self.db.execute("CYPHER social MATCH (n:Person) RETURN count(n)").fetchone(), (3,))
        self.db.execute("BEGIN; DROP PROPERTY GRAPH social; ROLLBACK")
        self.assertEqual(len(self.db.execute("DESCRIBE PROPERTY GRAPH social").fetchall()), 2)
        self.db.execute("DROP PROPERTY GRAPH social")
        with self.assertRaises(duckdb.Error):
            self.db.execute("CYPHER social MATCH (n) RETURN count(n)")

    def test_schema_changes_are_revalidated(self):
        self.db.execute("ALTER TABLE people RENAME COLUMN age TO years")
        with self.assertRaisesRegex(duckdb.Error, "age"):
            self.db.execute("CYPHER social MATCH (p:Person) RETURN p.age")

    def test_prepared_query_rebinds_graph_replacement(self):
        self.db.execute("PREPARE graph_query AS SELECT * FROM orchid_cypher('social', 'MATCH (p:Person) RETURN p.name AS name ORDER BY name')")
        self.assertEqual(len(self.db.execute("EXECUTE graph_query").fetchall()), 3)
        self.db.execute("CREATE VIEW older AS SELECT * FROM people WHERE age >= 40")
        self.db.execute("CREATE OR REPLACE PROPERTY GRAPH social VERTEX TABLES (older KEY(id) LABEL Person PROPERTIES(name,age))")
        self.assertEqual(self.db.execute("EXECUTE graph_query").fetchall(), [("Bob",)])

    def test_unmapped_source_types_do_not_block_binding(self):
        self.db.execute("ALTER TABLE people ADD COLUMN external_uuid UUID")
        self.assertEqual(self.db.execute("CYPHER social MATCH (p:Person) RETURN count(p)").fetchone(), (3,))

    def test_mapping_columns_follow_duckdb_identifier_rules(self):
        self.db.execute('CREATE TABLE mixed_case("ID" BIGINT, "Name" VARCHAR); INSERT INTO mixed_case VALUES (1,\'Ada\')')
        self.db.execute('CREATE PROPERTY GRAPH mixed VERTEX TABLES (mixed_case KEY(id) LABEL Person PROPERTIES(name AS name))')
        self.assertEqual(self.db.execute('CYPHER mixed MATCH (p:Person) RETURN p.name').fetchall(), [("Ada",)])

    def test_quoted_identifiers_and_literal_semicolons(self):
        self.db.execute('CREATE TABLE "odd people" ("the id" BIGINT, "full name" VARCHAR)')
        self.db.execute('INSERT INTO "odd people" VALUES (1, ?)', ["O'Reilly;雪"])
        self.db.execute('CREATE PROPERTY GRAPH "odd graph" VERTEX TABLES ("odd people" KEY ("the id") LABEL Person PROPERTIES ("full name" AS name))')
        self.assertEqual(self.db.execute('CYPHER "odd graph" MATCH (p:Person) RETURN p.name').fetchall(), [("O'Reilly;雪",)])
        self.assertEqual(self.db.execute("CYPHER social RETURN 'a;b' AS value").fetchone(), ("a;b",))

    def test_composite_keys_and_endpoint_type_check(self):
        self.db.execute("""
          CREATE TABLE tenant_people(tenant BIGINT, id BIGINT, name VARCHAR);
          INSERT INTO tenant_people VALUES (1,1,'A'),(1,2,'B'),(2,1,'C');
          CREATE TABLE tenant_links(tenant BIGINT,id BIGINT,src BIGINT,dst BIGINT);
          INSERT INTO tenant_links VALUES (1,10,1,2);
          CREATE PROPERTY GRAPH tenants
          VERTEX TABLES (tenant_people KEY(tenant,id) LABEL Person)
          EDGE TABLES (tenant_links KEY(tenant,id)
            SOURCE KEY(tenant,src) REFERENCES tenant_people(tenant,id)
            DESTINATION KEY(tenant,dst) REFERENCES tenant_people(tenant,id) LABEL KNOWS);
        """)
        self.assertEqual(self.db.execute("CYPHER tenants MATCH (a)-[:KNOWS]->(b) RETURN a.name,b.name").fetchall(), [("A", "B")])

    def test_deleting_connected_nodes_fails_without_modifying_data(self):
        with self.assertRaises(duckdb.Error):
            self.db.execute("CYPHER social MATCH (p:Person) DELETE p")
        self.assertEqual(self.db.execute("SELECT count(*) FROM people").fetchone(), (3,))

    def test_persistence_and_catalog_relative_sources(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "graph.duckdb"
            db = connect(path)
            fixture(db)
            db.close()
            db = connect(path)
            try:
                db.execute("ATTACH ':memory:' AS other; CREATE TABLE other.main.people(id BIGINT, name VARCHAR, age INTEGER); USE other")
                self.assertEqual(db.execute("CYPHER graph.main.social MATCH (p:Person) RETURN count(p)").fetchone(), (3,))
            finally:
                db.close()


@unittest.skipUnless(os.environ.get("ORCHID_EXTERNAL_TESTS") == "1", "set ORCHID_EXTERNAL_TESTS=1 for storage integration")
class ExternalStorageTests(unittest.TestCase):
    def test_iceberg_lance_join_and_indexed_search(self):
        import pyarrow as pa
        from pyiceberg.catalog.sql import SqlCatalog

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            catalog = SqlCatalog("fixture", uri="sqlite:///" + str(root / "catalog.db"), warehouse=root.as_uri())
            catalog.create_namespace("fixture")
            schema = pa.schema([("id", pa.int64()), ("person_id", pa.int64()), ("document_id", pa.int64())])
            edges = catalog.create_table("fixture.authorship", schema=schema)
            edges.append(pa.table({"id": [1, 2, 3], "person_id": [1, 1, 2], "document_id": [10, 20, 30]}, schema=schema))
            db = connect()
            try:
                db.execute("INSTALL iceberg; LOAD iceberg; INSTALL lance; LOAD lance")
                lance_dir = root / "lance"
                lance_dir.mkdir()
                db.execute(f"ATTACH {quote(lance_dir)} AS vectors (TYPE lance)")
                db.execute("""
                    CREATE TABLE vectors.main.documents AS SELECT * FROM (VALUES
                      (10::BIGINT,'A',[1,0,0]::FLOAT[3]),
                      (20::BIGINT,'B',[0,1,0]::FLOAT[3]),
                      (30::BIGINT,'C',[0,0,1]::FLOAT[3])
                    ) t(id,title,embedding);
                    CREATE TABLE people(id BIGINT,name VARCHAR);
                    INSERT INTO people VALUES (1,'Alice'),(2,'Bob');
                """)
                db.execute(f"CREATE VIEW authorship AS SELECT * FROM iceberg_scan({quote(edges.metadata_location)})")
                ddl = """
                    CREATE PROPERTY GRAPH knowledge
                    VERTEX TABLES (people KEY(id) LABEL Person,
                        vectors.main.documents AS docs KEY(id) LABEL Document PROPERTIES(title))
                    EDGE TABLES (authorship KEY(id)
                        SOURCE KEY(person_id) REFERENCES people(id)
                        DESTINATION KEY(document_id) REFERENCES docs(id) LABEL AUTHORED)
                """
                db.execute(ddl)
                query = "CYPHER knowledge MATCH (p:Person)-[:AUTHORED]->(d:Document) WHERE p.name='Alice' RETURN d.title AS title ORDER BY title"
                self.assertEqual(db.execute(query).fetchall(), [("A",), ("B",)])
                plan = str(db.execute("EXPLAIN " + query).fetchall())
                self.assertIn("LANCE", plan.upper())
                self.assertTrue("PARQUET" in plan.upper() or "ICEBERG" in plan.upper(), plan)

                dataset = str(lance_dir / "documents.lance")
                db.execute(f"CREATE INDEX vec_idx ON {quote(dataset)} (embedding) USING IVF_FLAT WITH (num_partitions=1, metric_type='l2')")
                db.execute(f"CREATE VIEW nearest_docs AS SELECT * FROM lance_vector_search({quote(dataset)}, 'embedding', [1,0,0]::FLOAT[3], k=1, use_index=true, explain_verbose=true)")
                db.execute(ddl.replace("knowledge", "nearest").replace("vectors.main.documents AS docs", "nearest_docs AS docs"))
                self.assertEqual(db.execute("CYPHER nearest MATCH (p)-[:AUTHORED]->(d) RETURN p.name,d.title").fetchall(), [("Alice", "A")])
                search_plan = str(db.execute("EXPLAIN CYPHER nearest MATCH (d:Document) RETURN d.title").fetchall())
                self.assertIn("LANCE", search_plan.upper())
                self.assertIn("ANNSubIndex", search_plan)
                print("Storage extensions:", db.execute("SELECT extension_name, extension_version FROM duckdb_extensions() WHERE loaded AND extension_name IN ('iceberg','lance')").fetchall())
            finally:
                db.close()


if __name__ == "__main__":
    unittest.main(verbosity=2)
