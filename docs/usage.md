
# Usage Details

## Selection manifest schema (excerpt)
```json
{
  "ref_tracks": [
    { "file_path": "/path/to/REF.mkv", "track_id": 1, "type": "audio", "language": "jpn" }
  ],
  "sec_tracks": [
    { "file_path": "/path/to/SEC.mkv", "track_id": 2, "type": "audio", "language": "eng" }
  ],
  "ter_tracks": [
    { "file_path": "/path/to/TER.mkv", "track_id": 1, "type": "audio", "language": "eng" }
  ]
}
```
Missing `language`/`codec` will be filled by `mkvmerge -J` in extract.

## Language matching (analyze)
- Determine REF audio language from the enriched manifest.
- In `sec/` and `ter/` directories, pick the **first file** with matching `.<lang>.` in the filename.
- If none, fall back to the **first audio** file encountered.

## Accuracy & performance
- Cross-correlation uses FFT and parabolic refinement for sub-sample alignment.
- `sample_rate` governs time resolution; 24k is fast and usually within 0.1–0.3 ms of 48k.
- `stereo-mode best` tends to be the most robust across varied mixes.

## Examples

### Fast check
```bash
vsg-cli analyze --from-manifest ./tmp_work/manifest/selection.json \
  --duration-s 1440 --chunks 8 --chunk-dur 6.0 \
  --sample-rate 12000 --stereo-mode best --method fft \
  --work-dir ./tmp_work
```

### Maximum precision
```bash
vsg-cli analyze --from-manifest ./tmp_work/manifest/selection.json \
  --duration-s 1440 --chunks 12 --chunk-dur 10.0 \
  --sample-rate 48000 --stereo-mode best --method fft --band none \
  --work-dir ./tmp_work
```
