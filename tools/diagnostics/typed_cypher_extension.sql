SELECT orchid_cypher_pack(NULL) IS NULL AS null_preserved,
       orchid_cypher_eq(orchid_cypher_map(), orchid_cypher_map()) AS empty_maps_equal,
       orchid_cypher_size(orchid_cypher_items(range(5000))) AS nested_list_size;
CREATE TABLE nodes(id BIGINT PRIMARY KEY, n BIGINT);
CREATE TABLE edges(id BIGINT PRIMARY KEY, src BIGINT, dst BIGINT);
INSERT INTO nodes VALUES (1,1),(2,2),(3,3),(4,4);
INSERT INTO edges VALUES (1,1,2),(2,2,3),(3,2,4);
CREATE PROPERTY GRAPH typed_probe
VERTEX TABLES (nodes KEY(id) LABEL N PROPERTIES(n))
EDGE TABLES (edges KEY(id) SOURCE KEY(src) REFERENCES nodes(id)
    DESTINATION KEY(dst) REFERENCES nodes(id) LABEL T NO PROPERTIES);
SELECT * FROM orchid_cypher('typed_probe',
    'MATCH p=(n:N)-->() WHERE n.n=1 RETURN n.n AS n, [x IN nodes(p) | size([(x)-->() | 1])] AS counts');
SELECT * FROM orchid_cypher('typed_probe',
    'WITH [1,2,3] AS xs RETURN [x IN reverse(xs) WHERE x > 1 | x+1] AS result');
SELECT * FROM orchid_cypher('typed_probe',
    'UNWIND [1,2] AS k RETURN k, all(x IN [1,2] WHERE any(y IN [1,2] WHERE y=x AND y>=k)) AS result ORDER BY k');
