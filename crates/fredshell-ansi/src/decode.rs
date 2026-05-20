// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! Decoders for structured terminal responses.
//!
//! Implements `PLAN_03` §5 — a small set of byte-oriented matchers
//! for the four response types fredshell needs to read from the
//! terminal:
//!
//! - [`Da1Response`] — Primary Device Attributes (`CSI ? Ps ; … c`).
//! - [`DsrCursorPosition`] — DSR cursor position
//!   (`CSI Pn ; Pn R`).
//! - [`KittyKeyboardQueryResponse`] — kitty keyboard query reply
//!   (`CSI ? flags u`).
//! - [`Osc52ReadResponse`] — OSC 52 clipboard read response
//!   (`OSC 52 ; <selection> ; <base64> ST`, `ST` = `ESC \` or `BEL`).
//!
//! Each decoder is a pure function `&[u8] -> Result<(Self, usize), DecodeError>`.
//! On success it returns the decoded value plus the number of bytes
//! consumed from the front of `input`. The decoder never owns the
//! caller's buffer; the caller is responsible for advancing past
//! `consumed` bytes after a successful decode.
//!
//! The decoder is **not** a general-purpose terminal state machine.
//! It does not handle interleaved printable text, partial recovery,
//! or async streaming. Per `PLAN_03` §5.1, those concerns belong to
//! the caller (see `PLAN_07`).

use crate::{Decode, DecodeError, kitty::KittyKeyboardFlags};

/// ESC = `0x1b`. CSI introducer is `ESC [`. OSC introducer is `ESC ]`.
const ESC: u8 = 0x1b;
/// ST terminator (the second byte; full ST is `ESC \`).
const ST_TAIL: u8 = b'\\';
/// BEL — accepted as an alternate OSC terminator per xterm convention.
const BEL: u8 = 0x07;

/// Maximum number of decimal parameters accepted in a CSI sequence.
///
/// DA1 in the wild reports up to ~16 capability codes; allow some
/// headroom. Bounded so the decoder never allocates.
const MAX_PARAMS: usize = 32;

/// A bounded inline parameter list parsed from a CSI sequence.
///
/// `len` indicates how many slots are populated; remaining slots
/// are zero. Stored on the stack to keep the decoder allocation-free.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Params {
    values: [u16; MAX_PARAMS],
    len: u8,
}

impl Params {
    const fn new() -> Self {
        Self {
            values: [0; MAX_PARAMS],
            len: 0,
        }
    }

    fn as_slice(&self) -> &[u16] {
        &self.values[..self.len as usize]
    }
}

/// Result of parsing a CSI body: the parameters and the final byte.
struct CsiParse {
    /// `true` if the sequence began with `CSI ?` (DEC private).
    private: bool,
    /// Decimal parameters separated by `;`.
    params: Params,
    /// Optional intermediate byte in `0x20..=0x2f` (e.g. `$` in
    /// DECRPM responses, `*` in some VT extensions). `None` if the
    /// sequence had no intermediate byte.
    intermediate: Option<u8>,
    /// Final byte (the byte in the `0x40..=0x7e` range that
    /// terminates the CSI).
    final_byte: u8,
    /// Total bytes consumed including `ESC [` and the final byte.
    consumed: usize,
}

