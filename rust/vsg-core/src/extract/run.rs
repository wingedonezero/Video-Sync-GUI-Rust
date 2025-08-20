use std::fs;
use std::path::Path;
use std::process::Command;
use crate::error::VsgError;
use crate::types::Source;

fn extension_for(codec: &str, track_type: &str) -> &'static str {
    match track_type {
        "audio" => match codec {
            "A_AAC" => ".aac",
            "A_AC3" => ".ac3",
            "A_EAC3" => ".eac3",
            "A_DTS" => ".dts",
            "A_TRUEHD" => ".thd",
            "A_FLAC" => ".flac",
            "A_OPUS" => ".opus",
            "A_VORBIS" => ".ogg",
            "PCM" => ".wav",
            _ => ".audio",
        },
        "subtitles" => match codec {
            "S_TEXT/ASS" => ".ass",
            "S_TEXT/UTF8" => ".srt",
            "S_HDMV/PGS" => ".sup",
            _ => ".sub",
        },
        "video" => match codec {
            "V_MPEG4/ISO/AVC" => ".h264",
            "V_MPEGH/ISO/HEVC" => ".hevc",
            "V_MS/VFW/FOURCC" => ".vc1",
            "V_MPEG2" => ".m2v",
            _ => ".video",
        },
        _ => ".bin",
    }
}

pub fn run_extract(
    source: Source,
    file_path: &str,
    entries: &[(u32, String, String)], // (track_id, lang, codec)
    work_dir: &Path,
) -> Result<(), VsgError> {
    let out_dir = work_dir.join(match source {
        Source::Ref => "ref",
        Source::Sec => "sec",
        Source::Ter => "ter",
    });
    fs::create_dir_all(&out_dir)?;

    for (track_id, lang, codec) in entries {
        let ext = extension_for(codec, if codec.starts_with("A_") || codec == "PCM" {
            "audio"
        } else if codec.starts_with("S_") {
            "subtitles"
        } else if codec.starts_with("V_") {
            "video"
        } else {
            "bin"
        });
        let out_file = out_dir.join(format!("{}_{}.{}{}", format!("{:03}", track_id), "track", lang, ext));
        let cmd = Command::new("mkvextract")
            .arg("tracks")
            .arg(file_path)
            .arg(format!("{}:{}", track_id, out_file.to_string_lossy()))
            .status()?;
        if !cmd.success() {
            return Err(VsgError::ExecFailed("mkvextract".into()));
        }
        println!("EXTRACTED {}", out_file.display());
    }

    Ok(())
}
