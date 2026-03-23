# Video Sync GUI — Python → Rust Port Tracker

> **Method:** For each Python file, fully read the Python, then write/verify the Rust.
> No file marked "Done" unless every function, branch, and edge case is accounted for.
> If something can't be ported 1:1, it gets a note explaining what changed and why.

---

## PART 1: CORE (vsg_core → crates/vsg_core)

Python: 149 files | Rust: 148 files

### 1.1 Models & Config

| # | Python File | Rust File | Status | Notes |
|---|---|---|---|---|
| 1 | `models/settings.py` | `models/settings.rs` | ✅ Verified | 172/172 fields match, all defaults match, 6 tests pass. Pydantic→serde. |
| 2 | `models/jobs.py` | `models/jobs.rs` | ✅ Verified | Delays 4/4, PlanItem 33/33, MergePlan 5/5, PipelineResult 11/11. All fields match. status uses String not enum (worker compat). |
| 3 | `models/context_types.py` | `models/context_types.rs` | ✅ Verified | 14 TypedDicts + 2 type aliases all match. SegmentFlagsEntry uses Value for ClusterDiagnostic/Validation (avoids circular deps). |
| 4 | `models/converters.py` | `models/converters.rs` | ✅ Verified | 3/3 functions match: tracks_from_dialog_info, realize_plan_from_manual_layout, signature_for_auto_apply. Counter→HashMap. |
| 5 | `models/media.py` | `models/media.rs` | ✅ Verified | StreamProps 3/3, Track 4/4, Attachment 3/3. All fields match. |
| 6 | `models/types.py` | `models/enums.rs` | ✅ Verified | 25/25 Literal types match as Rust enums. All serde renames match Python strings. +1 extra JobStatus enum (from jobs.py). |
| 7 | `config.py` | `config.rs` | ✅ Verified | All methods ported. Added: `cleanup_old_style_editor_temp()`, `get_vs_index_for_video()` (md5), `remove_orphaned_keys()`, `get_orphaned_keys()`, `get_unrecognized_keys()`, `validate_schema()`, field-by-field recovery. Skipped: `_migrate_legacy_keys()` (not needed for Rust). Added md-5 crate. |

### 1.2 IO & Discovery

| # | Python File | Rust File | Status | Notes |
|---|---|---|---|---|
| 8 | `io/runner.py` | `io/runner.rs` | ✅ Verified | run/run_binary/run_with_options match. Compact mode, progress filter, tail buffer, stdin, binary mode all ported. Note: GPU subprocess env (system/gpu_env.py) not set — tracked as file #149. |
| 9 | `job_discovery.py` | `job_discovery.rs` | ✅ Verified | Single file + batch folder modes match. Extensions, sorting, matching all correct. Return format: Rust returns flat HashMap (UI wraps in {"sources":...}). |

### 1.3 Extraction

| # | Python File | Rust File | Status | Notes |
|---|---|---|---|---|
| 10 | `extraction/tracks.py` | `extraction/tracks.rs` | ✅ Verified | 11/11 functions, codec map 21/21, detailed error reports (3 types), video/audio detail builders with all special cases (DTS-HD MA, Atmos, HDR, HLG, DV). |
| 11 | `extraction/attachments.py` | `extraction/attachments.rs` | ✅ Verified | Font detection: all MIME prefixes, 11 exact MIMEs, binary+ext, 3 keywords, 10 extensions. Extraction flow matches. 4 tests. |

### 1.4 Analysis

| # | Python File | Rust File | Status | Notes |
|---|---|---|---|---|
| 12 | `analysis/types.py` | `analysis/types.rs` | ✅ Verified | 9 structs + 1 enum (DiagnosisResult) replaces Python's 13 dataclasses. All fields match. Enum variants replace 3 separate diagnosis classes. |
| 13 | `analysis/track_selection.py` | `analysis/track_selection.rs` | ✅ Verified | 2/2 functions match. Priority order: explicit → language → first. |
| 14 | `analysis/container_delays.py` | `analysis/container_delays.rs` | ✅ Verified | 3/3 functions match. Container delay chain calculation, min_timestamp→ms rounding. |
| 15 | `analysis/delay_selection.py` | `analysis/delay_selection.rs` | ✅ Verified | All 5 delay modes: Mode, Mode(Clustered), Mode(EarlyCluster), FirstStable, Average. |
| 16 | `analysis/drift_detection.py` | `analysis/drift_detection.rs` | ✅ Verified | Custom dbscan_1d replaces sklearn. linear_fit+r_squared replace numpy. Missing: _format_chunk_range, _analyze_transition_patterns (logging only). |
| 17 | `analysis/global_shift.py` | `analysis/global_shift.rs` | ✅ Verified | 2/2 functions match. Negative delay elimination and application. |
| 18 | `analysis/source_separation.py` | `analysis/source_separation.rs` | ⚠️ Stub | 57 lines vs 1559. Known gap — Python uses python-audio-separator (ML). Needs ONNX Runtime or subprocess approach. Decided in previous chat to defer. |
| 19 | `analysis/sync_stability.py` | `analysis/sync_stability.rs` | ✅ Verified | analyze_sync_stability with uniform + cluster modes. Outlier detection, std_dev calculations match. |
| 20 | `analysis/videodiff.py` | `analysis/videodiff.rs` | ⚠️ Gaps Found | Core flow works: dhash, hamming, frame extraction, RANSAC inlined. Missing: speed_drift_detected always false (TODO), confidence calculation simplified, _match_frames inlined. |