/// Parse a CSI sequence from the front of `input`.
///
/// Recognises the shape `ESC [ [?] (digit | ';')* [intermediate]
/// final` where `intermediate` is a single byte in `0x20..=0x2f`
/// (sufficient for the responses we decode: DECRPM uses `$`, DA1 /
/// DSR / kitty have no intermediate). `final` is in `0x40..=0x7e`.
#[allow(clippy::too_many_lines)]
fn parse_csi(input: &[u8]) -> Result<CsiParse, DecodeError> {
    if input.len() < 2 {
        return Err(DecodeError::Incomplete);
    }
    if input[0] != ESC {
        return Err(DecodeError::Malformed {
            at: 0,
            reason: "expected ESC",
        });
    }
    if input[1] != b'[' {
        return Err(DecodeError::Malformed {
            at: 1,
            reason: "expected CSI introducer '['",
        });
    }

    let mut i = 2usize;
    let private = input.get(i) == Some(&b'?');
    if private {
        i += 1;
    }

    let mut params = Params::new();
    let mut current: u32 = 0;
    let mut have_digit = false;
    let mut intermediate: Option<u8> = None;

    while i < input.len() {
        let b = input[i];
        match b {
            b'0'..=b'9' => {
                have_digit = true;
                current = current
                    .saturating_mul(10)
                    .saturating_add(u32::from(b - b'0'));
                i += 1;
            }
            b';' => {
                if (params.len as usize) >= MAX_PARAMS {
                    return Err(DecodeError::Malformed {
                        at: i,
                        reason: "too many CSI parameters",
                    });
                }
                let value = u16::try_from(current).unwrap_or(u16::MAX);
                params.values[params.len as usize] = value;
                params.len += 1;
                current = 0;
                have_digit = false;
                i += 1;
            }
            0x20..=0x2f => {
                if intermediate.is_some() {
                    return Err(DecodeError::Malformed {
                        at: i,
                        reason: "multiple intermediate bytes not supported",
                    });
                }
                // Flush any pending parameter before the intermediate.
                if have_digit {
                    if (params.len as usize) >= MAX_PARAMS {
                        return Err(DecodeError::Malformed {
                            at: i,
                            reason: "too many CSI parameters",
                        });
                    }
                    let value = u16::try_from(current).unwrap_or(u16::MAX);
                    params.values[params.len as usize] = value;
                    params.len += 1;
                    current = 0;
                    have_digit = false;
                }
                intermediate = Some(b);
                i += 1;
            }
            0x40..=0x7e => {
                // Final byte. Flush any pending parameter (or an
                // empty trailing slot, e.g. `1;` — but only push
                // the pending slot if we saw a digit OR there is
                // already at least one separator, so that `CSI c`
                // with zero params yields an empty list).
                //
                // When an intermediate byte was already consumed,
                // any pending parameter was flushed at that point;
                // do not push a phantom trailing zero in that case.
                let should_flush = intermediate.is_none() && (have_digit || params.len > 0);
                if should_flush {
                    if (params.len as usize) >= MAX_PARAMS {
                        return Err(DecodeError::Malformed {
                            at: i,
                            reason: "too many CSI parameters",
                        });
                    }
                    let value = u16::try_from(current).unwrap_or(u16::MAX);
                    params.values[params.len as usize] = value;
                    params.len += 1;
                }
                return Ok(CsiParse {
                    private,
                    params,
                    intermediate,
                    final_byte: b,
                    consumed: i + 1,
                });
            }
            _ => {
                return Err(DecodeError::Malformed {
                    at: i,
                    reason: "invalid byte in CSI body",
                });
            }
        }
    }

    Err(DecodeError::Incomplete)
}

// ---------------------------------------------------------------------------
// DA1 — Primary Device Attributes response
// ---------------------------------------------------------------------------

/// Primary Device Attributes (DA1) response.
///
/// Wire form: `CSI ? <id> ; <cap> ; <cap> … c`. The leading parameter
/// is the conformance level (e.g. `64` = VT420) and the remaining
/// parameters are capability codes. Both are exposed verbatim;
/// interpretation is `PLAN_04`'s job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Da1Response {
    /// Conformance / device-type identifier (first parameter).
    pub id: u16,
    /// Capability codes (second and subsequent parameters).
    capabilities: [u16; MAX_PARAMS],
    /// Number of populated entries in [`Self::capabilities`].
    capabilities_len: u8,
}

impl Da1Response {
    /// Capability codes reported by the terminal.
    #[must_use]
    pub fn capabilities(&self) -> &[u16] {
        &self.capabilities[..self.capabilities_len as usize]
    }
}

impl Decode for Da1Response {
    fn decode(input: &[u8]) -> Result<(Self, usize), DecodeError> {
        let parsed = parse_csi(input)?;
        if !parsed.private {
            return Err(DecodeError::Malformed {
                at: 2,
                reason: "DA1 response must begin with 'CSI ?'",
            });
        }
        if parsed.intermediate.is_some() {
            return Err(DecodeError::Malformed {
                at: parsed.consumed - 2,
                reason: "DA1 response must not contain an intermediate byte",
            });
        }
        if parsed.final_byte != b'c' {
            return Err(DecodeError::Malformed {
                at: parsed.consumed - 1,
                reason: "DA1 response must end with 'c'",
            });
        }
        let params = parsed.params.as_slice();
        if params.is_empty() {
            return Err(DecodeError::Malformed {
                at: parsed.consumed - 1,
                reason: "DA1 response missing identifier",
            });
        }
        let id = params[0];
        let mut capabilities = [0u16; MAX_PARAMS];
        let rest = &params[1..];
        capabilities[..rest.len()].copy_from_slice(rest);
        // rest.len() <= MAX_PARAMS - 1 < u8::MAX so the cast is safe.
        let capabilities_len = u8::try_from(rest.len()).unwrap_or(0);
        Ok((
            Self {
                id,
                capabilities,
                capabilities_len,
            },
            parsed.consumed,
        ))
    }
}

