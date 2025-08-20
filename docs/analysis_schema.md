
# Analysis JSON Schema

This document summarizes the fields written by `analyze` to `./<work>/manifest/analysis.json`.

- `meta`
  - `ref_audio_path`, `sec_audio_path`, `ter_audio_path`
  - `ref_language`, `sec_language`, `ter_language`
- `params`
  - `chunks`, `chunk_dur_s`, `sample_rate`, `stereo_mode`, `method`, `band`
- `runs.<source>` (where `<source>` is `sec` or `ter`)
  - `language` (picked track language)
  - `language_matched` (bool; whether it matches `ref_language`)
  - `chunks[]`
    - `center_s`, `window_samples`, `lag_ns`, `lag_ms`, `peak`
  - `summary`
    - `median_delay_ns`, `median_delay_ms`, `peak_max`
- `final`
  - `delays_ms_signed`, `delays_ns_signed`
  - `global_shift_ms`
  - `delays_ms_positive`
  - `peaks`
  - `notes` (legend)
