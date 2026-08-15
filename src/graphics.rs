//! iTerm2's inline image protocol. Writes straight to the terminal after
//! ratatui has flushed its frame, painting over the half-block rows that
//! reserved the space.

use std::{env, io::Write};

use image::{
    ExtendedColorType, ImageEncoder, RgbImage,
    codecs::png::{CompressionType, FilterType, PngEncoder},
};

/// WezTerm and Rio speak the same protocol. An allowlist rather than a probe:
/// a terminal that doesn't parse the sequence would dump the whole base64
/// payload on screen, so anything unrecognised keeps the half-blocks.
///
/// tmux (`TMUX`) and screen (`STY`) don't pass it through — tmux would need
/// passthrough wrapping and `allow-passthrough on` — so they stay on half-blocks
/// even under a terminal that supports it. `LC_TERMINAL` is what makes this work
/// over ssh: iTerm2 sets it precisely because ssh forwards `LC_*` but not
/// `TERM_PROGRAM`.
pub fn supported() -> bool {
    supported_in(
        &env::var("TERM_PROGRAM").unwrap_or_default(),
        &env::var("LC_TERMINAL").unwrap_or_default(),
        env::var_os("TMUX").is_some(),
        env::var_os("STY").is_some(),
        env::var_os("MDVIEW_NO_INLINE_IMAGES").is_some(),
    )
}

/// Split out from the environment lookup so the rules are testable: setting
/// env vars in a test is process-global and races the other tests.
fn supported_in(term: &str, lc_terminal: &str, tmux: bool, screen: bool, off: bool) -> bool {
    if off || tmux || screen {
        return false;
    }
    matches!(term, "iTerm.app" | "WezTerm" | "rio") || lc_terminal == "iTerm2"
}

/// Encodes cell rows `rows` of `pixels`. Split from `place` so a scroll that
/// moves an image without changing its crop can reuse the bytes.
pub fn encode(pixels: &RgbImage, cells: (u16, u16), rows: (u16, u16)) -> std::io::Result<Vec<u8>> {
    // rows of an RgbImage are contiguous, so a cell-row range is a plain slice
    let px_per_row = pixels.height() / cells.1.max(1) as u32;
    let stride = pixels.width() as usize * 3;
    let (first, count) = (rows.0 as u32 * px_per_row, rows.1 as u32 * px_per_row);
    let buf = &pixels.as_raw()[first as usize * stride..(first + count) as usize * stride];
    // Adaptive filtering is what makes this affordable: on a photo it is ~4x
    // smaller than NoFilter for ~1ms more, while CompressionType::Default costs
    // 140ms and is far too slow to run on every scroll step.
    let mut png = Vec::new();
    PngEncoder::new_with_quality(&mut png, CompressionType::Fast, FilterType::Adaptive)
        .write_image(buf, pixels.width(), count, ExtendedColorType::Rgb8)
        .map_err(std::io::Error::other)?;
    Ok(png)
}

/// Draws an encoded image into a `cells_w` x `rows_h` box with its top-left at
/// (`x`, `y`), 0-indexed. The protocol has no clipping and an image running past
/// the last row scrolls the alternate screen, so the caller crops to what is on
/// screen and the box shrinks to match.
pub fn place<W: Write>(
    out: &mut W,
    x: u16,
    y: u16,
    cells_w: u16,
    rows_h: u16,
    png: &[u8],
) -> std::io::Result<()> {
    // aspect is already exact, so stretching to the box is a no-op that also
    // keeps cropped slices filling their reduced box
    write!(
        out,
        "\x1b7\x1b[{};{}H\x1b]1337;File=inline=1;width={};height={};preserveAspectRatio=0:",
        y + 1,
        x + 1,
        cells_w,
        rows_h,
    )?;
    out.write_all(encode_base64(png).as_bytes())?;
    out.write_all(b"\x07\x1b8")
}

fn encode_base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = u32::from(c[0]) << 16
            | u32::from(c.get(1).copied().unwrap_or(0)) << 8
            | u32::from(c.get(2).copied().unwrap_or(0));
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if c.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{encode_base64, supported_in};

    #[test]
    fn only_terminals_that_parse_the_sequence_are_opted_in() {
        // an unrecognised terminal would print the whole base64 payload
        for term in ["iTerm.app", "WezTerm", "rio"] {
            assert!(supported_in(term, "", false, false, false), "{term}");
        }
        for term in ["Apple_Terminal", "vscode", "ghostty", "kitty", ""] {
            assert!(!supported_in(term, "", false, false, false), "{term}");
        }
        // ssh forwards LC_* but not TERM_PROGRAM, which is how this survives a hop
        assert!(supported_in("", "iTerm2", false, false, false));
    }

    #[test]
    fn multiplexers_and_the_escape_hatch_win_over_the_terminal() {
        for (tmux, screen, off) in [(true, false, false), (false, true, false), (false, false, true)]
        {
            assert!(!supported_in("iTerm.app", "iTerm2", tmux, screen, off));
        }
    }

    #[test]
    fn base64_matches_rfc4648_vectors() {
        assert_eq!(encode_base64(b""), "");
        assert_eq!(encode_base64(b"f"), "Zg==");
        assert_eq!(encode_base64(b"fo"), "Zm8=");
        assert_eq!(encode_base64(b"foo"), "Zm9v");
        assert_eq!(encode_base64(b"foob"), "Zm9vYg==");
        assert_eq!(encode_base64(b"fooba"), "Zm9vYmE=");
        assert_eq!(encode_base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(encode_base64(&[0xff, 0xfe, 0xfd]), "//79");
    }
}
