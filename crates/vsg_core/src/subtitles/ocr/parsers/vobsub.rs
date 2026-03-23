//! VobSub (.sub/.idx) Parser
//!
//! Extracts subtitle images from DVD VobSub format files.
//! Based on SubtitleEdit's VobSub parsing logic, ported from Python.
//!
//! VobSub format consists of two files:
//!     - .idx: Text index file with timestamps and byte offsets
//!     - .sub: Binary file containing MPEG-2 PES packets with subtitle bitmaps
//!
//! The subtitle data is encoded as run-length encoded (RLE) bitmaps with a
//! 4-color palette per subtitle.
//!
//! RLE Encoding formats (from SubtitleEdit):
//!     Value      Bits   Format
//!     1-3        4      nncc               (half a byte)
//!     4-15       8      00nnnncc           (one byte)
//!     16-63     12      0000nnnnnncc       (one and a half byte)
//!     64-255    16      000000nnnnnnnncc   (two bytes)

use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use image::{RgbaImage, Rgba};
use tracing::{debug, warn};

use super::base::{ParseResult, SubtitleImage, SubtitleImageParser};

/// Minimum gap between consecutive subtitles (ms).
const MIN_GAP_MS: i64 = 24;
/// Maximum subtitle duration (ms).
const MAX_DURATION_MS: i64 = 8000;
/// Minimum subtitle duration (ms).
const MIN_DURATION_MS: i64 = 1000;
/// Default duration for last subtitle with no duration info (ms).
const DEFAULT_LAST_DURATION_MS: i64 = 3000;

/// Parsed entry from .idx file.
#[derive(Debug, Clone)]
struct IdxEntry {
    timestamp_ms: i64,
    file_position: u64,
}

/// Header information from .idx file.
#[derive(Debug, Clone)]
struct VobSubHeader {
    size_x: u32,
    size_y: u32,
    org_x: u32,
    org_y: u32,
    /// RGB tuples (16 colors)
    palette: Vec<(u8, u8, u8)>,
    language: String,
    language_index: u32,
}

impl Default for VobSubHeader {
    fn default() -> Self {
        // Default grayscale palette
        let palette: Vec<(u8, u8, u8)> = (0..16)
            .map(|i| {
                let v = (i * 17) as u8;
                (v, v, v)
            })
            .collect();
        Self {
            size_x: 720,
            size_y: 480,
            org_x: 0,
            org_y: 0,
            palette,
            language: "en".to_string(),
            language_index: 0,
        }
    }
}

/// Result of parsing control sequence.
#[derive(Debug)]
struct ControlResult {
    x1: u32,
    y1: u32,
    x2: u32,
    y2: u32,
    color_indices: [usize; 4],
    alpha_values: [u8; 4],
    top_field_offset: usize,
    bottom_field_offset: usize,
    forced: bool,
    duration_ms: i64,
}

/// Result of RLE decoding a single run.
#[derive(Debug)]
struct RleDecodeResult {
    index_increment: usize,
    run_length: usize,
    color: usize,
    only_half: bool,
    rest_of_line: bool,
}

/// Parser for VobSub (.sub/.idx) subtitle format.
///
/// Extracts subtitle bitmaps with timing and position information.
pub struct VobSubParser;

impl VobSubParser {
    pub fn new() -> Self {
        Self
    }

