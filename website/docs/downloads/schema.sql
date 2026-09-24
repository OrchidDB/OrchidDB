CREATE TABLE users (id BIGINT, name VARCHAR, age BIGINT);
INSERT INTO users VALUES
  (1, 'alice', 30), (2, 'bob', 28), (3, 'carol', 41);
CREATE TABLE orders (order_id BIGINT, user_id BIGINT, total DOUBLE);
INSERT INTO orders VALUES
  (100, 1, 50.0), (101, 1, 120.0), (102, 2, 80.0), (103, 3, 500.0);
CREATE TABLE follows (src BIGINT, dst BIGINT);
INSERT INTO follows VALUES (1, 2), (1, 3), (2, 3);
