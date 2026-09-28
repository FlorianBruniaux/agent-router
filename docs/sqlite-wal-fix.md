# Bundled SQLite WAL-reset correction

The four SQLite consumers pin `rusqlite = 0.39.0` with `bundled`, resolving to
`libsqlite3-sys 0.37.0` and SQLite **3.51.3**. This replaces SQLite 3.46.0.
The `fallible_uint` feature preserves checked `u64` and `usize` SQL conversions;
values outside SQLite's signed integer range must fail without wrapping.

[SQLite's WAL-reset advisory](https://sqlite.org/wal.html#the_wal_reset_bug)
describes a corruption race between concurrent writing/checkpointing connections
on the same WAL database. SQLite 3.51.3 includes the correction. Runtime and
coordinator control connections can write while their supervising connection
is open; the admission registry also supports multiple processes. Default
automatic checkpoints remain enabled. This update does not change the schema,
application locking or checkpoint policy.

The [rusqlite 0.39.0 source documentation](https://github.com/rusqlite/rusqlite/blob/v0.39.0/README.md#notes-on-building-rusqlite-and-libsqlite3-sys)
identifies the bundled version. `router-doctor/tests/sqlite_bundle.rs` checks the
engine's actual version and source identifier through SQL, plus unsigned numeric
boundaries and rejected writes. It does **not** reproduce the WAL-reset race.

This dependency update does not replace application fixes for closing auxiliary
file descriptors while SQLite holds POSIX locks. SQLite documents that distinct
[application locking hazard](https://sqlite.org/howtocorrupt.html#posix_advisory_locks_canceled_by_a_separate_thread_doing_close_).