    /// Parse the .idx index file.
    fn parse_idx(&self, idx_path: &Path) -> Result<(VobSubHeader, Vec<IdxEntry>), String> {
        let file = File::open(idx_path)
            .map_err(|e| format!("Failed to open IDX file: {}", e))?;
        let reader = BufReader::new(file);

        let mut header = VobSubHeader::default();
        let mut entries = Vec::new();

        let size_re = regex::Regex::new(r"size:\s*(\d+)x(\d+)").unwrap();
        let org_re = regex::Regex::new(r"org:\s*(\d+),\s*(\d+)").unwrap();
        let id_re = regex::Regex::new(r"id:\s*(\w+),\s*index:\s*(\d+)").unwrap();
        let ts_re = regex::Regex::new(
            r"timestamp:\s*(\d+):(\d+):(\d+):(\d+),\s*filepos:\s*([0-9a-fA-F]+)"
        ).unwrap();

        for line in reader.lines() {
            let line = match line {
                Ok(l) => l,
                Err(_) => continue,
            };
            let line = line.trim().to_string();

            if line.starts_with("size:") {
                if let Some(caps) = size_re.captures(&line) {
                    header.size_x = caps[1].parse().unwrap_or(720);
                    header.size_y = caps[2].parse().unwrap_or(480);
                }
            } else if line.starts_with("org:") {
                if let Some(caps) = org_re.captures(&line) {
                    header.org_x = caps[1].parse().unwrap_or(0);
                    header.org_y = caps[2].parse().unwrap_or(0);
                }
            } else if line.starts_with("palette:") {
                let palette_str = &line[8..].trim();
                let colors: Vec<&str> = palette_str.split(',').collect();
                header.palette.clear();
                for color in colors.iter().take(16) {
                    let color = color.trim();
                    if !color.is_empty() {
                        if let Ok(rgb) = u32::from_str_radix(color, 16) {
                            let r = ((rgb >> 16) & 0xFF) as u8;
                            let g = ((rgb >> 8) & 0xFF) as u8;
                            let b = (rgb & 0xFF) as u8;
                            header.palette.push((r, g, b));
                        } else {
                            header.palette.push((128, 128, 128));
                        }
                    }
                }
                // Pad to 16 colors if needed
                while header.palette.len() < 16 {
                    header.palette.push((128, 128, 128));
                }
            } else if line.starts_with("id:") {
                if let Some(caps) = id_re.captures(&line) {
                    header.language = caps[1].to_string();
                    header.language_index = caps[2].parse().unwrap_or(0);
                }
            } else if line.starts_with("timestamp:") {
                if let Some(caps) = ts_re.captures(&line) {
                    let hours: i64 = caps[1].parse().unwrap_or(0);
                    let minutes: i64 = caps[2].parse().unwrap_or(0);
                    let seconds: i64 = caps[3].parse().unwrap_or(0);
                    let ms: i64 = caps[4].parse().unwrap_or(0);
                    let filepos = u64::from_str_radix(&caps[5], 16).unwrap_or(0);

                    let timestamp_ms = hours * 3_600_000 + minutes * 60_000 + seconds * 1_000 + ms;
                    entries.push(IdxEntry { timestamp_ms, file_position: filepos });
                }
            }
        }

        Ok((header, entries))
    }

    /// Parse a single subtitle from the .sub file.
    fn parse_subtitle(
        &self,
        sub_file: &mut File,
        entry: &IdxEntry,
        index: usize,
        header: &VobSubHeader,
        all_entries: &[IdxEntry],
    ) -> Option<SubtitleImage> {
        sub_file.seek(SeekFrom::Start(entry.file_position)).ok()?;

        // Read MPEG-2 PES packets until we have complete subtitle data
        let subtitle_data = self.read_pes_packets(sub_file);
        if subtitle_data.len() < 4 {
            return None;
        }

        // Parse the subtitle packet (includes duration from control sequence)
        let (image, x, y, forced, duration_ms) = self.decode_subtitle_packet(&subtitle_data, header)?;

        // Calculate end time using duration from control sequence
        let has_spu_duration = duration_ms > 0;
        let mut end_ms;

        if has_spu_duration {
            end_ms = entry.timestamp_ms + duration_ms;
            debug!("Subtitle {}: using SPU duration {}ms", index, duration_ms);
        } else if index + 1 < all_entries.len() {
            end_ms = all_entries[index + 1].timestamp_ms - MIN_GAP_MS;
            debug!("Subtitle {}: no SPU duration, using next start - {}ms gap", index, MIN_GAP_MS);
        } else {
            end_ms = entry.timestamp_ms + DEFAULT_LAST_DURATION_MS;
            debug!("Subtitle {}: no SPU duration, using {}ms default", index, DEFAULT_LAST_DURATION_MS);
        }

        // Enforce duration limits
        let calculated_duration = end_ms - entry.timestamp_ms;

        // Cap maximum duration
        if calculated_duration > MAX_DURATION_MS {
            end_ms = entry.timestamp_ms + MAX_DURATION_MS;
            debug!("Subtitle {}: capped duration from {}ms to {}ms", index, calculated_duration, MAX_DURATION_MS);
        }

        // Enforce minimum duration ONLY for fallback durations (no SPU stop command)
        if !has_spu_duration && calculated_duration < MIN_DURATION_MS {
            if index + 1 < all_entries.len() {
                let max_end = all_entries[index + 1].timestamp_ms - MIN_GAP_MS;
                end_ms = (entry.timestamp_ms + MIN_DURATION_MS).min(max_end);
            } else {
                end_ms = entry.timestamp_ms + MIN_DURATION_MS;
            }
            debug!("Subtitle {}: extended short duration from {}ms", index, calculated_duration);
        }

        let (w, h) = image.dimensions();
        let palette_rgba: Vec<(u8, u8, u8, u8)> = header.palette.iter()
            .map(|&(r, g, b)| (r, g, b, 255))
            .collect();

        Some(SubtitleImage {
            index,
            start_ms: entry.timestamp_ms,
            end_ms,
            width: w,
            height: h,
            image,
            x,
            y,
            frame_width: header.size_x,
            frame_height: header.size_y,
            is_forced: forced,
            palette: Some(palette_rgba),
        })
    }

