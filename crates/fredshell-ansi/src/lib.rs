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
    fn encode<W: Write>(&self, w: &mut W) -> io::Result<()>;

    /// The exact number of bytes [`Encode::encode`] will write.
    ///
    /// Implementations must return the correct count; callers may
    /// rely on it for buffer pre-sizing. The cost of this call must
    /// be bounded (no I/O, no allocation, no looping over data of
    /// unbounded size).
    fn encoded_len(&self) -> usize;
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
mod tests {
    use super::{DecodeError, EncodeError};

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
}