// ---------------------------------------------------------------------------
// DSR cursor position
// ---------------------------------------------------------------------------

/// DSR (Device Status Report) cursor position response.
///
/// Wire form: `CSI <row> ; <col> R`. Both coordinates are 1-indexed
/// per the VT specification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DsrCursorPosition {
    /// 1-indexed row.
    pub row: u16,
    /// 1-indexed column.
    pub col: u16,
}

impl Decode for DsrCursorPosition {
    fn decode(input: &[u8]) -> Result<(Self, usize), DecodeError> {
        let parsed = parse_csi(input)?;
        if parsed.private {
            return Err(DecodeError::Malformed {
                at: 2,
                reason: "DSR cursor position must not be a private CSI",
            });
        }
        if parsed.intermediate.is_some() {
            return Err(DecodeError::Malformed {
                at: parsed.consumed - 2,
                reason: "DSR cursor position must not contain an intermediate byte",
            });
        }
        if parsed.final_byte != b'R' {
            return Err(DecodeError::Malformed {
                at: parsed.consumed - 1,
                reason: "DSR cursor position must end with 'R'",
            });
        }
        let params = parsed.params.as_slice();
        if params.len() != 2 {
            return Err(DecodeError::Malformed {
                at: parsed.consumed - 1,
                reason: "DSR cursor position requires exactly two parameters",
            });
        }
        Ok((
            Self {
                row: params[0],
                col: params[1],
            },
            parsed.consumed,
        ))
    }
}

// ---------------------------------------------------------------------------
// Kitty keyboard query response
// ---------------------------------------------------------------------------

/// Kitty keyboard query response.
///
/// Wire form: `CSI ? <flags> u`. Reports the currently active flag
/// set for the kitty keyboard protocol stack's top entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KittyKeyboardQueryResponse {
    /// Active flags reported by the terminal.
    pub flags: KittyKeyboardFlags,
}

impl Decode for KittyKeyboardQueryResponse {
    fn decode(input: &[u8]) -> Result<(Self, usize), DecodeError> {
        let parsed = parse_csi(input)?;
        if !parsed.private {
            return Err(DecodeError::Malformed {
                at: 2,
                reason: "kitty keyboard response must begin with 'CSI ?'",
            });
        }
        if parsed.intermediate.is_some() {
            return Err(DecodeError::Malformed {
                at: parsed.consumed - 2,
                reason: "kitty keyboard response must not contain an intermediate byte",
            });
        }
        if parsed.final_byte != b'u' {
            return Err(DecodeError::Malformed {
                at: parsed.consumed - 1,
                reason: "kitty keyboard response must end with 'u'",
            });
        }
        let params = parsed.params.as_slice();
        if params.len() != 1 {
            return Err(DecodeError::Malformed {
                at: parsed.consumed - 1,
                reason: "kitty keyboard response requires exactly one parameter",
            });
        }
        let bits = u8::try_from(params[0]).map_err(|_| DecodeError::Malformed {
            at: 3,
            reason: "kitty keyboard flags do not fit in u8",
        })?;
        Ok((
            Self {
                flags: KittyKeyboardFlags::from_bits(bits),
            },
            parsed.consumed,
        ))
    }
}

// ---------------------------------------------------------------------------
// DECRPM — Mode report response (DECRQM reply)
// ---------------------------------------------------------------------------

/// DECRPM mode-report response: the terminal's reply to a DECRQM
/// query.
///
/// Wire form: `CSI ? <mode> ; <value> $ y`. `mode` echoes the
/// requested DEC private mode (e.g. `2026` for synchronized output)
/// and `value` reports the mode's current state.
///
/// The most common queries fredshell issues are for mode 2026
/// (synchronized output) and mode 2027 (in-band resize notifications).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecrpmResponse {
    /// DEC private mode number being reported (e.g. `2026`).
    pub mode: u16,
    /// Mode state reported by the terminal.
    pub state: DecrpmState,
}

