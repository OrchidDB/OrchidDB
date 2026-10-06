-- Requires a running SpiceDB HTTP gateway with the schema/relationships in the
-- README's authorization example. Load Orchid first. Replace the production
-- endpoint/token, or use the README's loopback demo server.
CREATE SECRET company_auth (
    TYPE SPICEDB,
    ENDPOINT 'http://127.0.0.1:8448',
    TOKEN 'orchid-local-test'
);

CREATE TABLE slack_messages(id BIGINT, channel_id VARCHAR, body VARCHAR);
INSERT INTO slack_messages VALUES
    (1, 'engineering', 'Deployment notes'),
    (2, 'engineering', 'Release checklist'),
    (3, 'private', 'Private discussion');
CREATE TABLE message_chunks(id BIGINT, message_id BIGINT, channel_id VARCHAR, text VARCHAR);
INSERT INTO message_chunks VALUES
    (10, 1, 'engineering', 'Deployment notes'),
    (20, 2, 'engineering', 'Release checklist'),
    (30, 3, 'private', 'Private discussion');
CREATE TABLE message_parts(id BIGINT, message_id BIGINT, chunk_id BIGINT);
INSERT INTO message_parts VALUES (1,1,10), (2,2,20), (3,3,30);

CREATE PROPERTY GRAPH slack
VERTEX TABLES (
    slack_messages KEY(id) LABEL Message PROPERTIES(id, body),
    message_chunks KEY(id) LABEL Chunk PROPERTIES(id, text)
)
EDGE TABLES (
    message_parts KEY(id)
    SOURCE KEY(message_id) REFERENCES slack_messages(id)
    DESTINATION KEY(chunk_id) REFERENCES message_chunks(id)
    LABEL HAS_CHUNK
)
COMPUTED EDGES (
    RELATED SOURCE Message DESTINATION Message
    WHERE(source.id <> target.id)
    ORDER BY(target.id DESC)
    LIMIT PER SOURCE 1
)
AUTHORIZATION (
    PROVIDER company_auth, DEFAULT DENY,
    VERTEX Message RESOURCE channel KEY(channel_id) REQUIRE view,
    VERTEX Chunk RESOURCE channel KEY(channel_id) REQUIRE view
);

-- Set by the trusted embedding application, never accepted as an end-user claim.
SET GRAPH AUTHORIZATION (SUBJECT_TYPE 'user', SUBJECT_ID 'alice');
CYPHER slack MATCH (m:Message) RETURN m.id, m.body ORDER BY m.id;
-- Alice belongs to engineering: messages 1 and 2.
CYPHER slack MATCH (m:Message)-[:HAS_CHUNK]->(c:Chunk) RETURN m.id, c.text ORDER BY m.id;
CYPHER slack MATCH (:Message {id:1})-[:RELATED]->(m) RETURN m.id;
-- 2: the inaccessible message 3 does not occupy the computed edge's top-1.

SET GRAPH AUTHORIZATION (SUBJECT_TYPE 'user', SUBJECT_ID 'bob');
GREMLIN slack g.V().hasLabel('Message').values('body');
-- Bob has direct access to the private channel: Private discussion.
RESET GRAPH AUTHORIZATION;
-- Queries against slack now fail until the host supplies an identity again.
