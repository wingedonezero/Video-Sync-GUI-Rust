# Contributing

## Chat pin & patch rules
- `.chatpin` holds the base commit SHA for patches.
- Patches must be unified diff (`git apply` / GitKraken) against the exact SHA in `.chatpin`.
- Use plain UTF-8 spaces (no non-breaking spaces).

## External tools policy
- We call: `mkvmerge`, `mkvextract`, `ffmpeg`/`ffprobe`, `videodiff`.
- We validate tool presence and show actionable errors.

## Code layout
- `vsg-core`: probe/plan/chapters/mkvmerge tokens/process runner (library).
- `vsg-cli`: CLI for analyze/merge/probe.
- `vsg-gui`: imgui front-end (feature-gated while wiring).

## Style
- Rust 1.75+; `cargo fmt` and `cargo clippy -D warnings`.
- Errors: `thiserror` (lib), `anyhow` (bin). Logging with `tracing`.