### 1.5 Analysis — Correlation

| # | Python File | Rust File | Status | Notes |
|---|---|---|---|---|
| 21 | `analysis/correlation/run.py` | `analysis/correlation/run.rs` | ❌ Not Audited | Main correlation entry |
| 22 | `analysis/correlation/decode.py` | `analysis/correlation/decode.rs` | ❌ Not Audited | Audio decode to PCM |
| 23 | `analysis/correlation/dense.py` | `analysis/correlation/dense.rs` | ❌ Not Audited | Dense sliding window |
| 24 | `analysis/correlation/filtering.py` | `analysis/correlation/filtering.rs` | ❌ Not Audited | Pre-correlation filtering |
| 25 | `analysis/correlation/confidence.py` | `analysis/correlation/confidence.rs` | ❌ Not Audited | Match confidence scoring |
| 26 | `analysis/correlation/registry.py` | `analysis/correlation/registry.rs` | ❌ Not Audited | Method registry |
| 27 | `analysis/correlation/gpu_backend.py` | `analysis/correlation/gpu_backend.rs` | ❌ Not Audited | GPU via tch-rs |
| 28 | `analysis/correlation/gpu_correlation.py` | `analysis/correlation/gpu_correlation.rs` | ❌ Not Audited | GPU correlation entry |

### 1.6 Analysis — Correlation Methods

| # | Python File | Rust File | Status | Notes |
|---|---|---|---|---|
| 29 | `analysis/correlation/methods/scc.py` | `analysis/correlation/methods/scc.rs` | ❌ Not Audited | Standard cross-correlation |
| 30 | `analysis/correlation/methods/gcc_phat.py` | `analysis/correlation/methods/gcc_phat.rs` | ❌ Not Audited | GCC-PHAT |
| 31 | `analysis/correlation/methods/gcc_scot.py` | `analysis/correlation/methods/gcc_scot.rs` | ❌ Not Audited | GCC-SCOT |
| 32 | `analysis/correlation/methods/gcc_whiten.py` | `analysis/correlation/methods/gcc_whiten.rs` | ❌ Not Audited | Whitened GCC |
| 33 | `analysis/correlation/methods/onset.py` | `analysis/correlation/methods/onset.rs` | ❌ Not Audited | Onset detection |
| 34 | `analysis/correlation/methods/spectrogram.py` | `analysis/correlation/methods/spectrogram.rs` | ❌ Not Audited | Spectrogram method |

### 1.7 Correction

| # | Python File | Rust File | Status | Notes |
|---|---|---|---|---|
| 35 | `correction/linear.py` | `correction/linear.rs` | ❌ Not Audited | Linear offset correction |
| 36 | `correction/pal.py` | `correction/pal.rs` | ❌ Not Audited | PAL speed correction |

### 1.8 Correction — Stepping

| # | Python File | Rust File | Status | Notes |
|---|---|---|---|---|
| 37 | `correction/stepping/types.py` | `correction/stepping/types.rs` | ❌ Not Audited | Stepping data types |
| 38 | `correction/stepping/run.py` | `correction/stepping/run.rs` | ❌ Not Audited | Main stepping entry |
| 39 | `correction/stepping/timeline.py` | `correction/stepping/timeline.rs` | ❌ Not Audited | Timeline building |
| 40 | `correction/stepping/boundary_refiner.py` | `correction/stepping/boundary_refiner.rs` | ❌ Not Audited | Boundary refinement |
| 41 | `correction/stepping/audio_assembly.py` | `correction/stepping/audio_assembly.rs` | ❌ Not Audited | Audio segment assembly |
| 42 | `correction/stepping/edl_builder.py` | `correction/stepping/edl_builder.rs` | ❌ Not Audited | EDL file building |
| 43 | `correction/stepping/data_io.py` | `correction/stepping/data_io.rs` | ❌ Not Audited | Data serialization |
| 44 | `correction/stepping/qa_check.py` | `correction/stepping/qa_check.rs` | ❌ Not Audited | Quality assurance |

