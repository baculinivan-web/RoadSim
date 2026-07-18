use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Visitor};
use std::fmt;

/// A binary SHA-256 digest with one canonical lowercase hexadecimal encoding.
///
/// The type identifies the algorithm but does not define which project fields
/// participate in a particular semantic hash.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Sha256Digest([u8; 32]);

impl Sha256Digest {
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Display for Sha256Digest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl Serialize for Sha256Digest {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

struct DigestVisitor;

impl Visitor<'_> for DigestVisitor {
    type Value = Sha256Digest;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("exactly 64 lowercase hexadecimal SHA-256 characters")
    }

    fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
        if value.len() != 64
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(E::custom(
                "expected exactly 64 lowercase hexadecimal SHA-256 characters",
            ));
        }
        let mut bytes = [0_u8; 32];
        for (index, output) in bytes.iter_mut().enumerate() {
            let high = decode_nibble(value.as_bytes()[index * 2]);
            let low = decode_nibble(value.as_bytes()[index * 2 + 1]);
            *output = (high << 4) | low;
        }
        Ok(Sha256Digest(bytes))
    }
}

impl<'de> Deserialize<'de> for Sha256Digest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_str(DigestVisitor)
    }
}

const fn decode_nibble(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        _ => unreachable!(),
    }
}