    /// Read MPEG-2 PES packets containing subtitle data.
    ///
    /// VobSub uses MPEG-2 Program Stream format with subtitle data
    /// in private stream 1 (0xBD).
    fn read_pes_packets(&self, f: &mut File) -> Vec<u8> {
        let mut data = Vec::new();
        let max_read: usize = 65536 * 10; // Safety limit
        let mut bytes_read: usize = 0;

        while bytes_read < max_read {
            // Read pack header start code
            let mut start_code = [0u8; 4];
            if f.read_exact(&mut start_code).is_err() {
                break;
            }

            // Check for pack start code (0x000001BA)
            if start_code == [0x00, 0x00, 0x01, 0xBA] {
                // Skip pack header (MPEG-2 pack header is variable length)
                let mut pack_header = [0u8; 10];
                if f.read_exact(&mut pack_header).is_err() {
                    break;
                }
                // Check stuffing length in last byte
                let stuffing = (pack_header[9] & 0x07) as usize;
                if stuffing > 0 {
                    let mut stuff_buf = vec![0u8; stuffing];
                    if f.read_exact(&mut stuff_buf).is_err() {
                        break;
                    }
                }
                bytes_read += 14 + stuffing;
                continue;
            }

            // Check for PES packet start code (0x000001XX)
            if start_code[0] == 0x00 && start_code[1] == 0x00 && start_code[2] == 0x01 {
                let stream_id = start_code[3];

                // Read PES packet length
                let mut length_bytes = [0u8; 2];
                if f.read_exact(&mut length_bytes).is_err() {
                    break;
                }
                let packet_length = u16::from_be_bytes(length_bytes) as usize;
                bytes_read += 6;

                if packet_length == 0 {
                    break;
                }

                // Read PES packet data
                let mut packet_data = vec![0u8; packet_length];
                if f.read_exact(&mut packet_data).is_err() {
                    break;
                }
                bytes_read += packet_length;

                // Private stream 1 (0xBD) contains subtitles
                if stream_id == 0xBD && packet_data.len() >= 3 {
                    let pes_header_data_length = packet_data[2] as usize;
                    let payload_start = 3 + pes_header_data_length;

                    if payload_start < packet_data.len() {
                        // Check substream ID (subtitle streams are 0x20-0x3F)
                        let substream_id = packet_data[payload_start];
                        if (0x20..=0x3F).contains(&substream_id) {
                            // Add subtitle payload (skip substream ID byte)
                            data.extend_from_slice(&packet_data[payload_start + 1..]);
                        }
                    }
                }

                // Check for end code
                if stream_id == 0xB9 {
                    break;
                }
            } else {
                // Not a valid start code, we may be at end of subtitle data
                break;
            }
        }

        data
    }

