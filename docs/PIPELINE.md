
# PR2: Plan -> Make-Opts -> Mux

- Reads ./temp/manifest.json from Extract step.
- Computes positive-only delays from raw (sec/ter) ms.
- Final order: REF video -> SEC audio (preferred lang) -> SEC subs -> TER subs.
- Flags per track:
  - --compression 0:none
  - --language 0:<lang> (if known)
  - --track-name 0:<name> (if known)
  - --sync 0:<residual_ms> (video uses global_ms)
  - Audio default: first audio 'yes', others 'no'
  - Sub default: if --default-signs, pick first signs match; otherwise if --first-sub-default, first subtitle.
- Chapters from REF (if any): --chapters ./temp/REF_chapters.xml
- Attachments from TER: --attach-file <path>
- Writes mkvmerge @options JSON array tokens; runs mkvmerge.
- Optional --cleanup-temp to remove ./temp after success.