### 1.9 Subtitles — Core

| # | Python File | Rust File | Status | Notes |
|---|---|---|---|---|
| 45 | `subtitles/data.py` | `subtitles/data.rs` | ❌ Not Audited | SubtitleData, styles, events |
| 46 | `subtitles/style_engine.py` | `subtitles/style_engine.rs` | ❌ Not Audited | Style application engine |
| 47 | `subtitles/edit_plan.py` | `subtitles/edit_plan.rs` | ❌ Not Audited | Edit plan building |
| 48 | `subtitles/track_processor.py` | `subtitles/track_processor.rs` | ❌ Not Audited | Per-track processing |
| 49 | `subtitles/sync_dispatcher.py` | `subtitles/sync_dispatcher.rs` | ❌ Not Audited | Sync mode dispatch |
| 50 | `subtitles/sync_modes.py` | `subtitles/sync_modes.rs` | ❌ Not Audited | Sync mode definitions |
| 51 | `subtitles/sync_utils.py` | `subtitles/sync_utils.rs` | ❌ Not Audited | Timing utilities |
| 52 | `subtitles/checkpoint_selection.py` | `subtitles/checkpoint_selection.rs` | ❌ Not Audited | Checkpoint selection |

### 1.10 Subtitles — Parsers & Writers

| # | Python File | Rust File | Status | Notes |
|---|---|---|---|---|
| 53 | `subtitles/parsers/ass_parser.py` | `subtitles/parsers/ass_parser.rs` | ❌ Not Audited | ASS/SSA parser |
| 54 | `subtitles/parsers/srt_parser.py` | `subtitles/parsers/srt_parser.rs` | ❌ Not Audited | SRT parser |
| 55 | `subtitles/writers/ass_writer.py` | `subtitles/writers/ass_writer.rs` | ❌ Not Audited | ASS output writer |
| 56 | `subtitles/writers/srt_writer.py` | `subtitles/writers/srt_writer.rs` | ❌ Not Audited | SRT output writer |

### 1.11 Subtitles — Operations

| # | Python File | Rust File | Status | Notes |
|---|---|---|---|---|
| 57 | `subtitles/operations/style_ops.py` | `subtitles/operations/style_ops.rs` | ❌ Not Audited | Style manipulation |
| 58 | `subtitles/operations/stepping.py` | `subtitles/operations/stepping.rs` | ❌ Not Audited | Stepping on subtitles |

### 1.12 Subtitles — Frame Utils

| # | Python File | Rust File | Status | Notes |
|---|---|---|---|---|
| 59 | `subtitles/frame_utils/timing.py` | `subtitles/frame_utils/timing.rs` | ❌ Not Audited | Frame↔time conversion |
| 60 | `subtitles/frame_utils/video_properties.py` | `subtitles/frame_utils/video_properties.rs` | ❌ Not Audited | FPS detection |
| 61 | `subtitles/frame_utils/video_reader.py` | `subtitles/frame_utils/video_reader.rs` | ❌ Not Audited | OpenCV video reader |
| 62 | `subtitles/frame_utils/frame_hashing.py` | `subtitles/frame_utils/frame_hashing.rs` | ❌ Not Audited | Perceptual hashing |
| 63 | `subtitles/frame_utils/surgical_rounding.py` | `subtitles/frame_utils/surgical_rounding.rs` | ❌ Not Audited | Sub-frame rounding |
| 64 | `subtitles/frame_utils/visual_verify.py` | `subtitles/frame_utils/visual_verify.rs` | ❌ Not Audited | Visual verification |
| 65 | `subtitles/frame_utils/frame_audit.py` | `subtitles/frame_utils/frame_audit.rs` | ❌ Not Audited | Frame audit checks |

### 1.13 Subtitles — Sync Mode Plugins

