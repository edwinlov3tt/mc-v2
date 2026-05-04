-- Phase 5A Stream C — Postgres test fixture.
--
-- Live Postgres tests in `tests/postgres_tests.rs` are gated behind
-- `#[ignore]` (run explicitly with `cargo test -p mc-drivers --
-- --ignored postgres`). Before running them, create the fixture below
-- on a local Postgres instance and export `MC_DRIVERS_TEST_PG_DSN`:
--
--   $ createdb mc_drivers_test
--   $ psql mc_drivers_test < tests/fixtures/postgres_setup.sql
--   $ export MC_DRIVERS_TEST_PG_DSN='postgres://localhost/mc_drivers_test'
--   $ cargo test -p mc-drivers -- --ignored
--
-- Same fixture is reused by `tests/duckdb_postgres_tests.rs` (gated
-- identically). Drop and re-create on schema changes.

DROP TABLE IF EXISTS mc_drivers_orders;

CREATE TABLE mc_drivers_orders (
    id      INT4    NOT NULL PRIMARY KEY,
    spend   FLOAT8,
    cpc     FLOAT8,
    channel TEXT,
    active  BOOLEAN
);

INSERT INTO mc_drivers_orders (id, spend, cpc, channel, active) VALUES
    (1, 100.50, 1.25, 'search',  TRUE),
    (2,  50.00, 0.75, 'social',  FALSE),
    (3, 250.25, 2.10, 'search',  TRUE),
    (4,   0.00, NULL, 'display', FALSE),
    (5, NULL,   1.50, 'search',  TRUE);
