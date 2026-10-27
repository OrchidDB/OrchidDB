CREATE TABLE documents (id BIGINT PRIMARY KEY, tenant VARCHAR NOT NULL, secret VARCHAR, amount NUMERIC(20,2));
INSERT INTO documents VALUES (1,'alice','alice-private',12.50),(2,'bob','bob-private',25.00),(3,'alice',NULL,NULL);
CREATE TABLE labels (id BIGINT PRIMARY KEY, title VARCHAR);
INSERT INTO labels VALUES (1,'first'),(2,'second'),(3,'third');