| # | Python File | Rust File | Status | Notes |
|---|---|---|---|---|
| 66 | `subtitles/sync_mode_plugins/time_based.py` | `subtitles/sync_mode_plugins/time_based.rs` | ❌ Not Audited | Time-based sync |
| 67 | `subtitles/sync_mode_plugins/video_verified/plugin.py` | `subtitles/sync_mode_plugins/video_verified/plugin.rs` | ❌ Not Audited | VV plugin entry |
| 68 | `subtitles/sync_mode_plugins/video_verified/matcher.py` | `subtitles/sync_mode_plugins/video_verified/matcher.rs` | ❌ Not Audited | Classic frame matching |
| 69 | `subtitles/sync_mode_plugins/video_verified/neural_matcher.py` | `subtitles/sync_mode_plugins/video_verified/neural_matcher.rs` | ❌ Not Audited | Neural matching |
| 70 | `subtitles/sync_mode_plugins/video_verified/neural_subprocess.py` | `subtitles/sync_mode_plugins/video_verified/neural_subprocess.rs` | ❌ Not Audited | Neural subprocess |
| 71 | `subtitles/sync_mode_plugins/video_verified/isc_model.py` | `subtitles/sync_mode_plugins/video_verified/isc_model.rs` | ❌ Not Audited | ISC model wrapper |
| 72 | `subtitles/sync_mode_plugins/video_verified/candidates.py` | `subtitles/sync_mode_plugins/video_verified/candidates.rs` | ❌ Not Audited | Candidate generation |
| 73 | `subtitles/sync_mode_plugins/video_verified/offset.py` | `subtitles/sync_mode_plugins/video_verified/offset.rs` | ❌ Not Audited | Offset calculation |
| 74 | `subtitles/sync_mode_plugins/video_verified/preprocessing.py` | `subtitles/sync_mode_plugins/video_verified/preprocessing.rs` | ❌ Not Audited | Frame preprocessing |
| 75 | `subtitles/sync_mode_plugins/video_verified/quality.py` | `subtitles/sync_mode_plugins/video_verified/quality.rs` | ❌ Not Audited | Quality scoring |
| 76 | `subtitles/sync_mode_plugins/video_verified/verification.py` | `subtitles/sync_mode_plugins/video_verified/verification.rs` | ❌ Not Audited | Verification pass |

### 1.14 Subtitles — OCR

| # | Python File | Rust File | Status | Notes |
|---|---|---|---|---|
| 77 | `subtitles/ocr/engine.py` | `subtitles/ocr/engine.rs` | ❌ Not Audited | OCR engine entry |
| 78 | `subtitles/ocr/pipeline.py` | `subtitles/ocr/pipeline.rs` | ❌ Not Audited | OCR processing pipeline |
| 79 | `subtitles/ocr/preprocessing.py` | `subtitles/ocr/preprocessing.rs` | ❌ Not Audited | Image preprocessing |
| 80 | `subtitles/ocr/postprocess.py` | `subtitles/ocr/postprocess.rs` | ❌ Not Audited | Text postprocessing |
| 81 | `subtitles/ocr/output.py` | `subtitles/ocr/output.rs` | ❌ Not Audited | Output formatting |
| 82 | `subtitles/ocr/dictionaries.py` | `subtitles/ocr/dictionaries.rs` | ❌ Not Audited | OCR dictionaries |
| 83 | `subtitles/ocr/subtitle_edit.py` | `subtitles/ocr/subtitle_edit.rs` | ❌ Not Audited | SE parser |
| 84 | `subtitles/ocr/word_lists.py` | `subtitles/ocr/word_lists.rs` | ❌ Not Audited | Word list management |
| 85 | `subtitles/ocr/romaji_dictionary.py` | `subtitles/ocr/romaji_dictionary.rs` | ❌ Not Audited | Romaji dictionary |
| 86 | `subtitles/ocr/wrapper.py` | `subtitles/ocr/wrapper.rs` | ❌ Not Audited | OCR wrapper |
| 87 | `subtitles/ocr/debug.py` | `subtitles/ocr/debug.rs` | ❌ Not Audited | Debug output |
| 88 | `subtitles/ocr/report.py` | `subtitles/ocr/report.rs` | ❌ Not Audited | OCR report |
| 89 | `subtitles/ocr/preview_subprocess.py` | `subtitles/ocr/preview_subprocess.rs` | ❌ Not Audited | Preview subprocess |
| 90 | `subtitles/ocr/unified_subprocess.py` | `subtitles/ocr/unified_subprocess.rs` | ❌ Not Audited | Unified subprocess |
| 91 | `subtitles/ocr/parsers/base.py` | `subtitles/ocr/parsers/base.rs` | ❌ Not Audited | Base parser |
| 92 | `subtitles/ocr/parsers/vobsub.py` | `subtitles/ocr/parsers/vobsub.rs` | ❌ Not Audited | VobSub parser |
| 93 | `subtitles/ocr/backends.py` | *(see notes)* | ❌ Not Audited | May need Rust-native solution |

