
# Pipeline (agreed invariants)

1) Analyze: 10 evenly spaced windows on reference timeline; 8kHz mono; xcorr ±lag; winner = votes then avg%; ms rounding.
2) Extract: deterministic filenames; only what final mux needs.
3) Plan: positive-only delays; strict ordering & defaults; compression none.
4) Make Opts: mkvmerge argv tokens JSON; pretty log.
5) Mux: run mkvmerge @opts.json; temp cleanup policy.
