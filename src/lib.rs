// DuckDB loadable extension: Windows Event Log (.evtx) file inspection, in SQL.
//
// Thin quack-rs binding over `evtx_parse` (verbatim from Query-farm's vgi-evtx
// evtx-worker, MIT): a pure-compute record counter + validity probe over an
// in-memory `.evtx` buffer. It builds on the upstream `evtx` parser crate
// (0.13, `default-features = false`). Only the Arrow IPC transport was
// replaced with direct C-API vector I/O.
//
// Function surface (v1 — 2 flat scalars):
//
//   evtx_record_count(bytes) -> BIGINT    event records in the file (0 for garbage)
//   evtx_is_valid(bytes)     -> BOOLEAN   `ElfFile\0` magic + parser constructs
//
// `bytes` is a `BLOB` holding a `.evtx` file (e.g. read with DuckDB's
// `read_blob('Security.evtx')`). Both functions are **total** on arbitrary
// input: a garbage/oversized/truncated blob yields `0` / `false` (never a SQL
// error), and a `NULL` input yields `NULL`.
//
// Not ported in v1: the `evtx_records` table function (one row per event record,
// with the full event as a JSON column) — that's a table-in-out, not a flat
// scalar; see the README.

use quack_rs::connection::{Connection, Registrar};
use quack_rs::error::ExtensionError;
use quack_rs::scalar::ScalarFunctionBuilder;
use quack_rs::types::TypeId;
use quack_rs::vector::VectorReader;

mod evtx_parse;

/// Read a BLOB cell into bytes; None when the row is NULL/invalid.
unsafe fn payload_bytes(value: &VectorReader, row: usize) -> Option<Vec<u8>> {
    if !value.is_valid(row) {
        return None;
    }
    Some(value.read_blob(row).to_vec())
}

// ─────────────────────────────────────────────────────────────────────────────
// evtx_record_count(bytes) -> BIGINT
// ─────────────────────────────────────────────────────────────────────────────

quack_rs::scalar_callback!(record_count_fn, |_info, input, output| {
    unsafe {
        let payload = VectorReader::new(input, 0);
        let row_count = payload.row_count();
        let mut writer = quack_rs::vector::VectorWriter::new(output);
        for row in 0..row_count {
            match payload_bytes(&payload, row) {
                Some(bytes) => writer.write_i64(row, evtx_parse::record_count(&bytes)),
                None => writer.set_null(row),
            }
        }
    }
});

// ─────────────────────────────────────────────────────────────────────────────
// evtx_is_valid(bytes) -> BOOLEAN
// ─────────────────────────────────────────────────────────────────────────────

quack_rs::scalar_callback!(is_valid_fn, |_info, input, output| {
    unsafe {
        let payload = VectorReader::new(input, 0);
        let row_count = payload.row_count();
        let mut writer = quack_rs::vector::VectorWriter::new(output);
        for row in 0..row_count {
            match payload_bytes(&payload, row) {
                Some(bytes) => writer.write_bool(row, evtx_parse::is_valid(&bytes)),
                None => writer.set_null(row),
            }
        }
    }
});

// ─────────────────────────────────────────────────────────────────────────────
// Registration + entry point
// ─────────────────────────────────────────────────────────────────────────────

unsafe fn register_all(con: &Connection) -> Result<(), ExtensionError> {
    con.register_scalar(
        ScalarFunctionBuilder::new("evtx_record_count")
            .param(TypeId::Blob)
            .returns(TypeId::BigInt)
            .function(record_count_fn),
    )?;

    con.register_scalar(
        ScalarFunctionBuilder::new("evtx_is_valid")
            .param(TypeId::Blob)
            .returns(TypeId::Boolean)
            .function(is_valid_fn),
    )?;

    Ok(())
}

quack_rs::entry_point_v2!(evtx_init_c_api, |con| register_all(con));