/// Possible values reported by a DECRPM response.
///
/// Per the VT specification:
/// - `0` — mode is not recognized.
/// - `1` — mode is set (enabled).
/// - `2` — mode is reset (disabled / supported but off).
/// - `3` — mode is permanently set.
/// - `4` — mode is permanently reset.
///
/// For capability detection, **any value other than [`NotRecognized`]
/// indicates the terminal supports the mode**; only the `0` reply
/// means "I do not know about this mode."
///
/// [`NotRecognized`]: DecrpmState::NotRecognized
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DecrpmState {
    /// `0` — mode is not recognized by the terminal.
    NotRecognized,
    /// `1` — mode is set.
    Set,
    /// `2` — mode is reset (supported but currently off).
    Reset,
    /// `3` — mode is permanently set (cannot be disabled).
    PermanentlySet,
    /// `4` — mode is permanently reset (cannot be enabled).
    PermanentlyReset,
    /// Any other value the terminal returned. Per the VT spec this
    /// is reserved, but we preserve it so callers can match on it
    /// explicitly rather than collapsing it into one of the standard
    /// values.
    Other(u16),
}

impl DecrpmState {
    /// `true` if the terminal recognizes the mode (any state other
    /// than [`Self::NotRecognized`]).
    ///
    /// This is the predicate capability detection uses: we don't
    /// care whether mode 2026 is currently on or off, we care
    /// whether the terminal knows about it.
    #[must_use]
    pub const fn is_supported(self) -> bool {
        !matches!(self, Self::NotRecognized)
    }

    const fn from_value(value: u16) -> Self {
        match value {
            0 => Self::NotRecognized,
            1 => Self::Set,
            2 => Self::Reset,
            3 => Self::PermanentlySet,
            4 => Self::PermanentlyReset,
            other => Self::Other(other),
        }
    }
}

impl Decode for DecrpmResponse {
    fn decode(input: &[u8]) -> Result<(Self, usize), DecodeError> {
        let parsed = parse_csi(input)?;
        if !parsed.private {
            return Err(DecodeError::Malformed {
                at: 2,
                reason: "DECRPM response must begin with 'CSI ?'",
            });
        }
        if parsed.intermediate != Some(b'$') {
            return Err(DecodeError::Malformed {
                at: parsed.consumed - 2,
                reason: "DECRPM response must contain the '$' intermediate byte",
            });
        }
        if parsed.final_byte != b'y' {
            return Err(DecodeError::Malformed {
                at: parsed.consumed - 1,
                reason: "DECRPM response must end with 'y'",
            });
        }
        let params = parsed.params.as_slice();
        if params.len() != 2 {
            return Err(DecodeError::Malformed {
                at: parsed.consumed - 1,
                reason: "DECRPM response requires exactly two parameters",
            });
        }
        Ok((
            Self {
                mode: params[0],
                state: DecrpmState::from_value(params[1]),
            },
            parsed.consumed,
        ))
    }
}

// ---------------------------------------------------------------------------
// OSC 52 read response
// ---------------------------------------------------------------------------

/// Selection target reported by the terminal in an OSC 52 response.
///
/// Single-character codes per the xterm OSC 52 specification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Osc52Selection {
    /// `c` — clipboard.
    Clipboard,
    /// `p` — primary selection.
    Primary,
    /// `s` — system selection (xterm extension).
    Selection,
    /// `0`..`7` — cut-buffer slots 0 through 7.
    CutBuffer(u8),
}

/// OSC 52 clipboard read response.
///
/// Wire form: `ESC ] 5 2 ; <selection> ; <base64> ST` where
/// `ST` is either `ESC \` (proper) or `BEL` (xterm-compatible).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Osc52ReadResponse {
    /// The selection the response refers to.
    pub selection: Osc52Selection,
    /// Decoded payload bytes.
    pub payload: Vec<u8>,
}

