// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! OSC (Operating System Command) encoders.
//!
//! Implements `PLAN_03` §4.4. Covers OSC 7 (current working
//! directory notification), OSC 8 (hyperlinks), OSC 52 (clipboard
//! set), and OSC 133 (semantic prompt markers).
//!
//! All OSC sequences are terminated with `ST` (`ESC \`), per the
//! `PLAN_03` decision to emit the spec terminator universally and
//! defer the `BEL` compatibility question to `PLAN_04`.
//!
//! The OSC 7 encoder percent-encodes the path; OSC 52 base64-encodes
//! the payload. Both encode in a streaming fashion using small
//! on-stack scratch buffers — no intermediate `String` or `Vec<u8>`.
//! OSC 8 with a `Set` variant accepts an owned `String` for the URI
//! and optional ID; `PLAN_03` §6 documents this as one of two
//! exceptions to the zero-allocation contract.

use std::io::{self, Write};
use std::path::Path;

use crate::Encode;

const ESC: u8 = 0x1b;
const OSC_INTRO: &[u8] = b"\x1b]";
const ST: &[u8] = b"\x1b\\";

// ---------------------------------------------------------------------------
// OSC 7: current working directory notification
// ---------------------------------------------------------------------------

/// OSC 7 — notify the terminal of the shell's current working
/// directory.
///
/// The encoded form is `ESC ] 7 ; file://<host>/<percent-encoded-path> ST`.
/// If `hostname` is `None`, the host portion is empty (`file:///path`).
///
/// The path bytes are percent-encoded per RFC 3986 unreserved set.
/// `cwd` is held by reference; the encoder does not allocate.
#[derive(Debug, Clone, Copy)]
pub struct Osc7<'a> {
    /// Working directory. Must be absolute; the encoder does not
    /// validate this — emitting a relative path produces a malformed
    /// `file://` URI but the encoder remains well-defined.
    pub cwd: &'a Path,
    /// Optional hostname segment. When `None`, the URI is
    /// `file:///path`.
    pub hostname: Option<&'a str>,
}

impl Encode for Osc7<'_> {
    fn encode<W: Write + ?Sized>(&self, w: &mut W) -> io::Result<()> {
        w.write_all(OSC_INTRO)?;
        w.write_all(b"7;file://")?;
        if let Some(h) = self.hostname {
            write_percent_encoded(w, h.as_bytes())?;
        }
        // Path is written byte-by-byte; on Unix `Path` bytes are
        // arbitrary but typically UTF-8. The percent-encoder treats
        // them as opaque bytes.
        let path_bytes = path_as_bytes(self.cwd);
        // Ensure a single leading slash separates host from path,
        // even if the path is empty or already starts with `/`.
        if !path_bytes.starts_with(b"/") {
            w.write_all(b"/")?;
        }
        write_percent_encoded(w, path_bytes)?;
        w.write_all(ST)
    }

    fn encoded_len(&self) -> usize {
        let host_len = self
            .hostname
            .map_or(0, |h| percent_encoded_len(h.as_bytes()));
        let path_bytes = path_as_bytes(self.cwd);
        let path_pad = usize::from(!path_bytes.starts_with(b"/"));
        // OSC_INTRO + "7;file://" + host + path_pad + path + ST
        OSC_INTRO.len()
            + b"7;file://".len()
            + host_len
            + path_pad
            + percent_encoded_len(path_bytes)
            + ST.len()
    }
}

#[cfg(unix)]
fn path_as_bytes(p: &Path) -> &[u8] {
    use std::os::unix::ffi::OsStrExt;
    p.as_os_str().as_bytes()
}

#[cfg(not(unix))]
fn path_as_bytes(p: &Path) -> &[u8] {
    // On non-Unix targets, fall back to UTF-8 of `to_string_lossy`
    // would allocate. fredshell only targets Linux + macOS; the
    // non-Unix branch is unreachable in practice but kept compiling.
    p.to_str().map(str::as_bytes).unwrap_or(b"")
}

