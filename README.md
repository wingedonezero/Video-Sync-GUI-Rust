
# Video-Sync-GUI-Rust (CLI core)

## What we have so far

### Step 1 — Extraction
- Uses `mkvextract tracks` to pull only the selected tracks.
- Flat output layout: `./<work>/ref|sec|ter` (no type subfolders).
- Filenames: `NNN_<type>.<language>.<ext>` (e.g. `000_audio.jpn.aac`).
- Extensions are assigned by codec (AAC/AC3/EAC3/DTS/FLAC/OPUS/VORBIS/PCM, ASS/SRT/PGS, H264/HEVC/VC1/MPEG2).

**Manifest mode**
- `vsg-cli extract --manifest selection.json --work-dir ./tmp_work`
- CLI auto-probes input files with `mkvmerge -J` and fills missing `codec`/`language`.
- The enriched manifest is saved to `./tmp_work/manifest/selection.json`.

### Step 2 — Analysis (audio cross-correlation)
- N chunks (default 10) **evenly spaced** across the full REF duration.
- Cross-correlation with FFT and sub-sample peak interpolation.
- **Auto language matching**: if REF audio is `jpn`, analyzer picks `jpn` tracks in SEC/TER if present; otherwise falls back to first audio.
- Writes `./<work>/manifest/analysis.json` including:
  - `delays_ms_signed` (per source, signed)
  - `global_shift_ms` (positive-only shift)
  - `delays_ms_positive` (shifted)
  - `peaks` (peak score per comparison)

## CLI usage

### Extract
```bash
vsg-cli extract --manifest selection.json --work-dir ./tmp_work
```

### Analyze (auto-pick tracks via manifest)
```bash
# duration_s should come from the REF container
DUR=$(ffprobe -v error -show_entries format=duration -of csv=p=0 <REF.mkv> | awk '{printf("%.0f",$1)}')

vsg-cli analyze \
  --from-manifest ./tmp_work/manifest/selection.json \
  --duration-s "$DUR" \
  --chunks 10 \
  --chunk-dur 8.0 \
  --sample-rate 24000 \
  --stereo-mode best \
  --method fft \
  --band none \
  --work-dir ./tmp_work
```

## Options reference

- `--chunks <N>`: number of evenly spaced chunks (default 10)
- `--chunk-dur <seconds>`: per-chunk window (default 8.0)
- `--sample-rate {12000|24000|48000}`: analysis decode rate
  - Higher = finer time resolution (48k → 20.833 µs/sample)
  - 24k is a good speed/accuracy trade-off
- `--stereo-mode {mono|left|right|mid|best}`
  - `best`: compute L/L and R/R, pick the stronger match per chunk
  - `mid`: (L+R)/2
- `--method {fft|compat}`
  - `fft`: fast FFT-based correlation (recommended)
  - `compat`: placeholder for Python-style parity; currently aliases to FFT
- `--band {none|voice}`
  - `voice`: light band-limit (100–3000 Hz) for robustness on dialog-heavy sources
- `--from-manifest <path>`: auto-pick REF/SEC/TER audio files using REF language (fallback to first audio)
- Manual overrides: `--ref-audio-path`, `--sec-audio-path`, `--ter-audio-path`

## Defaults & directories
- If no `--work-dir`/`--out-dir`, dirs are created next to the binary (e.g., `./_work`, `./_out`).

## Next up
- Video-based diff (`videodiff`) option for frame-accurate offsets
- Chapter rename & I-frame snap
- Final MKV assembly (mkvmerge) with computed delays
