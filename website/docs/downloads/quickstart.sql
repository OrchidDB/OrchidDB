CREATE TABLE people(id BIGINT PRIMARY KEY, name VARCHAR, age INTEGER);
CREATE TABLE follows(id BIGINT, src BIGINT, dst BIGINT, since INTEGER);
INSERT INTO people VALUES (1, 'Alice', 30), (2, 'Bob', 40);
INSERT INTO follows VALUES (10, 1, 2, 2020);

CREATE PROPERTY GRAPH social
VERTEX TABLES (people KEY (id) LABEL Person PROPERTIES (name, age))
EDGE TABLES (
    follows KEY (id)
    SOURCE KEY (src) REFERENCES people (id)
    DESTINATION KEY (dst) REFERENCES people (id)
    LABEL FOLLOWS PROPERTIES (since)
);

CYPHER social
MATCH (a:Person)-[e:FOLLOWS]->(b:Person)
RETURN a.name AS person, b.name AS friend, e.since AS since;

GREMLIN social g.V().hasLabel('Person').out('FOLLOWS').values('name');
