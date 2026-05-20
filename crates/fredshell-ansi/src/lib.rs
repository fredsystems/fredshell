// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! ANSI / VT escape-sequence encoder and minimal decoder for fredshell.
//!
//! This crate operationalizes ADR 0002 and `PLAN_03`. It is **encoder
//! first**: the dominant use case is "write a known sequence to a
//! `Write`," and the data model is shaped around that. A small
//! decoder surface handles the structured terminal responses the
//! shell needs to read (DA1, DSR cursor position, kitty keyboard
//! query, OSC 52 read).
//!
//! The crate intentionally does **not**:
//!
//! - implement a general-purpose VT/ANSI terminal-emulator state
//!   machine (that is freminal's domain),
//! - know which sequences are safe to send to the host terminal
//!   (capability detection lives in [`PLAN_04`]),
//! - hold any global state.
//!
//! The boundary rule from `PLAN_03`: this crate knows how to *speak*
//! terminal; it does not know how to *be* a terminal.
//!
//! [`PLAN_04`]: https://github.com/FredSystems/fredshell/blob/main/Documents/PLAN_04_terminal_io.md
//!
//! # Public surface
//!
//! All encodable sequences implement [`Encode`]. All decodable
//! responses implement [`Decode`]. Concrete types live in
//! sub-modules (`sgr`, `cursor`, `erase`, `osc`, `mode`, `kitty`,
//! `decode`) and are populated by subsequent subtasks of `PLAN_03`.
//!
//! Error types are split per the default decision in `PLAN_03` §10:
//! [`EncodeError`] for encoder failures, [`DecodeError`] for
//! decoder failures. The two failure spaces have different shapes
//! and merging them would force callers to match on irrelevant
//! variants.

use std::io::{self, Write};

pub mod cursor;
pub mod decode;
pub mod erase;
pub(crate) mod int;
pub mod kitty;
pub mod mode;
pub mod osc;
pub mod sgr;

/// Common contract for all encodable ANSI sequences.
///
/// Implementations must:
///
/// 1. Write a *complete, well-formed* sequence (no partial writes,
///    no trailing newlines, no implicit resets unless the sequence
///    type semantically implies one).
/// 2. Allocate zero bytes on the heap on the encoding path. The
///    line editor redraws on every keystroke; allocation in this
///    code path would be a regression. The two documented
///    exceptions (OSC 8 with an owned URI, OSC 52 with a payload)
///    accept owned buffers from the caller — the encoder does not
///    grow them.
/// 3. Return a value from [`Encode::encoded_len`] that **exactly**
///    equals the number of bytes [`Encode::encode`] writes. Callers
///    rely on this to pre-size buffers.
///
/// In debug builds the crate may verify the `encoded_len` contract
/// by routing `encode` through a counting writer and panicking on
/// mismatch (`PLAN_03` §10). In release builds the contract is the
/// implementation's responsibility.
pub trait Encode {
    /// Write the sequence to `w`.
    ///
    /// # Errors
    ///
    /// Returns any error produced by `w`. Encoders themselves do
    /// not synthesize errors: every byte written is determined by
    /// `self`, and the only failure mode is the underlying writer.
    fn encode<W: Write + ?Sized>(&self, w: &mut W) -> io::Result<()>;

    /// The exact number of bytes [`Encode::encode`] will write.
    ///
    /// Implementations must return the correct count; callers may
    /// rely on it for buffer pre-sizing. The cost of this call must
    /// be bounded (no I/O, no allocation, no looping over data of
    /// unbounded size).
    fn encoded_len(&self) -> usize;
}

