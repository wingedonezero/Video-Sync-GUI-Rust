
# Video-Sync-GUI-Rust

Audio/video sync tooling (Rust). This repo is the **fresh** restart; we’re building the pipeline step-by-step with a strong focus on **deterministic extraction**, **robust analysis**, and **clear artifacts** for later steps (chapters, snapping to I-frames, and final mkvmerge).

> Rules we follow right now:
> - **Extraction**: `mkvextract` (not ffmpeg).
> - **Merge**: `mkvmerge` (ffmpeg possible later, but *not* for extraction/merge).
> - **Analysis**: in RAM; reads extracted audio files, computes cross-correlation across evenly spaced chunks; reports **nanosecond** and **millisecond** delays.

## Features (current)
- ✅ **Deterministic extraction** of selected tracks from `REF`, `SEC`, `TER` using `mkvextract`.
  - Output structure is flat by source: `./<work>/{ref,sec,ter}/NNN_audio.<lang>.<ext>`
  - File extensions map correctly from codec IDs (aac, ac3, eac3, dts, thd, flac, opus, vorbis, pcm/wav).
  - If no `--work-dir`/`--out-dir` are supplied, they’re created alongside the binary (`./tmp_work`, `./out`).
- ✅ **Language-aware analysis**:
  - Chooses the **first audio** in REF (by manifest order) and uses its **language** as the target.
  - In SEC/TER, it **picks the first track with that same language**; if none, **falls back to the first audio**.