/// Returns `true` for the RFC 3986 unreserved set plus `/`, which
/// is safe to leave un-encoded inside a `file://` path.
const fn is_path_safe(b: u8) -> bool {
    matches!(b, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/')
}

fn write_percent_encoded<W: Write + ?Sized>(w: &mut W, bytes: &[u8]) -> io::Result<()> {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    // Buffer runs of safe bytes for fewer write_all calls.
    let mut start = 0;
    for (i, &b) in bytes.iter().enumerate() {
        if !is_path_safe(b) {
            if i > start {
                w.write_all(&bytes[start..i])?;
            }
            let hi = HEX[usize::from(b >> 4)];
            let lo = HEX[usize::from(b & 0x0f)];
            w.write_all(&[b'%', hi, lo])?;
            start = i + 1;
        }
    }
    if start < bytes.len() {
        w.write_all(&bytes[start..])?;
    }
    Ok(())
}

fn percent_encoded_len(bytes: &[u8]) -> usize {
    let mut len = 0;
    for &b in bytes {
        len += if is_path_safe(b) { 1 } else { 3 };
    }
    len
}

// ---------------------------------------------------------------------------
// OSC 8: hyperlinks
// ---------------------------------------------------------------------------

/// OSC 8 — set or clear a terminal hyperlink.
///
/// `Set` emits `ESC ] 8 ; [id=<id>] ; <uri> ST`. `Clear` emits
/// `ESC ] 8 ; ; ST` (empty params, empty URI).
///
/// `Set` holds owned `String`s because URIs and IDs are typically
/// constructed at call sites and have variable length; `PLAN_03`
/// §6 documents this as a permitted allocation by the *caller*.
/// The encoder itself does not allocate.
#[derive(Debug, Clone)]
pub enum Osc8 {
    /// Open a hyperlink. The `uri` is written verbatim (no escape
    /// of the URI content beyond what OSC requires — the URI must
    /// not contain a literal `ST` byte sequence).
    Set {
        /// Target URI.
        uri: String,
        /// Optional `id=` parameter for matched-link styling.
        id: Option<String>,
    },
    /// Close any active hyperlink.
    Clear,
}

impl Encode for Osc8 {
    fn encode<W: Write + ?Sized>(&self, w: &mut W) -> io::Result<()> {
        w.write_all(OSC_INTRO)?;
        w.write_all(b"8;")?;
        match self {
            Self::Set { uri, id } => {
                if let Some(id) = id {
                    w.write_all(b"id=")?;
                    w.write_all(id.as_bytes())?;
                }
                w.write_all(b";")?;
                w.write_all(uri.as_bytes())?;
            }
            Self::Clear => {
                w.write_all(b";")?;
            }
        }
        w.write_all(ST)
    }

    fn encoded_len(&self) -> usize {
        let body = match self {
            Self::Set { uri, id } => {
                let id_len = id.as_ref().map_or(0, |s| b"id=".len() + s.len());
                id_len + b";".len() + uri.len()
            }
            Self::Clear => b";".len(),
        };
        OSC_INTRO.len() + b"8;".len() + body + ST.len()
    }
}

// ---------------------------------------------------------------------------
// OSC 52: clipboard set
// ---------------------------------------------------------------------------

/// OSC 52 — set the system clipboard ("c" selection) to the given
/// payload, base64-encoded on the wire.
///
/// The payload is held as raw bytes; the encoder writes the base64
/// stream directly to the writer using a 3-byte input scratch
/// window (no intermediate buffer beyond `[u8; 4]`).
#[derive(Debug, Clone)]
pub struct Osc52Set {
    /// Raw clipboard bytes. Encoded as standard base64 (RFC 4648,
    /// no URL-safe alphabet, with `=` padding).
    pub payload: Vec<u8>,
}

impl Encode for Osc52Set {
    fn encode<W: Write + ?Sized>(&self, w: &mut W) -> io::Result<()> {
        w.write_all(OSC_INTRO)?;
        w.write_all(b"52;c;")?;
        write_base64(w, &self.payload)?;
        w.write_all(ST)
    }

    fn encoded_len(&self) -> usize {
        OSC_INTRO.len() + b"52;c;".len() + base64_encoded_len(self.payload.len()) + ST.len()
    }
}

const BASE64_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

// `chunks_exact_to_as_chunks` is a nightly-only clippy nursery lint
// that suggests `as_chunks::<3>()`, which is still unstable and thus
// unavailable on the stable toolchain we also build with. Allow it
// here; `unknown_lints` keeps stable clippy (which does not know the
// lint name) from erroring on the allow itself.
#[allow(unknown_lints)]
#[allow(clippy::chunks_exact_to_as_chunks)]
fn write_base64<W: Write + ?Sized>(w: &mut W, bytes: &[u8]) -> io::Result<()> {
    let mut chunks = bytes.chunks_exact(3);
    let mut buf = [0_u8; 4];
    for chunk in &mut chunks {
        let n = (u32::from(chunk[0]) << 16) | (u32::from(chunk[1]) << 8) | u32::from(chunk[2]);
        buf[0] = BASE64_ALPHABET[((n >> 18) & 0x3f) as usize];
        buf[1] = BASE64_ALPHABET[((n >> 12) & 0x3f) as usize];
        buf[2] = BASE64_ALPHABET[((n >> 6) & 0x3f) as usize];
        buf[3] = BASE64_ALPHABET[(n & 0x3f) as usize];
        w.write_all(&buf)?;
    }
    let rem = chunks.remainder();
    match rem.len() {
        0 => {}
        1 => {
            let n = u32::from(rem[0]) << 16;
            buf[0] = BASE64_ALPHABET[((n >> 18) & 0x3f) as usize];
            buf[1] = BASE64_ALPHABET[((n >> 12) & 0x3f) as usize];
            buf[2] = b'=';
            buf[3] = b'=';
            w.write_all(&buf)?;
        }
        2 => {
            let n = (u32::from(rem[0]) << 16) | (u32::from(rem[1]) << 8);
            buf[0] = BASE64_ALPHABET[((n >> 18) & 0x3f) as usize];
            buf[1] = BASE64_ALPHABET[((n >> 12) & 0x3f) as usize];
            buf[2] = BASE64_ALPHABET[((n >> 6) & 0x3f) as usize];
            buf[3] = b'=';
            w.write_all(&buf)?;
        }
        _ => unreachable!("chunks_exact(3) remainder is always 0..=2"),
    }
    Ok(())
}

const fn base64_encoded_len(input_len: usize) -> usize {
    // ceil(n / 3) * 4
    input_len.div_ceil(3) * 4
}

// ---------------------------------------------------------------------------
// OSC 133: semantic prompt markers (FinalTerm)
// ---------------------------------------------------------------------------

/// OSC 133 — semantic prompt markers (`FinalTerm` protocol).
///
/// The mapping of variant to letter follows the `FinalTerm`
/// specification used by `kitty`, `WezTerm`, `iTerm2`, and
/// `ghostty`:
///
/// | Variant            | Letter | Meaning                          |
/// | ------------------ | ------ | -------------------------------- |
/// | `PromptStart`      | `A`    | Start of the primary prompt.     |
/// | `CommandStart`     | `B`    | End of prompt, start of input.   |
/// | `OutputStart`      | `C`    | End of input, start of output.   |
/// | `CommandEnd`       | `D`    | End of output (command done).    |
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Osc133 {
    /// Start of the primary prompt (`OSC 133 ; A ST`).
    PromptStart,
    /// End of prompt, start of user-typed command
    /// (`OSC 133 ; B ST`).
    CommandStart,
    /// End of input, start of command output
    /// (`OSC 133 ; C ST`).
    OutputStart,
    /// End of command output (`OSC 133 ; D ST`).
    CommandEnd,
}

impl Encode for Osc133 {
    fn encode<W: Write + ?Sized>(&self, w: &mut W) -> io::Result<()> {
        let bytes: &[u8] = match self {
            Self::PromptStart => b"\x1b]133;A\x1b\\",
            Self::CommandStart => b"\x1b]133;B\x1b\\",
            Self::OutputStart => b"\x1b]133;C\x1b\\",
            Self::CommandEnd => b"\x1b]133;D\x1b\\",
        };
        w.write_all(bytes)
    }

    fn encoded_len(&self) -> usize {
        // ESC ] 1 3 3 ; X ESC \  = 9 bytes for any variant.
        9
    }
}

// Sanity check: OSC introducer must be exactly `ESC ]`.
const _: () = assert!(OSC_INTRO[0] == ESC && OSC_INTRO[1] == b']');

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::{Osc7, Osc8, Osc52Set, Osc133, base64_encoded_len, write_base64};
    use crate::Encode;
    use std::path::Path;

    fn enc<E: Encode>(e: &E) -> Vec<u8> {
        let mut out = Vec::new();
        e.encode(&mut out).unwrap();
        assert_eq!(out.len(), e.encoded_len());
        out
    }

    // OSC 7 -----------------------------------------------------------------

    #[test]
    fn osc7_with_hostname() {
        let v = Osc7 {
            cwd: Path::new("/home/fred"),
            hostname: Some("nixbox"),
        };
        assert_eq!(enc(&v), b"\x1b]7;file://nixbox/home/fred\x1b\\");
    }

    #[test]
    fn osc7_without_hostname() {
        let v = Osc7 {
            cwd: Path::new("/tmp"),
            hostname: None,
        };
        assert_eq!(enc(&v), b"\x1b]7;file:///tmp\x1b\\");
    }

    #[test]
    fn osc7_percent_encodes_spaces_and_unicode() {
        let v = Osc7 {
            cwd: Path::new("/with space/café"),
            hostname: None,
        };
        // "café" UTF-8: 63 61 66 C3 A9 → c a f %C3%A9
        assert_eq!(
            enc(&v),
            b"\x1b]7;file:///with%20space/caf%C3%A9\x1b\\".as_slice()
        );
    }

    #[test]
    fn osc7_path_without_leading_slash_gets_one() {
        let v = Osc7 {
            cwd: Path::new("relative"),
            hostname: None,
        };
        assert_eq!(enc(&v), b"\x1b]7;file:///relative\x1b\\");
    }

    // OSC 8 -----------------------------------------------------------------

    #[test]
    fn osc8_set_no_id() {
        let v = Osc8::Set {
            uri: "https://example.com/".to_string(),
            id: None,
        };
        assert_eq!(enc(&v), b"\x1b]8;;https://example.com/\x1b\\");
    }

    #[test]
    fn osc8_set_with_id() {
        let v = Osc8::Set {
            uri: "https://example.com/".to_string(),
            id: Some("xyz".to_string()),
        };
        assert_eq!(enc(&v), b"\x1b]8;id=xyz;https://example.com/\x1b\\");
    }

    #[test]
    fn osc8_clear() {
        assert_eq!(enc(&Osc8::Clear), b"\x1b]8;;\x1b\\");
    }

    // OSC 52 ----------------------------------------------------------------

    #[test]
    fn base64_lengths() {
        assert_eq!(base64_encoded_len(0), 0);
        assert_eq!(base64_encoded_len(1), 4);
        assert_eq!(base64_encoded_len(2), 4);
        assert_eq!(base64_encoded_len(3), 4);
        assert_eq!(base64_encoded_len(4), 8);
        assert_eq!(base64_encoded_len(5), 8);
        assert_eq!(base64_encoded_len(6), 8);
        assert_eq!(base64_encoded_len(7), 12);
    }

    #[test]
    fn base64_known_vectors() {
        // RFC 4648 §10 test vectors.
        let cases: &[(&[u8], &[u8])] = &[
            (b"", b""),
            (b"f", b"Zg=="),
            (b"fo", b"Zm8="),
            (b"foo", b"Zm9v"),
            (b"foob", b"Zm9vYg=="),
            (b"fooba", b"Zm9vYmE="),
            (b"foobar", b"Zm9vYmFy"),
        ];
        for (input, expected) in cases {
            let mut out = Vec::new();
            write_base64(&mut out, input).unwrap();
            assert_eq!(out, *expected, "input={input:?}");
        }
    }

    #[test]
    fn osc52_empty_payload() {
        let v = Osc52Set {
            payload: Vec::new(),
        };
        assert_eq!(enc(&v), b"\x1b]52;c;\x1b\\");
    }

    #[test]
    fn osc52_short_payload() {
        let v = Osc52Set {
            payload: b"foobar".to_vec(),
        };
        assert_eq!(enc(&v), b"\x1b]52;c;Zm9vYmFy\x1b\\");
    }

    #[test]
    fn osc52_unaligned_payloads() {
        let v = Osc52Set {
            payload: b"f".to_vec(),
        };
        assert_eq!(enc(&v), b"\x1b]52;c;Zg==\x1b\\");
        let v = Osc52Set {
            payload: b"fo".to_vec(),
        };
        assert_eq!(enc(&v), b"\x1b]52;c;Zm8=\x1b\\");
    }

    // OSC 133 ---------------------------------------------------------------

    #[test]
    fn osc133_letters() {
        assert_eq!(enc(&Osc133::PromptStart), b"\x1b]133;A\x1b\\");
        assert_eq!(enc(&Osc133::CommandStart), b"\x1b]133;B\x1b\\");
        assert_eq!(enc(&Osc133::OutputStart), b"\x1b]133;C\x1b\\");
        assert_eq!(enc(&Osc133::CommandEnd), b"\x1b]133;D\x1b\\");
    }

    #[test]
    fn osc133_all_variants_nine_bytes() {
        for v in [
            Osc133::PromptStart,
            Osc133::CommandStart,
            Osc133::OutputStart,
            Osc133::CommandEnd,
        ] {
            assert_eq!(v.encoded_len(), 9);
        }
    }
}