/// Object-safe companion to [`Encode`] for heterogeneous batch
/// encoding.
///
/// `Encode` itself is not object-safe (its `encode` method is
/// generic over the writer). [`EncodeDyn`] is a thin shim that
/// takes `&mut dyn Write`, so a slice of `&dyn EncodeDyn` can mix
/// concrete types — exactly the line-editor redraw case
/// (`Cursor::Goto` + `Erase::ToEndOfLine` + `Sgr` + …).
///
/// A blanket impl covers every [`Encode`] type; callers do not
/// implement [`EncodeDyn`] directly.
pub trait EncodeDyn {
    /// Write the sequence to a type-erased writer.
    ///
    /// # Errors
    ///
    /// Returns any error produced by `w`.
    fn encode_dyn(&self, w: &mut dyn Write) -> io::Result<()>;

    /// The exact number of bytes [`Self::encode_dyn`] will write.
    fn encoded_len_dyn(&self) -> usize;
}

impl<T: Encode + ?Sized> EncodeDyn for T {
    fn encode_dyn(&self, w: &mut dyn Write) -> io::Result<()> {
        self.encode(w)
    }

    fn encoded_len_dyn(&self) -> usize {
        self.encoded_len()
    }
}

/// Encode every value in `items` to `w`, in order.
///
/// Equivalent to calling [`EncodeDyn::encode_dyn`] on each item.
/// In debug builds this routes each write through a counting writer
/// and asserts that the bytes written match
/// [`EncodeDyn::encoded_len_dyn`] (per `PLAN_03` §10).
///
/// # Errors
///
/// Returns the first writer error encountered. Items already
/// written before the failure are left in `w`; the caller is
/// responsible for any cleanup.
pub fn encode_all<W: Write>(w: &mut W, items: &[&dyn EncodeDyn]) -> io::Result<()> {
    for item in items {
        encode_checked(*item, w)?;
    }
    Ok(())
}

/// Sum of [`EncodeDyn::encoded_len_dyn`] across `items`.
///
/// Used to pre-size a buffer before [`encode_all`].
#[must_use]
pub fn encoded_len_all(items: &[&dyn EncodeDyn]) -> usize {
    items.iter().map(|i| i.encoded_len_dyn()).sum()
}

/// Encode `value` to `w`, verifying the [`Encode::encoded_len`]
/// contract in debug builds.
///
/// In `cfg(debug_assertions)` builds this wraps `w` in a counting
/// writer and asserts that the byte count matches
/// `value.encoded_len_dyn()`. In release builds it is a direct call
/// with no overhead.
///
/// Encoder authors should funnel through this helper from tests so
/// the contract is checked, but the hot path may call
/// [`Encode::encode`] directly when the saved cycle matters.
///
/// # Errors
///
/// Returns any error produced by `w`.
///
/// # Panics
///
/// In debug builds, panics if the number of bytes written does not
/// equal `value.encoded_len_dyn()`. This is a programming error in
/// the encoder, never a runtime failure of the writer.
pub fn encode_checked<W: Write>(value: &dyn EncodeDyn, w: &mut W) -> io::Result<()> {
    #[cfg(debug_assertions)]
    {
        let expected = value.encoded_len_dyn();
        let mut counter = CountingWriter::new(w);
        value.encode_dyn(&mut counter)?;
        let written = counter.count();
        assert!(
            written == expected,
            "Encode::encoded_len contract violated: encoded_len() = {expected}, but encode() wrote {written} bytes",
        );
        Ok(())
    }
    #[cfg(not(debug_assertions))]
    {
        value.encode_dyn(w)
    }
}

/// `Write` adapter that forwards to an inner writer and counts the
/// bytes successfully written.
///
/// Used by [`encode_checked`] to verify the [`Encode::encoded_len`]
/// contract. Public so external benches and tests of encoder
/// implementations can use it; not part of the stable hot path.
pub struct CountingWriter<'a, W: Write + ?Sized> {
    inner: &'a mut W,
    count: usize,
}

impl<'a, W: Write + ?Sized> CountingWriter<'a, W> {
    /// Wrap `inner`. The wrapper counts bytes that the inner writer
    /// reports as written; partial writes are accounted correctly.
    pub const fn new(inner: &'a mut W) -> Self {
        Self { inner, count: 0 }
    }

