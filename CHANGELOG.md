
# Changelog
All notable changes to this project will be documented in this file.

## [0.1.0] – Day 1 (fresh repo reset)
- Initialized Rust workspace (`vsg-core`, `vsg-cli`).
- Wired basic extraction and CLI skeleton.
- Draft README.

## [0.2.0] – Audio analysis & language matching
- **Core**
  - `vsg-core/src/analyze/audio_xcorr.rs`: FFT cross-correlation across evenly spaced chunks; stereo modes (`mono|left|right|mid|best`); optional voice band.
  - Delays reported in **nanoseconds** and rounded **milliseconds**; per-chunk peaks.
- **CLI**
  - Language-aware selection: prefer REF language in SEC/TER; fallback to first audio.
  - Rich `analysis.json` with `meta`, `params`, `runs.*` (chunks + summary), and `final` block (signed delays, global shift, positive-only delays).
- **Docs**
  - README updated: usage, JSON explanation, defaults, and Known Issues.
  - `docs/analysis_schema.md` added.

## [Unreleased]
- Ref FFT caching across chunks for extra speed.
- Outlier rejection (IQR/MAD) toggle.
- Videodiff analysis path.
- Chapters processing + I-frame snapping.
- Final merge (mkvmerge) with delays and cleanup.