### 1.15 Subtitles — Diagnostics

| # | Python File | Rust File | Status | Notes |
|---|---|---|---|---|
| 94 | `subtitles/diagnostics/timestamp_debug.py` | `subtitles/diagnostics/timestamp_debug.rs` | ❌ Not Audited | Timestamp debugging |

### 1.16 Chapters

| # | Python File | Rust File | Status | Notes |
|---|---|---|---|---|
| 95 | `chapters/process.py` | `chapters/process.rs` | ❌ Not Audited | Chapter processing |
| 96 | `chapters/keyframes.py` | `chapters/keyframes.rs` | ❌ Not Audited | Keyframe snapping |

### 1.17 Mux

| # | Python File | Rust File | Status | Notes |
|---|---|---|---|---|
| 97 | `mux/options_builder.py` | `mux/options_builder.rs` | ❌ Not Audited | mkvmerge options |

### 1.18 Pipeline & Orchestrator

| # | Python File | Rust File | Status | Notes |
|---|---|---|---|---|
| 98 | `pipeline.py` | `pipeline.rs` | ❌ Not Audited | JobPipeline.run_job |
| 99 | `orchestrator/pipeline.py` | `orchestrator/pipeline.rs` | ❌ Not Audited | Step orchestration |
| 100 | `orchestrator/validation.py` | `orchestrator/validation.rs` | ❌ Not Audited | Input validation |
| 101 | `orchestrator/steps/context.py` | `orchestrator/steps/context.rs` | ❌ Not Audited | Step context |
| 102 | `orchestrator/steps/extract_step.py` | `orchestrator/steps/extract_step.rs` | ❌ Not Audited | Extraction step |
| 103 | `orchestrator/steps/analysis_step.py` | `orchestrator/steps/analysis_step.rs` | ❌ Not Audited | Analysis step |
| 104 | `orchestrator/steps/subtitles_step.py` | `orchestrator/steps/subtitles_step.rs` | ❌ Not Audited | Subtitles step |
| 105 | `orchestrator/steps/chapters_step.py` | `orchestrator/steps/chapters_step.rs` | ❌ Not Audited | Chapters step |
| 106 | `orchestrator/steps/audio_correction_step.py` | `orchestrator/steps/audio_correction_step.rs` | ❌ Not Audited | Audio correction step |
| 107 | `orchestrator/steps/attachments_step.py` | `orchestrator/steps/attachments_step.rs` | ❌ Not Audited | Attachments step |
| 108 | `orchestrator/steps/mux_step.py` | `orchestrator/steps/mux_step.rs` | ❌ Not Audited | Mux step |

### 1.19 Pipeline Components

| # | Python File | Rust File | Status | Notes |
|---|---|---|---|---|
| 109 | `pipeline_components/tool_validator.py` | `pipeline_components/tool_validator.rs` | ❌ Not Audited | Tool path validation |
| 110 | `pipeline_components/sync_planner.py` | `pipeline_components/sync_planner.rs` | ❌ Not Audited | Sync planning |
| 111 | `pipeline_components/sync_executor.py` | `pipeline_components/sync_executor.rs` | ❌ Not Audited | Sync execution |
| 112 | `pipeline_components/output_writer.py` | `pipeline_components/output_writer.rs` | ❌ Not Audited | Output file writing |
| 113 | `pipeline_components/result_auditor.py` | `pipeline_components/result_auditor.rs` | ❌ Not Audited | Result auditing |
| 114 | `pipeline_components/log_manager.py` | `pipeline_components/log_manager.rs` | ❌ Not Audited | Log file management |

### 1.20 Postprocess — Auditors

