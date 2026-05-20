// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! Decoders for structured terminal responses.
//!
//! Populated by `PLAN_03` subtask 03.6. Will host [`super::Decode`]
//! impls for the four response types the shell needs to read:
//! DA1 (Primary Device Attributes), DSR (Device Status Report,
//! cursor position), kitty keyboard query response, and OSC 52
//! clipboard read response.