impl Decode for Osc52ReadResponse {
    fn decode(input: &[u8]) -> Result<(Self, usize), DecodeError> {
        // ESC ] 5 2 ;
        if input.len() < 5 {
            return Err(DecodeError::Incomplete);
        }
        if input[0] != ESC || input[1] != b']' {
            return Err(DecodeError::Malformed {
                at: 0,
                reason: "expected OSC introducer 'ESC ]'",
            });
        }
        if &input[2..5] != b"52;" {
            return Err(DecodeError::Malformed {
                at: 2,
                reason: "expected OSC 52 prefix '52;'",
            });
        }

        // Find the selection field (single-byte code) and the
        // separating ';'.
        let sel_start = 5;
        let after_sel = sel_start + 1;
        if input.len() <= after_sel {
            return Err(DecodeError::Incomplete);
        }
        if input[after_sel] != b';' {
            return Err(DecodeError::Malformed {
                at: after_sel,
                reason: "OSC 52 selection must be exactly one byte followed by ';'",
            });
        }
        let selection = match input[sel_start] {
            b'c' => Osc52Selection::Clipboard,
            b'p' => Osc52Selection::Primary,
            b's' => Osc52Selection::Selection,
            d @ b'0'..=b'7' => Osc52Selection::CutBuffer(d - b'0'),
            _ => {
                return Err(DecodeError::Malformed {
                    at: sel_start,
                    reason: "unknown OSC 52 selection code",
                });
            }
        };

        // Scan for terminator: BEL or ESC \.
        let payload_start = after_sel + 1;
        let mut i = payload_start;
        let (payload_end, consumed) = loop {
            if i >= input.len() {
                return Err(DecodeError::Incomplete);
            }
            match input[i] {
                BEL => break (i, i + 1),
                ESC => {
                    if i + 1 >= input.len() {
                        return Err(DecodeError::Incomplete);
                    }
                    if input[i + 1] == ST_TAIL {
                        break (i, i + 2);
                    }
                    return Err(DecodeError::Malformed {
                        at: i,
                        reason: "expected ST ('ESC \\') after ESC",
                    });
                }
                _ => i += 1,
            }
        };

        let encoded = &input[payload_start..payload_end];
        let payload = base64_decode(encoded).map_err(|reason| DecodeError::Malformed {
            at: payload_start,
            reason,
        })?;

        Ok((Self { selection, payload }, consumed))
    }
}

/// Decode RFC 4648 base64 (standard alphabet, with `=` padding).
///
/// Returns a static error reason on malformed input; the caller
/// attaches the byte offset.
fn base64_decode(input: &[u8]) -> Result<Vec<u8>, &'static str> {
    if !input.len().is_multiple_of(4) {
        return Err("base64 length not a multiple of 4");
    }
    let mut out = Vec::with_capacity(input.len() / 4 * 3);
    let mut i = 0;
    while i < input.len() {
        let q = &input[i..i + 4];
        let v0 = b64_value(q[0])?;
        let v1 = b64_value(q[1])?;
        let v2 = b64_value(q[2])?;
        let v3 = b64_value(q[3])?;
        if v0 == PAD || v1 == PAD {
            return Err("base64 padding in disallowed position");
        }
        let last = i + 4 == input.len();
        if !last && (v2 == PAD || v3 == PAD) {
            return Err("base64 padding before final quartet");
        }
        let b0 = (v0 << 2) | (v1 >> 4);
        out.push(b0);
        if v2 != PAD {
            let b1 = ((v1 & 0x0f) << 4) | (v2 >> 2);
            out.push(b1);
            if v3 != PAD {
                let b2 = ((v2 & 0x03) << 6) | v3;
                out.push(b2);
            }
        } else if v3 != PAD {
            return Err("base64 trailing non-padding byte after padding");
        }
        i += 4;
    }
    Ok(out)
}

/// Sentinel used by [`b64_value`] to indicate `=` padding.
const PAD: u8 = 0xff;