    /// Decode subtitle packet into bitmap image.
    ///
    /// Returns (image, x_position, y_position, is_forced, duration_ms).
    fn decode_subtitle_packet(
        &self,
        data: &[u8],
        header: &VobSubHeader,
    ) -> Option<(RgbaImage, u32, u32, bool, i64)> {
        if data.len() < 4 {
            return None;
        }

        // First two bytes are total size, next two are offset to control sequence
        let ctrl_offset = u16::from_be_bytes([data[2], data[3]]) as usize;

        if ctrl_offset >= data.len() {
            return None;
        }

        // Parse control sequence to get display parameters and duration
        let ctrl_result = self.parse_control_sequence(data, ctrl_offset, header)?;

        let width = ctrl_result.x2.saturating_sub(ctrl_result.x1) + 1;
        let height = ctrl_result.y2.saturating_sub(ctrl_result.y1) + 1;

        if width == 0 || height == 0 || width > 2000 || height > 2000 {
            return None;
        }

        // Decode RLE data into bitmap
        let image = self.decode_rle_image(
            data,
            ctrl_result.top_field_offset,
            ctrl_result.bottom_field_offset,
            width as usize,
            height as usize,
            &ctrl_result.color_indices,
            &ctrl_result.alpha_values,
            &header.palette,
        );

        Some((image, ctrl_result.x1, ctrl_result.y1, ctrl_result.forced, ctrl_result.duration_ms))
    }

    /// Parse subtitle control sequence.
    ///
    /// The control sequence contains timing info in SP_DCSQ_STM field.
    /// When we see command 0x02 (stop display), the delay value tells us
    /// the subtitle duration.
    fn parse_control_sequence(
        &self,
        data: &[u8],
        mut offset: usize,
        header: &VobSubHeader,
    ) -> Option<ControlResult> {
        let mut result = ControlResult {
            x1: 0,
            y1: 0,
            x2: header.size_x.saturating_sub(1),
            y2: header.size_y.saturating_sub(1),
            color_indices: [0, 1, 2, 3],
            alpha_values: [0, 15, 15, 15],
            top_field_offset: 4,
            bottom_field_offset: 4,
            forced: false,
            duration_ms: 0,
        };

        let initial_offset = offset;

        while offset < data.len().saturating_sub(3) {
            // Read SP_DCSQ_STM delay field (2 bytes, big-endian)
            if offset + 2 > data.len() {
                break;
            }
            let delay_ticks = u16::from_be_bytes([data[offset], data[offset + 1]]);
            offset += 2;

            if offset + 2 > data.len() {
                break;
            }

            // Read next control sequence offset
            let next_ctrl = u16::from_be_bytes([data[offset], data[offset + 1]]) as usize;
            offset += 2;

            // Process commands until end
            while offset < data.len() {
                let cmd = data[offset];
                offset += 1;

                match cmd {
                    0x00 => {
                        // Forced display
                        result.forced = true;
                    }
                    0x01 => {
                        // Start display
                    }
                    0x02 => {
                        // Stop display - the delay_ticks tells us when to stop
                        // Convert to milliseconds: (ticks * 1024) / 90
                        result.duration_ms = (delay_ticks as i64 * 1024) / 90;
                    }
                    0x03 => {
                        // Palette - 4 nibbles map to color slots 3,2,1,0
                        if offset + 2 <= data.len() {
                            let b1 = data[offset];
                            let b2 = data[offset + 1];
                            result.color_indices = [
                                (b2 & 0x0F) as usize,        // slot 0 (background)
                                ((b2 >> 4) & 0x0F) as usize,  // slot 1 (text/pattern)
                                (b1 & 0x0F) as usize,        // slot 2 (emphasis1/outline)
                                ((b1 >> 4) & 0x0F) as usize,  // slot 3 (emphasis2/anti-alias)
                            ];
                            offset += 2;
                        }
                    }
                    0x04 => {
                        // Alpha channel - same nibble order as palette
                        if offset + 2 <= data.len() {
                            let b1 = data[offset];
                            let b2 = data[offset + 1];
                            result.alpha_values = [
                                b2 & 0x0F,        // slot 0 alpha
                                (b2 >> 4) & 0x0F,  // slot 1 alpha
                                b1 & 0x0F,        // slot 2 alpha
                                (b1 >> 4) & 0x0F,  // slot 3 alpha
                            ];
                            offset += 2;
                        }
                    }
                    0x05 => {
                        // Coordinates
                        if offset + 6 <= data.len() {
                            result.x1 = ((data[offset] as u32) << 4) | (((data[offset + 1] >> 4) & 0x0F) as u32);
                            result.x2 = (((data[offset + 1] & 0x0F) as u32) << 8) | (data[offset + 2] as u32);
                            result.y1 = ((data[offset + 3] as u32) << 4) | (((data[offset + 4] >> 4) & 0x0F) as u32);
                            result.y2 = (((data[offset + 4] & 0x0F) as u32) << 8) | (data[offset + 5] as u32);
                            offset += 6;
                        }
                    }
                    0x06 => {
                        // RLE offsets (top and bottom fields)
                        if offset + 4 <= data.len() {
                            result.top_field_offset = u16::from_be_bytes([data[offset], data[offset + 1]]) as usize;
                            result.bottom_field_offset = u16::from_be_bytes([data[offset + 2], data[offset + 3]]) as usize;
                            offset += 4;
                        }
                    }
                    0xFF => {
                        // End of control sequence
                        break;
                    }
                    _ => {
                        // Unknown command, skip
                    }
                }
            }

            // Check if we should continue to next control sequence
            if next_ctrl == initial_offset {
                break;
            }
            offset = next_ctrl;
        }

        Some(result)
    }

