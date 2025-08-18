
# VSG Architecture (Baseline PR0–PR3)

Library core in `vsg-core`; CLI in `vsg-cli`. GUI can call into `vsg-core` later.

- `vsg-core`:
  - `analysis/xcorr.rs` — reference-windowed audio xcorr (**implemented**)
  - `plan.rs` — positive-only delay computation (**implemented**)
  - `probe.rs`, `extract.rs`, `opts.rs`, `chapters.rs` — TODO
- `vsg-cli`:
  - `analyze-audio` — **implemented**
  - other subcommands — TODO stubs with explicit errors
