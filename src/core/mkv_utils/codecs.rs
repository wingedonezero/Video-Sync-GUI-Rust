// src/core/mkv_utils/codecs.rs

/// Map MKV track type + codec_id to an output file extension (Python parity).
pub fn ext_for_codec(ttype: &str, codec_id: &str) -> &'static str {
    let cid = codec_id.to_uppercase();
    match ttype {
        "video" => {
            if cid.contains("V_MPEGH/ISO/HEVC") { "h265" }
            else if cid.contains("V_MPEG4/ISO/AVC") { "h24" }
            else if cid.contains("V_MPEG1/2") { "mpg" }
            else if cid.contains("V_VP9") { "vp9" }
            else if cid.contains("V_AV1") { "av1" }
            else { "bin" }
        }
        "audio" => {
            if cid.contains("A_TRUEHD") { "thd" }
            else if cid.contains("A_EAC3") { "eac3" }
            else if cid.contains("A_AC3") { "ac3" }
            else if cid.contains("A_DTS") { "dts" }
            else if cid.contains("A_AAC") { "aac" }
            else if cid.contains("A_FLAC") { "flac" }
            else if cid.contains("A_OPUS") { "opus" }
            else if cid.contains("A_VORBIS") { "ogg" }
            else if cid.contains("A_PCM") { "wav" }
            else { "bin" }
        }
        "subtitles" => {
            if cid.contains("S_TEXT/ASS") { "ass" }
            else if cid.contains("S_TEXT/SSA") { "ssa" }
            else if cid.contains("S_TEXT/UTF8") { "srt" }
            else if cid.contains("S_HDMV/PGS") { "sup" }
            else if cid.contains("S_VOBSUB") { "sub" }
            else { "sub" }
        }
        _ => "bin",
    }
}

/// Choose PCM codec for A_MS/ACM fallback by bit depth (Python parity).
pub fn pcm_codec_from_bit_depth(bit_depth: Option<i64>) -> &'static str {
    let bd = bit_depth.unwrap_or(16);
    if bd >= 64 { "pcm_f64le" }
    else if bd >= 32 { "pcm_s32le" }
    else if bd >= 24 { "pcm_s24le" }
    else { "pcm_s16le" }
}
