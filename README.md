# Video-Sync-GUI (Rust)

Rust rewrite of the original Python app, preserving exact behavior:
- Always-add delay anchoring
- Language/default rules (Signs/Songs default, first-subtitle default if no EN audio)
- Explicit `--track-order`
- mkvmerge options via `@opts.json` (JSON array of argv tokens)
- Chapter rename/shift/snap/normalize
- Attachments (fonts) preservation
- Compact logging

## Workspace layout

```
Cargo.toml
crates/
  vsg-core/   # pure logic + process runner
  vsg-cli/    # CLI (analyze/merge/probe)
  vsg-gui/    # imgui GUI (feature-gated placeholder)
```

## Build
```
cargo build -p vsg-core -p vsg-cli
```

## Probe (first working step)
```
# prints parsed tracks from mkvmerge -J as JSON
cargo run -p vsg-cli -- probe --file /path/to/ref.mkv

# if mkvmerge isn't on PATH
cargo run -p vsg-cli -- probe --file /path/to/ref.mkv --mkvmerge /usr/bin/mkvmerge
```