| # | Python File | Rust File | Status | Notes |
|---|---|---|---|---|
| 115 | `postprocess/finalizer.py` | `postprocess/finalizer.rs` | ❌ Not Audited | Post-mux finalizer |
| 116 | `postprocess/final_auditor.py` | `postprocess/final_auditor.rs` | ❌ Not Audited | Final audit runner |
| 117 | `postprocess/chapter_backup.py` | `postprocess/chapter_backup.rs` | ❌ Not Audited | Chapter backup |
| 118 | `postprocess/auditors/base.py` | `postprocess/auditors/base.rs` | ❌ Not Audited | Base auditor trait |
| 119 | `postprocess/auditors/attachments.py` | `postprocess/auditors/attachments.rs` | ❌ Not Audited | |
| 120 | `postprocess/auditors/audio_channels.py` | `postprocess/auditors/audio_channels.rs` | ❌ Not Audited | |
| 121 | `postprocess/auditors/audio_object_based.py` | `postprocess/auditors/audio_object_based.rs` | ❌ Not Audited | |
| 122 | `postprocess/auditors/audio_quality.py` | `postprocess/auditors/audio_quality.rs` | ❌ Not Audited | |
| 123 | `postprocess/auditors/audio_sync.py` | `postprocess/auditors/audio_sync.rs` | ❌ Not Audited | |
| 124 | `postprocess/auditors/chapters.py` | `postprocess/auditors/chapters.rs` | ❌ Not Audited | |
| 125 | `postprocess/auditors/codec_integrity.py` | `postprocess/auditors/codec_integrity.rs` | ❌ Not Audited | |
| 126 | `postprocess/auditors/dolby_vision.py` | `postprocess/auditors/dolby_vision.rs` | ❌ Not Audited | |
| 127 | `postprocess/auditors/drift_correction.py` | `postprocess/auditors/drift_correction.rs` | ❌ Not Audited | |
| 128 | `postprocess/auditors/frame_audit.py` | `postprocess/auditors/frame_audit.rs` | ❌ Not Audited | |
| 129 | `postprocess/auditors/global_shift.py` | `postprocess/auditors/global_shift.rs` | ❌ Not Audited | |
| 130 | `postprocess/auditors/language_tags.py` | `postprocess/auditors/language_tags.rs` | ❌ Not Audited | |
| 131 | `postprocess/auditors/neural_confidence.py` | `postprocess/auditors/neural_confidence.rs` | ❌ Not Audited | |
| 132 | `postprocess/auditors/stepping_correction.py` | `postprocess/auditors/stepping_correction.rs` | ❌ Not Audited | |
| 133 | `postprocess/auditors/subtitle_clamping.py` | `postprocess/auditors/subtitle_clamping.rs` | ❌ Not Audited | |
| 134 | `postprocess/auditors/subtitle_formats.py` | `postprocess/auditors/subtitle_formats.rs` | ❌ Not Audited | |
| 135 | `postprocess/auditors/track_flags.py` | `postprocess/auditors/track_flags.rs` | ❌ Not Audited | |
| 136 | `postprocess/auditors/track_names.py` | `postprocess/auditors/track_names.rs` | ❌ Not Audited | |
| 137 | `postprocess/auditors/track_order.py` | `postprocess/auditors/track_order.rs` | ❌ Not Audited | |
| 138 | `postprocess/auditors/video_metadata.py` | `postprocess/auditors/video_metadata.rs` | ❌ Not Audited | |

### 1.21 Reporting

| # | Python File | Rust File | Status | Notes |
|---|---|---|---|---|
| 139 | `reporting/report_writer.py` | `reporting/report_writer.rs` | ❌ Not Audited | Report generation |
| 140 | `reporting/debug_manager.py` | `reporting/debug_manager.rs` | ❌ Not Audited | Debug output manager |
| 141 | `reporting/debug_paths.py` | `reporting/debug_paths.rs` | ❌ Not Audited | Debug path resolver |

### 1.22 Job Layouts

| # | Python File | Rust File | Status | Notes |
|---|---|---|---|---|
| 142 | `job_layouts/manager.py` | `job_layouts/manager.rs` | ❌ Not Audited | Layout manager |
| 143 | `job_layouts/persistence.py` | `job_layouts/persistence.rs` | ❌ Not Audited | Layout file I/O |
| 144 | `job_layouts/signature.py` | `job_layouts/signature.rs` | ❌ Not Audited | Structure signatures |
| 145 | `job_layouts/validation.py` | `job_layouts/validation.rs` | ❌ Not Audited | Layout validation |

### 1.23 Standalone Modules

| # | Python File | Rust File | Status | Notes |
|---|---|---|---|---|
| 146 | `favorite_colors.py` | `favorite_colors.rs` | ❌ Not Audited | Color manager |
| 147 | `font_manager.py` | `font_manager.rs` | ❌ Not Audited | Font scanning/replacement |
| 148 | `audit/trail.py` | `audit/trail.rs` | ❌ Not Audited | Audit trail |
| 149 | `system/gpu_env.py` | *(in config/pipeline)* | ❌ Not Audited | GPU environment setup |

