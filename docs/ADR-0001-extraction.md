# ADR-0001: Extraction First

## Context
Extraction is required even for analysis-only; analysis must operate on deterministic raw streams.

## Decisions
- Always probe and persist JSONs (`mkvmerge -J`, `ffprobe`) verbatim under `manifest/`.
- Build `selection.json` with source order and full metadata we may need later.
- Extract **only** selected tracks.
- Use **mkvextract** for extraction; use **mkvmerge** for merging. (ffmpeg may be added later for transforms.)
- If no temp/work or output directory is provided, create them **next to the binary**:
  - Work root: `<binary_dir>/_work/job_<timestamp>`
  - Output root: `<binary_dir>/_out`
- For Analyze-Only, perform extraction for the needed tracks (REF/SEC[/TER]), run analysis, then clean extracted media but keep manifests/logs unless `--keep-temp`.

## Status
Accepted.