    /// Decode RLE-encoded subtitle image.
    ///
    /// VobSub uses interlaced RLE with separate top and bottom fields.
    /// Uses luminance thresholding to produce grayscale output.
    fn decode_rle_image(
        &self,
        data: &[u8],
        top_offset: usize,
        bottom_offset: usize,
        width: usize,
        height: usize,
        color_indices: &[usize; 4],
        alpha_values: &[u8; 4],
        palette: &[(u8, u8, u8)],
    ) -> RgbaImage {
        debug!("VobSub decode: color_indices={:?}, alpha_values={:?}", color_indices, alpha_values);

        let palette_colors: Vec<(u8, u8, u8)> = color_indices.iter()
            .map(|&idx| {
                if idx < palette.len() {
                    palette[idx]
                } else {
                    (0, 0, 0)
                }
            })
            .collect();
        debug!("VobSub decode: palette_colors={:?}", palette_colors);

        // Calculate luminance for each color position
        let luminances: Vec<f64> = color_indices.iter()
            .map(|&idx| {
                if idx < palette.len() {
                    let (r, g, b) = palette[idx];
                    0.299 * r as f64 + 0.587 * g as f64 + 0.114 * b as f64
                } else {
                    128.0
                }
            })
            .collect();
        debug!("VobSub decode: luminances={:?}", luminances);

        // Thresholds for determining text pixels (subtile-ocr approach)
        let alpha_threshold: u8 = 1;
        let luma_threshold: f64 = 100.0;

        // Build color lookup - determine if each position is TEXT or BACKGROUND
        let mut is_text = Vec::with_capacity(4);
        for i in 0..4 {
            if i == 0 {
                is_text.push(false);
            } else if alpha_values[i] >= alpha_threshold && luminances[i] > luma_threshold {
                is_text.push(true);
            } else {
                is_text.push(false);
            }
        }

        // FALLBACK: If no positions passed luminance threshold, fall back to alpha-only
        if !is_text.iter().any(|&t| t) {
            debug!("VobSub decode: No bright colors found, falling back to alpha-only");
            is_text.clear();
            for i in 0..4 {
                if i == 0 {
                    is_text.push(false);
                } else if alpha_values[i] >= alpha_threshold {
                    is_text.push(true);
                } else {
                    is_text.push(false);
                }
            }
        }

        debug!("VobSub decode: is_text={:?}", is_text);

        // Convert is_text to grayscale values: True=0 (black), False=255 (white)
        let colors: Vec<u8> = is_text.iter().map(|&t| if t { 0 } else { 255 }).collect();

        // Create grayscale image (white background)
        let mut gray_image = vec![255u8; width * height];

        // Decode top field (even lines: 0, 2, 4, ...)
        self.decode_rle_field_grayscale(
            data, top_offset, &mut gray_image, 0, 2, width, height, &colors,
        );

        // Decode bottom field (odd lines: 1, 3, 5, ...)
        self.decode_rle_field_grayscale(
            data, bottom_offset, &mut gray_image, 1, 2, width, height, &colors,
        );

        // Convert grayscale to RGBA (white bg, black text with full opacity)
        let mut rgba = RgbaImage::new(width as u32, height as u32);
        for y in 0..height {
            for x in 0..width {
                let g = gray_image[y * width + x];
                rgba.put_pixel(x as u32, y as u32, Rgba([g, g, g, 255]));
            }
        }

        rgba
    }