---

## PART 2: UI (vsg_qt → crates/vsg_ui)

Python: 41 files | Rust: 34 files (bridges) + 17 QML

### 2.1 Main Window

| # | Python File | Rust File | QML File | Status | Notes |
|---|---|---|---|---|---|
| 1 | `main_window/controller.py` | `bridges/main_controller.rs` | `MainWindow.qml` | ❌ Not Audited | Main app controller |
| 2 | `main_window/window.py` | *(in QML)* | `MainWindow.qml` | ❌ Not Audited | Window layout |

### 2.2 Worker

| # | Python File | Rust File | QML File | Status | Notes |
|---|---|---|---|---|---|
| 3 | `worker/runner.py` | `worker/runner.rs` | — | ❌ Not Audited | Job batch runner |
| 4 | `worker/signals.py` | `bridges/worker_signals.rs` | — | ❌ Not Audited | Worker signal defs |

### 2.3 Add Job Dialog

| # | Python File | Rust File | QML File | Status | Notes |
|---|---|---|---|---|---|
| 5 | `add_job_dialog/ui.py` | `bridges/add_job_logic.rs` | `AddJobDialog.qml` | ❌ Not Audited | Source inputs + discovery |

### 2.4 Job Queue Dialog

| # | Python File | Rust File | QML File | Status | Notes |
|---|---|---|---|---|---|
| 6 | `job_queue_dialog/ui.py` | *(in QML)* | `JobQueueDialog.qml` | ❌ Not Audited | Queue table UI |
| 7 | `job_queue_dialog/logic.py` | `bridges/job_queue_logic.rs` | — | ❌ Not Audited | Queue logic + validation |

### 2.5 Manual Selection Dialog

| # | Python File | Rust File | QML File | Status | Notes |
|---|---|---|---|---|---|
| 8 | `manual_selection_dialog/ui.py` | *(in QML)* | `ManualSelectionDialog.qml` | ❌ Not Audited | Layout builder UI |
| 9 | `manual_selection_dialog/logic.py` | `bridges/manual_selection_logic.rs` | — | ❌ Not Audited | Prepopulate/normalize |
| 10 | `manual_selection_dialog/widgets.py` | `bridges/source_section_logic.rs` | — | ❌ Not Audited | SourceList + FinalList |

### 2.6 Track Widget

| # | Python File | Rust File | QML File | Status | Notes |
|---|---|---|---|---|---|
| 11 | `track_widget/ui.py` | *(in QML)* | `TrackWidget.qml` | ❌ Not Audited | Track row UI |
| 12 | `track_widget/logic.py` | `bridges/track_widget_logic.rs` | — | ❌ Not Audited | Badges, summary, config |
| 13 | `track_widget/helpers.py` | `track_widget/helpers.rs` | — | ❌ Not Audited | Label/summary builders |

### 2.7 Track Settings Dialog

| # | Python File | Rust File | QML File | Status | Notes |
|---|---|---|---|---|---|
| 14 | `track_settings_dialog/ui.py` | *(in QML)* | `TrackSettingsDialog.qml` | ❌ Not Audited | Per-track settings UI |
| 15 | `track_settings_dialog/logic.py` | `bridges/track_settings_logic.rs` | — | ❌ Not Audited | Language codes, init |

### 2.8 Options Dialog

| # | Python File | Rust File | QML File | Status | Notes |
|---|---|---|---|---|---|
| 16 | `options_dialog/ui.py` | *(in QML)* | `OptionsDialog.qml` | ❌ Not Audited | Settings dialog UI |
| 17 | `options_dialog/logic.py` | `bridges/options_logic.rs` | — | ❌ Not Audited | Load/save settings |
| 18 | `options_dialog/tabs.py` | `options_dialog/tabs.rs` | — | ❌ Not Audited | 29K tokens — biggest file |
| 19 | `options_dialog/model_manager_dialog.py` | *(missing)* | *(missing)* | ❌ Not Started | Neural model manager |

### 2.9 Source Settings Dialog

| # | Python File | Rust File | QML File | Status | Notes |
|---|---|---|---|---|---|
| 20 | `source_settings_dialog/dialog.py` | `bridges/source_settings_logic.rs` | `SourceSettingsDialog.qml` | ❌ Not Audited | Correlation settings |

### 2.10 Sync Exclusion Dialog

| # | Python File | Rust File | QML File | Status | Notes |
|---|---|---|---|---|---|
| 21 | `sync_exclusion_dialog/ui.py` | `bridges/sync_exclusion_logic.rs` | `SyncExclusionDialog.qml` | ❌ Not Audited | Style exclusion config |

