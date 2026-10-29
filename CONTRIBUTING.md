# Contributing to StageSync

StageSync is an **independent, clean-room implementation**. If you would like to help,
this page sets out what the project is and what a change has to satisfy.

## The two rules

Most of what matters is in [`AGENTS.md`](AGENTS.md). Two points are worth repeating here.

**Never present an estimate as a measurement.** A drift figure that does not carry the
uncertainty that produced it is a number, not a measurement. If a figure could not be
measured, say so and say why. A disagreement between two methods is reported, never
averaged away.

**The recordings are read-only.** No stage of the pipeline may modify, rewrite, normalise
or truncate an input file. The reference recording is never altered at all, and the
originals are hashed before and after a run by the test suite to prove it.

## Before you start

Read [`AGENTS.md`](AGENTS.md) in full. It is the operating contract for this repository,
and it records a list of mistakes that have already been made here — each one with the
reason it went wrong. Most of them are easy to make again.

The clean-room boundary is also in that file. Do not read, decompile, disassemble or
otherwise use any proprietary source, any Ghidra output, any internal table, parameter
set or weight file. The permitted inputs are the documents in [`cleanroom/`](cleanroom/)
and the publicly documented DSP literature listed in
[`cleanroom/PUBLIC_DSP_REFERENCES.md`](cleanroom/PUBLIC_DSP_REFERENCES.md).

## The four gates

Every change has to pass all four, and the real exit codes have to be reported:

```powershell
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release
```

Do not weaken a lint rule or delete a test to make a run green. If a test is genuinely
wrong, say so explicitly and explain why.

## Tests

Tests are hermetic: they generate their own fixtures, use `tempfile::TempDir`, and depend
on nothing outside the test — no Python, no network, no checked-in media. The fixture
generator in `tools/make_fixtures.py` is a development aid and is **not** part of the
application; the Rust suite does not call it.

A test that measures the source's last samples is measuring the boundary, not the hole. A
test whose verdict depends on where a block boundary happens to fall is testing the
block size. Both of these have happened here; see `AGENTS.md`.

## Adding a dependency

Each dependency's licence must be compatible with the project's dual MIT/Apache-2.0
licence, and it must be recorded in [`LICENSES.md`](LICENSES.md). `symphonia` is
MPL-2.0: it is linked, not vendored, and a redistributor of a binary must honour the
MPL's file-level copyleft.

## Style

Rust 2021, `anyhow` at the edges, `thiserror` for typed errors in library internals.
Public items carry `///` comments that explain **why**, not what. Prefer a clear,
deterministic algorithm over a clever one: the same input must produce byte-identical
output.

## Pull requests

1. Say what the change does and why the old behaviour was wrong.
2. Show the four gates passing, with their exit codes.
3. If the change touches the alignment or the drift measurement, say what was measured
   and how it was checked — including what the check could **not** establish.

## Reporting a problem

Use the issue templates in `.github/ISSUE_TEMPLATE`. A bug report that includes the
`report.json` from the run, with the recordings removed, is worth a great deal more than
one that does not.

For anything that might be a security problem, follow [`SECURITY.md`](SECURITY.md) rather
than opening a public issue.