    /// Decode a single RLE run from the data.
    ///
    /// Based on SubtitleEdit's DecodeRle algorithm.
    fn decode_rle(
        &self,
        data: &[u8],
        index: usize,
        only_half: bool,
    ) -> RleDecodeResult {
        // Safety check
        if index + 2 >= data.len() {
            return RleDecodeResult {
                index_increment: 0,
                run_length: 0,
                color: 0,
                only_half,
                rest_of_line: true,
            };
        }

        let mut b1 = data[index];
        let mut b2 = data[index + 1];

        // If we're at a half-byte position, reconstruct the bytes
        if only_half {
            if index + 2 >= data.len() {
                return RleDecodeResult {
                    index_increment: 0,
                    run_length: 0,
                    color: 0,
                    only_half,
                    rest_of_line: true,
                };
            }
            let b3 = data[index + 2];
            b1 = ((b1 & 0x0F) << 4) | ((b2 & 0xF0) >> 4);
            b2 = ((b2 & 0x0F) << 4) | ((b3 & 0xF0) >> 4);
        }

        // 16-bit code: 000000nnnnnnnncc (two bytes, 64-255 pixels)
        if b1 >> 2 == 0 {
            let run_length = ((b1 as usize) << 6) | ((b2 as usize) >> 2);
            let color = (b2 & 0x03) as usize;
            let mut rest_of_line = false;
            if run_length == 0 {
                rest_of_line = true;
                if only_half {
                    return RleDecodeResult {
                        index_increment: 3,
                        run_length,
                        color,
                        only_half: false,
                        rest_of_line,
                    };
                }
            }
            return RleDecodeResult {
                index_increment: 2,
                run_length,
                color,
                only_half,
                rest_of_line,
            };
        }

        // 12-bit code: 0000nnnnnncc (one and a half bytes, 16-63 pixels)
        if b1 >> 4 == 0 {
            let run_length = ((b1 as usize) << 2) | ((b2 as usize) >> 6);
            let color = ((b2 & 0x30) >> 4) as usize;
            if only_half {
                return RleDecodeResult {
                    index_increment: 2,
                    run_length,
                    color,
                    only_half: false,
                    rest_of_line: false,
                };
            }
            return RleDecodeResult {
                index_increment: 1,
                run_length,
                color,
                only_half: true,
                rest_of_line: false,
            };
        }

        // 8-bit code: 00nnnncc (one byte, 4-15 pixels)
        if b1 >> 6 == 0 {
            let run_length = (b1 >> 2) as usize;
            let color = (b1 & 0x03) as usize;
            return RleDecodeResult {
                index_increment: 1,
                run_length,
                color,
                only_half,
                rest_of_line: false,
            };
        }

        // 4-bit code: nncc (half a byte, 1-3 pixels)
        let run_length = (b1 >> 6) as usize;
        let color = ((b1 & 0x30) >> 4) as usize;

        if only_half {
            RleDecodeResult {
                index_increment: 1,
                run_length,
                color,
                only_half: false,
                rest_of_line: false,
            }
        } else {
            RleDecodeResult {
                index_increment: 0,
                run_length,
                color,
                only_half: true,
                rest_of_line: false,
            }
        }
    }