/// Decode a single base64 alphabet byte. Returns [`PAD`] for `=`.
const fn b64_value(b: u8) -> Result<u8, &'static str> {
    match b {
        b'A'..=b'Z' => Ok(b - b'A'),
        b'a'..=b'z' => Ok(b - b'a' + 26),
        b'0'..=b'9' => Ok(b - b'0' + 52),
        b'+' => Ok(62),
        b'/' => Ok(63),
        b'=' => Ok(PAD),
        _ => Err("invalid base64 character"),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    // ---- DA1 -------------------------------------------------------------

    #[test]
    fn da1_typical_response() {
        let input = b"\x1b[?64;1;2;6;9;15;22c";
        let (r, n) = Da1Response::decode(input).unwrap();
        assert_eq!(n, input.len());
        assert_eq!(r.id, 64);
        assert_eq!(r.capabilities(), &[1, 2, 6, 9, 15, 22]);
    }

    #[test]
    fn da1_id_only() {
        let input = b"\x1b[?6c";
        let (r, n) = Da1Response::decode(input).unwrap();
        assert_eq!(n, 5);
        assert_eq!(r.id, 6);
        assert!(r.capabilities().is_empty());
    }

    #[test]
    fn da1_incomplete_no_final() {
        assert_eq!(
            Da1Response::decode(b"\x1b[?64;1"),
            Err(DecodeError::Incomplete),
        );
    }

    #[test]
    fn da1_rejects_non_private() {
        let err = Da1Response::decode(b"\x1b[64c").unwrap_err();
        assert!(matches!(err, DecodeError::Malformed { .. }));
    }

    #[test]
    fn da1_rejects_wrong_final() {
        let err = Da1Response::decode(b"\x1b[?64R").unwrap_err();
        assert!(matches!(err, DecodeError::Malformed { .. }));
    }

    #[test]
    fn da1_consumes_only_one_response() {
        let input = b"\x1b[?6c\x1b[?6c";
        let (_, n) = Da1Response::decode(input).unwrap();
        assert_eq!(n, 5);
    }

    // ---- DSR -------------------------------------------------------------

    #[test]
    fn dsr_cursor_position_typical() {
        let (r, n) = DsrCursorPosition::decode(b"\x1b[12;34R").unwrap();
        assert_eq!(n, 8);
        assert_eq!(r, DsrCursorPosition { row: 12, col: 34 });
    }

    #[test]
    fn dsr_cursor_position_one_one() {
        let (r, n) = DsrCursorPosition::decode(b"\x1b[1;1R").unwrap();
        assert_eq!(n, 6);
        assert_eq!(r, DsrCursorPosition { row: 1, col: 1 });
    }

    #[test]
    fn dsr_rejects_private_csi() {
        let err = DsrCursorPosition::decode(b"\x1b[?1;1R").unwrap_err();
        assert!(matches!(err, DecodeError::Malformed { .. }));
    }

    #[test]
    fn dsr_rejects_wrong_final() {
        let err = DsrCursorPosition::decode(b"\x1b[1;1c").unwrap_err();
        assert!(matches!(err, DecodeError::Malformed { .. }));
    }

    #[test]
    fn dsr_rejects_wrong_param_count() {
        assert!(matches!(
            DsrCursorPosition::decode(b"\x1b[1R"),
            Err(DecodeError::Malformed { .. })
        ));
        assert!(matches!(
            DsrCursorPosition::decode(b"\x1b[1;1;1R"),
            Err(DecodeError::Malformed { .. })
        ));
    }

    #[test]
    fn dsr_incomplete() {
        assert_eq!(
            DsrCursorPosition::decode(b"\x1b[12;3"),
            Err(DecodeError::Incomplete),
        );
        assert_eq!(
            DsrCursorPosition::decode(b"\x1b["),
            Err(DecodeError::Incomplete),
        );
        assert_eq!(
            DsrCursorPosition::decode(b"\x1b"),
            Err(DecodeError::Incomplete),
        );
    }

    // ---- Kitty keyboard query response -----------------------------------

    #[test]
    fn kkbd_query_response_zero_flags() {
        let (r, n) = KittyKeyboardQueryResponse::decode(b"\x1b[?0u").unwrap();
        assert_eq!(n, 5);
        assert_eq!(r.flags, KittyKeyboardFlags::NONE);
    }

    #[test]
    fn kkbd_query_response_typical() {
        let (r, n) = KittyKeyboardQueryResponse::decode(b"\x1b[?3u").unwrap();
        assert_eq!(n, 5);
        assert_eq!(
            r.flags,
            KittyKeyboardFlags::DISAMBIGUATE | KittyKeyboardFlags::REPORT_EVENT_TYPES,
        );
    }

    #[test]
    fn kkbd_query_response_all_bits() {
        let (r, _) = KittyKeyboardQueryResponse::decode(b"\x1b[?31u").unwrap();
        assert_eq!(r.flags.bits(), 31);
    }

    #[test]
    fn kkbd_rejects_non_private() {
        assert!(matches!(
            KittyKeyboardQueryResponse::decode(b"\x1b[3u"),
            Err(DecodeError::Malformed { .. }),
        ));
    }

    #[test]
    fn kkbd_rejects_overflow_flags() {
        assert!(matches!(
            KittyKeyboardQueryResponse::decode(b"\x1b[?256u"),
            Err(DecodeError::Malformed { .. }),
        ));
    }

    // ---- DECRPM (mode report) --------------------------------------------

    #[test]
    fn decrpm_synchronized_output_supported_set() {
        let input = b"\x1b[?2026;1$y";
        let (r, n) = DecrpmResponse::decode(input).unwrap();
        assert_eq!(n, input.len());
        assert_eq!(r.mode, 2026);
        assert_eq!(r.state, DecrpmState::Set);
        assert!(r.state.is_supported());
    }

    #[test]
    fn decrpm_synchronized_output_supported_reset() {
        let input = b"\x1b[?2026;2$y";
        let (r, n) = DecrpmResponse::decode(input).unwrap();
        assert_eq!(n, input.len());
        assert_eq!(r.state, DecrpmState::Reset);
        assert!(r.state.is_supported());
    }

    #[test]
    fn decrpm_mode_not_recognized() {
        let input = b"\x1b[?2026;0$y";
        let (r, _) = DecrpmResponse::decode(input).unwrap();
        assert_eq!(r.state, DecrpmState::NotRecognized);
        assert!(!r.state.is_supported());
    }

    #[test]
    fn decrpm_permanently_set_and_reset() {
        let (set, _) = DecrpmResponse::decode(b"\x1b[?2026;3$y").unwrap();
        assert_eq!(set.state, DecrpmState::PermanentlySet);
        assert!(set.state.is_supported());

        let (reset, _) = DecrpmResponse::decode(b"\x1b[?2026;4$y").unwrap();
        assert_eq!(reset.state, DecrpmState::PermanentlyReset);
        assert!(reset.state.is_supported());
    }

    #[test]
    fn decrpm_preserves_unknown_state_value() {
        let input = b"\x1b[?2026;7$y";
        let (r, _) = DecrpmResponse::decode(input).unwrap();
        assert_eq!(r.state, DecrpmState::Other(7));
        // Other(_) is still "supported": the terminal recognized the
        // mode and reported *something*, just not a standard value.
        assert!(r.state.is_supported());
    }

    #[test]
    fn decrpm_rejects_non_private_csi() {
        let input = b"\x1b[2026;1$y";
        assert!(matches!(
            DecrpmResponse::decode(input),
            Err(DecodeError::Malformed { .. })
        ));
    }

    #[test]
    fn decrpm_rejects_missing_intermediate() {
        // Wrong final byte (`y` without `$`) — the parser will see
        // `y` as the final and report two-parameter shape, but the
        // intermediate check rejects it.
        let input = b"\x1b[?2026;1y";
        assert!(matches!(
            DecrpmResponse::decode(input),
            Err(DecodeError::Malformed { .. })
        ));
    }

    #[test]
    fn decrpm_rejects_wrong_final() {
        let input = b"\x1b[?2026;1$x";
        assert!(matches!(
            DecrpmResponse::decode(input),
            Err(DecodeError::Malformed { .. })
        ));
    }

    #[test]
    fn decrpm_rejects_wrong_param_count() {
        let input = b"\x1b[?2026$y";
        assert!(matches!(
            DecrpmResponse::decode(input),
            Err(DecodeError::Malformed { .. })
        ));
    }

    #[test]
    fn decrpm_incomplete() {
        // Has the `$` intermediate but truncated before the final.
        let input = b"\x1b[?2026;1$";
        assert!(matches!(
            DecrpmResponse::decode(input),
            Err(DecodeError::Incomplete)
        ));
    }

    #[test]
    fn decrpm_does_not_swallow_following_bytes() {
        let mut buf = Vec::new();
        buf.extend_from_slice(b"\x1b[?2026;1$y");
        buf.extend_from_slice(b"junk");
        let (_, n) = DecrpmResponse::decode(&buf).unwrap();
        assert_eq!(n, 11);
        assert_eq!(&buf[n..], b"junk");
    }

    #[test]
    fn da1_still_rejects_unexpected_intermediate() {
        // Sanity: adding intermediate-byte support to parse_csi must
        // not let DA1 accept a sequence with one.
        let input = b"\x1b[?64;1$c";
        assert!(matches!(
            Da1Response::decode(input),
            Err(DecodeError::Malformed { .. })
        ));
    }

    // ---- OSC 52 read -----------------------------------------------------

    #[test]
    fn osc52_read_clipboard_st_terminated() {
        // base64("hi") = "aGk="
        let input = b"\x1b]52;c;aGk=\x1b\\";
        let (r, n) = Osc52ReadResponse::decode(input).unwrap();
        assert_eq!(n, input.len());
        assert_eq!(r.selection, Osc52Selection::Clipboard);
        assert_eq!(r.payload, b"hi");
    }

    #[test]
    fn osc52_read_bel_terminated() {
        // base64("foo") = "Zm9v"
        let input = b"\x1b]52;c;Zm9v\x07";
        let (r, n) = Osc52ReadResponse::decode(input).unwrap();
        assert_eq!(n, input.len());
        assert_eq!(r.payload, b"foo");
    }

    #[test]
    fn osc52_read_primary_selection() {
        let input = b"\x1b]52;p;\x1b\\";
        let (r, _) = Osc52ReadResponse::decode(input).unwrap();
        assert_eq!(r.selection, Osc52Selection::Primary);
        assert!(r.payload.is_empty());
    }

    #[test]
    fn osc52_read_cut_buffer() {
        let input = b"\x1b]52;3;\x1b\\";
        let (r, _) = Osc52ReadResponse::decode(input).unwrap();
        assert_eq!(r.selection, Osc52Selection::CutBuffer(3));
    }

    #[test]
    fn osc52_read_rfc4648_vectors() {
        // Inverse of the encoder's RFC 4648 §10 vectors.
        let cases: &[(&[u8], &[u8])] = &[
            (b"\x1b]52;c;\x1b\\", b""),
            (b"\x1b]52;c;Zg==\x1b\\", b"f"),
            (b"\x1b]52;c;Zm8=\x1b\\", b"fo"),
            (b"\x1b]52;c;Zm9v\x1b\\", b"foo"),
            (b"\x1b]52;c;Zm9vYg==\x1b\\", b"foob"),
            (b"\x1b]52;c;Zm9vYmE=\x1b\\", b"fooba"),
            (b"\x1b]52;c;Zm9vYmFy\x1b\\", b"foobar"),
        ];
        for (input, expected) in cases {
            let (r, n) = Osc52ReadResponse::decode(input).unwrap();
            assert_eq!(n, input.len(), "bytes consumed for input {input:?}");
            assert_eq!(&r.payload[..], *expected, "payload for input {input:?}");
        }
    }

    #[test]
    fn osc52_incomplete_no_terminator() {
        assert_eq!(
            Osc52ReadResponse::decode(b"\x1b]52;c;aGk="),
            Err(DecodeError::Incomplete),
        );
    }

    #[test]
    fn osc52_incomplete_esc_only() {
        assert_eq!(
            Osc52ReadResponse::decode(b"\x1b]52;c;aGk=\x1b"),
            Err(DecodeError::Incomplete),
        );
    }

    #[test]
    fn osc52_rejects_bad_selection() {
        assert!(matches!(
            Osc52ReadResponse::decode(b"\x1b]52;x;\x1b\\"),
            Err(DecodeError::Malformed { .. }),
        ));
    }

    #[test]
    fn osc52_rejects_bad_base64() {
        assert!(matches!(
            Osc52ReadResponse::decode(b"\x1b]52;c;@@@\x1b\\"),
            Err(DecodeError::Malformed { .. }),
        ));
    }

    #[test]
    fn osc52_rejects_bad_prefix() {
        assert!(matches!(
            Osc52ReadResponse::decode(b"\x1b]53;c;\x1b\\"),
            Err(DecodeError::Malformed { .. }),
        ));
    }

    // ---- base64 helper ---------------------------------------------------

    #[test]
    fn base64_rejects_unaligned_length() {
        assert!(base64_decode(b"abc").is_err());
    }

    #[test]
    fn base64_rejects_padding_in_first_two() {
        assert!(base64_decode(b"=BCD").is_err());
        assert!(base64_decode(b"A=CD").is_err());
    }
}
