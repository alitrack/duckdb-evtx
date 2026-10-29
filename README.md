# duckdb-evtx

**Windows Event Log (.evtx) inspection, in SQL.** Point a `.evtx` file at the
extension — via DuckDB's own `read_blob('Security.evtx')` — and ask how many
event records it contains, or whether it's a parseable Event Log at all. No
`evtxparse`, no `plaso`, no Python.

```sql
LOAD 'evtx.duckdb_extension';

-- Is this a real .evtx?
SELECT evtx_is_valid(read_blob('C:\Logs\security.evtx')) AS looks_like_evtx;

-- How many events?
SELECT evtx_record_count(read_blob('C:\Logs\security.evtx')) AS events;

-- Pre-filter a table of files before doing the heavy work
SELECT path
FROM files
WHERE evtx_is_valid(content) AND evtx_record_count(content) > 0;
```

## Why

`.evtx` files are the raw Windows Event Log. The first thing any DFIR analyst or
security tool does with one is establish that it *is* a valid Event Log and how
many events it holds — before reaching for a heavyweight parser. Today that's a
round-trip to `evtxparse` / `plaso` / a Python script, then a join back. This
extension makes the two cheap inspection primitives SQL functions: the file is a
`BLOB` column, the answers are scalar values you can `WHERE`, `GROUP BY`, or fan
out over a table of captured files.

## Functions

| Function | Returns | Notes |
|---|---|---|
| `evtx_record_count(bytes)` | `BIGINT` | Number of event records in the file (capped at 5 000 000). Garbage / truncated / oversized / non-`.evtx` input → `0`. `NULL` input → `NULL`. |
| `evtx_is_valid(bytes)` | `BOOLEAN` | `true` iff the bytes carry the `ElfFile\0` magic **and** the parser constructs successfully over them. Garbage → `false`. `NULL` input → `NULL`. |

`bytes` is a `BLOB` holding a `.evtx` file. Read one with DuckDB's built-in
`read_blob('path')`. Both functions are **total** and **hostile-input-safe**:
the input is bounded to 256 MiB, parser construction and per-record iteration
each run under `catch_unwind`, and a single corrupt record never aborts the file
— so a `.evtx` from a *compromised* host can't crash the DuckDB process.

## Scope (v1)

- **The two flat inspection scalars** (verbatim port of the pure `evtx_parse`
  record counter + validity probe from Query-farm's `vgi-evtx` worker, MIT).
  This builds on the upstream [`evtx`](https://crates.io/crates/evtx) parser
  crate (`0.13`, `default-features = false`) — a real external dependency, so
  this extension is *not* dependency-free.
- **Not ported:** the `evtx_records` **table function** (one row per event,
  with `record_id` / `event_id` / `provider` / `channel` / `computer` /
  `level` / `time_created` lifted from `Event.System` and the full event as a
  JSON column). That's a table-in-out, not a flat scalar, and is the natural v2.

## Build

```sh
make release        # -> target/release/evtx.duckdb_extension
make test           # bundled DuckDB runs tests/integration.rs (SQL-level)
```

The cdylib links the DuckDB C extension ABI (resolved at `LOAD` time) plus the
`evtx` parser + its transitive tree. `panic = "unwind"` keeps a bug in the
extension from aborting the host process.