    /// Decode one RLE field directly to grayscale image.
    fn decode_rle_field_grayscale(
        &self,
        data: &[u8],
        offset: usize,
        image: &mut [u8],
        start_line: usize,
        line_step: usize,
        width: usize,
        height: usize,
        colors: &[u8],
    ) {
        if offset >= data.len() {
            return;
        }

        let mut index = offset;
        let mut only_half = false;
        let mut x = 0usize;
        let mut y = start_line;

        while y < height && index + 2 < data.len() {
            let rle = self.decode_rle(data, index, only_half);
            index += rle.index_increment;
            only_half = rle.only_half;

            let mut run_length = rle.run_length;

            // If end of line, fill rest with this color
            if rle.rest_of_line {
                run_length = width.saturating_sub(x);
            }

            // Get grayscale value for this color index
            let gray = if rle.color < colors.len() { colors[rle.color] } else { 255 };

            // Draw pixels for this run
            for _ in 0..run_length {
                if x >= width {
                    // Line wrap - align to byte boundary
                    if only_half {
                        only_half = false;
                        index += 1;
                    }
                    x = 0;
                    y += line_step;
                    break;
                }

                if y < height {
                    image[y * width + x] = gray;
                }
                x += 1;
            }

            // Check if we naturally hit end of line
            if x >= width {
                if only_half {
                    only_half = false;
                    index += 1;
                }
                x = 0;
                y += line_step;
            }
        }
    }
}

impl SubtitleImageParser for VobSubParser {
    fn can_parse(&self, file_path: &Path) -> bool {
        let suffix = file_path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        match suffix.as_str() {
            "idx" => file_path.with_extension("sub").exists(),
            "sub" => file_path.with_extension("idx").exists(),
            _ => false,
        }
    }

    fn parse(&self, file_path: &Path, _work_dir: Option<&Path>) -> ParseResult {
        let mut result = ParseResult::default();

        // Normalize to .idx path
        let suffix = file_path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        let (idx_path, sub_path) = if suffix == "sub" {
            (file_path.with_extension("idx"), file_path.to_path_buf())
        } else {
            (file_path.to_path_buf(), file_path.with_extension("sub"))
        };

        // Verify both files exist
        if !idx_path.exists() {
            result.errors.push(format!("IDX file not found: {}", idx_path.display()));
            return result;
        }
        if !sub_path.exists() {
            result.errors.push(format!("SUB file not found: {}", sub_path.display()));
            return result;
        }

        // Parse .idx file for header and entries
        let (header, entries) = match self.parse_idx(&idx_path) {
            Ok(v) => v,
            Err(e) => {
                result.errors.push(format!("Failed to parse VobSub: {}", e));
                return result;
            }
        };

        result.format_info.insert("format".into(), "VobSub".into());
        result.format_info.insert("frame_size".into(), format!("{}x{}", header.size_x, header.size_y));
        result.format_info.insert("language".into(), header.language.clone());
        result.format_info.insert("subtitle_count".into(), entries.len().to_string());

        if entries.is_empty() {
            result.warnings.push("No subtitle entries found in IDX file".into());
            return result;
        }

        // Parse .sub file for actual subtitle data
        let mut sub_file = match File::open(&sub_path) {
            Ok(f) => f,
            Err(e) => {
                result.errors.push(format!("Failed to open SUB file: {}", e));
                return result;
            }
        };

        for (i, entry) in entries.iter().enumerate() {
            match self.parse_subtitle(&mut sub_file, entry, i, &header, &entries) {
                Some(subtitle) => result.subtitles.push(subtitle),
                None => {
                    result.warnings.push(format!("Failed to parse subtitle {}", i));
                }
            }
        }

        result
    }
}