- ✅ **Cross-correlation analysis**:
  - Evenly spaced **10 chunks** by default across full duration (configurable).
  - Per chunk: cross-correlates REF vs OTHER with FFT-based method, sub-sample interpolation.
  - Stereo handling: `mono|left|right|mid|best` (default `best` compares L/L and R/R choosing the higher peak per chunk).
  - Optional **voice band** prefilter (100Hz–3kHz) for dialogue-heavy sources.
  - Outputs **per-chunk** lags in **ns**/**ms**, per-chunk peaks, and per-source summaries.
- ✅ **Richer `analysis.json`**:
  - `meta`: resolved paths & languages.
  - `params`: what knobs were used.
  - `runs.sec`/`runs.ter`: detailed chunks + summary (median delay ns/ms, peak max).
  - `final`: signed delays (ms/ns), a **global positive shift**, positive-only delays for friendly consumption later, and a compact legend.

## Usage
### 1) Extract
```bash
# Build (workspace root)
cargo build -p vsg-cli --release

# Run extract
./rust/target/release/vsg-cli extract \
  --manifest ./tmp_work/manifest/selection.json \
  --work_dir ./tmp_work
```
**Notes**
- The command expects a `SelectionManifest` (see below). The extractor persists an enriched copy to `./tmp_work/manifest/selection.json` (adds language/codec from `mkvmerge -J`).
- An `extract.log` is emitted at `./tmp_work/manifest/extract.log` for quick verification.

### 2) Analyze
```bash
# Total duration rounded to seconds (example with ffprobe)
DUR=$(ffprobe -v error -show_entries format=duration -of csv=p=0 "/path/to/REF.mkv" | awk '{printf("%.0f",$1)}')

./rust/target/release/vsg-cli analyze \
  --from-manifest ./tmp_work/manifest/selection.json \
  --duration-s "$DUR" \
  --chunks 10 --chunk-dur 8.0 \
  --sample-rate 24000 --stereo-mode best --method fft --band none \
  --work_dir ./tmp_work
```
This writes `./tmp_work/manifest/analysis.json`.

### `SelectionManifest` (shape)
The tool reads a JSON manifest describing which tracks to extract from REF/SEC/TER. During extraction, we probe with `mkvmerge -J` and fill in missing `codec`/`language` fields.
```jsonc
{
  "ref_tracks": [
    { "file_path": "/path/to/ref.mkv", "track_id": 1, "type": "audio", "codec": null, "language": null }
  ],
  "sec_tracks": [
    { "file_path": "/path/to/sec.mkv", "track_id": 2, "type": "audio", "codec": null, "language": null }
  ],
  "ter_tracks": [
    { "file_path": "/path/to/ter.mkv", "track_id": 3, "type": "audio", "codec": null, "language": null }
  ]
}
```

## `analysis.json` (explained)
Example structure:
```jsonc
{
  "meta": {
    "ref_audio_path": "tmp_work/ref/000_audio.jpn.aac",
    "ref_language": "jpn",
    "sec_audio_path": "tmp_work/sec/000_audio.eng.thd",
    "ter_audio_path": "tmp_work/ter/000_audio.jpn.aac",
    "sec_language": "eng",
    "ter_language": "jpn"
  },
  "params": {
    "chunks": 10,
    "chunk_dur_s": 8.0,
    "sample_rate": 24000,
    "stereo_mode": "Best",
    "method": "Fft",
    "band": "None"
  },
  "runs": {
    "sec": {
      "language": "eng",
      "language_matched": false,
      "chunks": [
        { "center_s": 90.0, "window_samples": 192000, "lag_ns": -1250000, "lag_ms": -1.25, "peak": 3.41 }
        // ... 10 entries, evenly spaced across the whole file
      ],
      "summary": {
        "median_delay_ns": -1250000,
        "median_delay_ms": -1,
        "peak_max": 4.02
      }
    },
    "ter": { /* same shape */ }
  },
  "final": {
    "delays_ms_signed": { "sec": -1, "ter": 0 },
    "delays_ns_signed": { "sec": -1250000, "ter": 0 },
    "global_shift_ms": 1,
    "delays_ms_positive": { "sec": 0, "ter": 1 },
    "peaks": { "sec": 4.02, "ter": 3.88 },
    "notes": [
      "delays_ms_signed: signed median delay per source (ms)",
      "delays_ns_signed: same in nanoseconds (ns)",
      "global_shift_ms: minimum shift to make all delays non-negative",
      "delays_ms_positive: per-source delays after applying global_shift_ms",
      "runs.*.chunks: detailed per-chunk lags/peaks for QA"
    ]
  }
}
```

**How to read it**
- `runs.*.chunks[]` → each chunk gives a local `lag_ns`/`lag_ms` and the correlation `peak`. Outliers will show up as inconsistent lags/low peaks.
- `runs.*.summary.median_delay_ms` → the **per-source delay** we’d use for merging.  
- `final.global_shift_ms` → the minimum value needed to make all per-source delays non-negative.  
- `final.delays_ms_positive` → convenient for formats that only accept positive delays; you add `global_shift_ms` to others as needed.

## Defaults & Behavior
- If **no `--work_dir`/`--out_dir`** are set, they’re created next to the binary.
- **Extraction-only** and **analysis-only** are both supported. Analysis reads whatever was extracted (REF is always required; SEC/TER optional).
- **Cleanup**: by default we keep extracted files. A `--keep-temp=false` flag can be added to the extract subcommand in the future to auto-clean (TBD).
- **Extensions**: container codecs are mapped to meaningful file extensions for readability and downstream tools.

## Known Issues & Next Steps (living list)
- [x] Extraction verified (hash-equal to gmkvextract outputs)
- [x] Language-first matching for analysis; fallback to first audio
- [x] Analysis per-chunk with sub-sample interpolation; outputs ns & ms
- [x] Richer `analysis.json` with human-readable **final** block
- [ ] Speed parity with Python (add ref FFT caching across chunks & sources)
- [ ] Optional outlier rejection in summary (IQR or MAD)
- [ ] Videodiff analysis path (second option) and flagging
- [ ] Chapters processing/rename + optional snap-to-I-frame
- [ ] Final assembly (delays applied to the right tracks) + mkvmerge step
- [ ] Automatic temp cleanup after successful merge

