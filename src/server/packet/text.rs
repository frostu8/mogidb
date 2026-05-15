//! Stylized text.

use std::{
    borrow::Cow,
    error::Error as StdError,
    fmt::{self, Display, Formatter},
    str::from_utf8,
};

/// Ring Racers stylized text.
///
/// Ring Racers uses the extended ASCII bytes to represent colors, which aren't
/// representable in UTF-8. This is a type safe wrapper around that.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Text(Vec<u8>);

impl Text {
    /// Creates a new blank text.
    pub fn new() -> Text {
        Text::default()
    }

    /// Returns the text as a string, if it is entirely ASCII (with no color).
    pub fn to_str(&self) -> Option<&str> {
        from_utf8(&self.0).ok()
    }

    /// Returns the text as a string, removing any color bytes.
    pub fn to_stripped_str<'a>(&'a self) -> Cow<'a, str> {
        match from_utf8(&self.0) {
            Ok(str) => Cow::Borrowed(str),
            Err(err) => {
                // Copy valid text
                let valid_up_to = err.valid_up_to();
                let mut text = from_utf8(&self.0[..valid_up_to]).unwrap().to_owned();
                let mut rest = &self.0[valid_up_to..];

                while rest.len() > 0 {
                    // Text invariant makes sure we can't get control
                    // characters
                    let idx = rest.iter().position(|byte| !byte.is_ascii());
                    let (part, other) = rest.split_at(idx.unwrap_or(rest.len()));
                    text.push_str(from_utf8(part).unwrap());

                    // Discard other characters
                    let idx = rest.iter().position(|byte| byte.is_ascii());
                    let (_, other) = other.split_at(idx.unwrap_or(rest.len()));
                    rest = other;
                }

                Cow::Owned(text)
            }
        }
    }

    /// Returns the text as bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Converts from a Ring Racers c-str packet.
    pub fn from_cstr(buf: &[u8]) -> Result<Text, TextError> {
        let nul_idx = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        Text::from_bytes(&buf[..nul_idx])
    }

    /// Converts text from a slice of bytes.
    pub fn from_bytes(buf: &[u8]) -> Result<Text, TextError> {
        let mut result = Vec::with_capacity(buf.len());
        for (i, byte) in buf.iter().copied().enumerate() {
            if byte != 0x7F && (0x20u8..=0x8Fu8).contains(&byte) {
                // Valid byte
                result.push(byte);
            } else {
                return Err(TextError { valid_up_to: i });
            }
        }

        Ok(Text(result))
    }
}

impl Default for Text {
    fn default() -> Text {
        Text(Vec::new())
    }
}

/// An error type for [`Text`].
#[derive(Debug)]
pub struct TextError {
    valid_up_to: usize,
}

impl Display for TextError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "invalid ascii text @ {}", self.valid_up_to)
    }
}

impl StdError for TextError {}