    /// Total bytes successfully written through this wrapper.
    #[must_use]
    pub const fn count(&self) -> usize {
        self.count
    }
}

impl<W: Write + ?Sized> Write for CountingWriter<'_, W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.count += n;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// Common contract for the small set of structured terminal
/// responses the shell decodes.
///
/// The decoder is **byte-oriented and incremental**: it operates on
/// a `&[u8]` slice and reports either successful decode plus the
/// number of bytes consumed, or one of the two failure modes in
/// [`DecodeError`].
///
/// Decoders are *not* a state machine over arbitrary terminal
/// output. They are predicate-style matchers: "does this slice
/// match this specific shape?". Mixed-input streams, recovery
/// across malformed input, and async buffering are caller concerns
/// (see `PLAN_03` §5.1).
pub trait Decode: Sized {
    /// Attempt to decode a complete response from the front of
    /// `input`.
    ///
    /// On success returns `Ok((decoded, consumed))` where
    /// `consumed` is the number of bytes consumed from the front of
    /// `input`. The caller is responsible for advancing its buffer
    /// by `consumed` bytes.
    ///
    /// # Errors
    ///
    /// - [`DecodeError::Incomplete`] — `input` is a valid prefix of
    ///   a response of this type but is not yet complete. The
    ///   caller should buffer more input and retry.
    /// - [`DecodeError::Malformed`] — `input` cannot be a valid
    ///   response of this type. The caller should drop bytes up to
    ///   the next plausible start (typically `ESC`) and retry.
    fn decode(input: &[u8]) -> Result<(Self, usize), DecodeError>;
}

/// Errors produced by the encoder surface.
///
/// The encoder is mostly infallible — well-typed inputs produce
/// well-formed bytes — but a small number of constructors validate
/// arguments (e.g. one-indexed cursor coordinates per VT spec) and
/// can reject input. Those constructors return this type rather
/// than panicking.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum EncodeError {
    /// A coordinate (row or column) was zero. VT cursor sequences
    /// are one-indexed; zero is reserved.
    InvalidCoordinate {
        /// Human-readable name of the offending field
        /// (e.g. `"row"`, `"col"`).
        field: &'static str,
    },
}

impl std::fmt::Display for EncodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidCoordinate { field } => {
                write!(f, "invalid coordinate: {field} must be >= 1")
            }
        }
    }
}

impl std::error::Error for EncodeError {}

/// Errors produced by the decoder surface.
///
/// Distinct from [`EncodeError`] because the failure modes are
/// shape-different: the encoder's failures are validation of caller
/// input, while the decoder's failures are about the wire bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum DecodeError {
    /// The input is a valid prefix of a response of this type but
    /// is not yet complete. The caller should buffer more input and
    /// retry.
    Incomplete,
    /// The input cannot be a valid response of this type. The
    /// caller should drop bytes up to the next plausible
    /// resynchronisation point (typically `ESC`) and retry.
    Malformed {
        /// Byte offset within the input slice at which the
        /// malformation was detected.
        at: usize,
        /// Static reason describing what went wrong. Static so the
        /// decoder does not allocate on the failure path.
        reason: &'static str,
    },
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Incomplete => f.write_str("incomplete response"),
            Self::Malformed { at, reason } => {
                write!(f, "malformed response at byte {at}: {reason}")
            }
        }
    }
}