### 2.11 Resample Dialog

| # | Python File | Rust File | QML File | Status | Notes |
|---|---|---|---|---|---|
| 22 | `resample_dialog/ui.py` | `bridges/resample_logic.rs` | `ResampleDialog.qml` | ❌ Not Audited | Resolution resampling |

### 2.12 Favorites Dialog

| # | Python File | Rust File | QML File | Status | Notes |
|---|---|---|---|---|---|
| 23 | `favorites_dialog/ui.py` | `bridges/favorites_logic.rs` | `FavoritesDialog.qml` | ❌ Not Audited | Color favorites CRUD |

### 2.13 Font Manager Dialog

| # | Python File | Rust File | QML File | Status | Notes |
|---|---|---|---|---|---|
| 24 | `font_manager_dialog/ui.py` | `bridges/font_manager_logic.rs` | `FontManagerDialog.qml` | ❌ Not Audited | Font replacement |

### 2.14 OCR Dictionary Dialog

| # | Python File | Rust File | QML File | Status | Notes |
|---|---|---|---|---|---|
| 25 | `ocr_dictionary_dialog/ui.py` | `bridges/ocr_dictionary_logic.rs` | `OCRDictionaryDialog.qml` | ❌ Not Audited | OCR dict editor |

### 2.15 Report Dialogs

| # | Python File | Rust File | QML File | Status | Notes |
|---|---|---|---|---|---|
| 26 | `report_dialogs/batch_completion_dialog.py` | `bridges/batch_completion_logic.rs` | `BatchCompletionDialog.qml` | ❌ Not Audited | Batch summary |
| 27 | `report_dialogs/report_viewer.py` | `bridges/report_viewer_logic.rs` | `ReportViewerDialog.qml` | ❌ Not Audited | Report viewer |

### 2.16 Subtitle Editor

| # | Python File | Rust File | QML File | Status | Notes |
|---|---|---|---|---|---|
| 28 | `subtitle_editor/editor_window.py` | `bridges/subtitle_editor_logic.rs` | `subtitle_editor/SubtitleEditorWindow.qml` | ❌ Not Audited | Main editor window |
| 29 | `subtitle_editor/events_table.py` | `bridges/events_table_logic.rs` | — | ❌ Not Audited | Events table |
| 30 | `subtitle_editor/video_panel.py` | `bridges/video_panel_logic.rs` | — | ❌ Not Audited | Video panel |
| 31 | `subtitle_editor/tab_panel.py` | *(in QML)* | — | ❌ Not Audited | Tab panel container |
| 32 | `subtitle_editor/tabs/base_tab.py` | *(trait)* | — | ❌ Not Audited | Base tab interface |
| 33 | `subtitle_editor/tabs/styles_tab.py` | `bridges/styles_tab_logic.rs` | — | ❌ Not Audited | Styles tab |
| 34 | `subtitle_editor/tabs/fonts_tab.py` | `bridges/fonts_tab_logic.rs` | — | ❌ Not Audited | Fonts tab |
| 35 | `subtitle_editor/tabs/filtering_tab.py` | `bridges/filtering_tab_logic.rs` | — | ❌ Not Audited | Filtering tab |
| 36 | `subtitle_editor/state/editor_state.py` | `subtitle_editor/state.rs` | — | ❌ Not Audited | Editor state mgmt |
| 37 | `subtitle_editor/state/undo_manager.py` | `subtitle_editor/undo.rs` | — | ❌ Not Audited | Undo/redo |
| 38 | `subtitle_editor/utils/cps.py` | `subtitle_editor/utils/cps.rs` | — | ❌ Not Audited | Chars per second |
| 39 | `subtitle_editor/utils/time_format.py` | `subtitle_editor/utils/time_format.rs` | — | ❌ Not Audited | Time formatting |
| 40 | `subtitle_editor/subprocess_launcher.py` | *(in editor logic)* | — | ❌ Not Audited | External app launch |
| 41 | `subtitle_editor/player/player_thread.py` | *(in video_panel)* | — | ❌ Not Audited | Video playback |

---

## Progress Summary

| Section | Total Files | Done | In Progress | Not Audited |
|---|---|---|---|---|
| **Core** | 149 | 18 | 2 | 129 |
| **UI** | 41 | 0 | 0 | 41 |
| **TOTAL** | **190** | **18** | **2** | **170** |

> Last updated: 2026-03-23
