
# PR1: Probe + Extract

Implements:
- `probe` via `mkvmerge -J` into TrackMeta & attachments.
- `extract` via `mkvextract` according to rules:
  - REF: first video track + chapters XML
  - SEC: English audio + all subtitles
  - TER: all subtitles + all attachments (fonts)
- Deterministic filenames: REF_v_000.h264, SEC_a_001.ac3, TER_s_002.ass, TER_attach_###_name.ttf
- Writes `manifest.json` under temp root.