impl std::error::Error for DecodeError {}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::{
        CountingWriter, DecodeError, Encode, EncodeDyn, EncodeError, encode_all, encode_checked,
        encoded_len_all,
    };
    use crate::cursor::Cursor;
    use crate::erase::Erase;
    use crate::sgr::Sgr;
    use std::io::Write;

    #[test]
    fn encode_error_display_invalid_coordinate() {
        let err = EncodeError::InvalidCoordinate { field: "row" };
        assert_eq!(err.to_string(), "invalid coordinate: row must be >= 1");
    }

    #[test]
    fn decode_error_display_incomplete() {
        assert_eq!(DecodeError::Incomplete.to_string(), "incomplete response");
    }

    #[test]
    fn decode_error_display_malformed() {
        let err = DecodeError::Malformed {
            at: 4,
            reason: "expected CSI",
        };
        assert_eq!(
            err.to_string(),
            "malformed response at byte 4: expected CSI",
        );
    }

    #[test]
    fn encode_error_is_std_error() {
        fn assert_error<E: std::error::Error>() {}
        assert_error::<EncodeError>();
        assert_error::<DecodeError>();
    }

    #[test]
    fn counting_writer_counts_bytes() {
        let mut sink = Vec::new();
        let mut counter = CountingWriter::new(&mut sink);
        counter.write_all(b"hello").unwrap();
        counter.write_all(b" world").unwrap();
        assert_eq!(counter.count(), 11);
        assert_eq!(sink, b"hello world");
    }

    #[test]
    fn encode_dyn_blanket_impl_round_trips() {
        let sgr = Sgr::RESET;
        let dyn_ref: &dyn EncodeDyn = &sgr;
        let mut out = Vec::new();
        dyn_ref.encode_dyn(&mut out).unwrap();
        assert_eq!(dyn_ref.encoded_len_dyn(), out.len());
        assert_eq!(dyn_ref.encoded_len_dyn(), sgr.encoded_len());
    }

    #[test]
    fn encode_checked_passes_for_correct_encoder() {
        let mut out = Vec::new();
        encode_checked(&Sgr::RESET, &mut out).unwrap();
        assert_eq!(out, b"\x1b[0m");
    }

    #[test]
    fn encode_all_writes_in_order() {
        // A typical line-editor redraw frame: home cursor, erase to
        // end of line, set bold, write a literal (caller's job, not
        // ours), reset.
        let goto = Cursor::goto(1, 1).unwrap();
        let erase = Erase::InLineToEnd;
        let sgr = Sgr::RESET.with_bold();
        let reset = Sgr::RESET;

        let items: [&dyn EncodeDyn; 4] = [&goto, &erase, &sgr, &reset];
        let total = encoded_len_all(&items);
        let mut out = Vec::with_capacity(total);
        encode_all(&mut out, &items).unwrap();
        assert_eq!(out.len(), total);

        // Verify byte-for-byte: CSI 1;1H, CSI 0K, CSI 1m, CSI 0m.
        assert_eq!(&out, b"\x1b[1;1H\x1b[0K\x1b[1m\x1b[0m");
    }

    #[test]
    fn encoded_len_all_is_sum() {
        let items: [&dyn EncodeDyn; 3] = [&Sgr::RESET, &Erase::InLineAll, &Sgr::RESET];
        let expected = items.iter().map(|i| i.encoded_len_dyn()).sum::<usize>();
        assert_eq!(encoded_len_all(&items), expected);
    }

    #[test]
    fn encoded_len_all_empty_is_zero() {
        let items: [&dyn EncodeDyn; 0] = [];
        assert_eq!(encoded_len_all(&items), 0);
    }

    /// In debug builds, an encoder whose `encoded_len` lies must
    /// trigger the contract assertion. We construct a deliberately
    /// broken encoder and verify the panic.
    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "Encode::encoded_len contract violated")]
    fn encode_checked_detects_contract_violation() {
        struct Broken;
        impl Encode for Broken {
            fn encode<W: Write + ?Sized>(&self, w: &mut W) -> std::io::Result<()> {
                w.write_all(b"hello")
            }
            fn encoded_len(&self) -> usize {
                3 // lies — actually writes 5 bytes
            }
        }
        let mut sink = Vec::new();
        let _ = encode_checked(&Broken, &mut sink);
    }
}
