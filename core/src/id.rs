use std::fmt;

use serde::{Deserialize, Serialize};

/// Ids are stored exactly as they appear on disk so a file written by any other
/// version round-trips verbatim. Generation produces 16 lowercase hex chars,
/// matching `:crypto.strong_rand_bytes(8) |> Base.encode16(case: :lower)`.
macro_rules! id_type {
    ($name:ident, $label:literal) => {
        #[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(Box<str>);

        impl $name {
            #[must_use]
            pub fn generate() -> Self {
                let mut bytes = [0u8; 8];
                getrandom::fill(&mut bytes).unwrap_or_else(|_| {
                    // getrandom only fails if the OS entropy source is unavailable, which
                    // would also break TLS and ssh. Fall back to the clock so a todo can
                    // still be created rather than losing the keystroke.
                    let nanos = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(0u128, |d| d.as_nanos());
                    bytes = u64::try_from(nanos & u128::from(u64::MAX))
                        .unwrap_or(0)
                        .to_le_bytes();
                });
                Self(hex(&bytes).into_boxed_str())
            }

            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }

            /// True when the id is safe to use as a path component.
            #[must_use]
            pub fn is_canonical(&self) -> bool {
                self.0.len() == 16 && self.0.bytes().all(|b| b.is_ascii_hexdigit())
            }
        }

        impl From<&str> for $name {
            fn from(s: &str) -> Self {
                Self(s.into())
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, concat!($label, "#{}"), self.0)
            }
        }
    };
}

id_type!(TodoId, "todo");
id_type!(ProjectId, "project");

fn hex(bytes: &[u8]) -> String {
    use fmt::Write as _;
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(out, "{b:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_ids_are_16_lowercase_hex() {
        for _ in 0..64 {
            let id = TodoId::generate();
            assert_eq!(id.as_str().len(), 16, "{id:?}");
            assert!(
                id.as_str()
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            );
            assert!(id.is_canonical());
        }
    }

    #[test]
    fn generated_ids_are_distinct() {
        let a = TodoId::generate();
        let b = TodoId::generate();
        assert_ne!(a, b);
    }

    #[test]
    fn foreign_ids_round_trip_verbatim() {
        for raw in [
            "",
            "not-hex",
            "ABCDEF0123456789",
            "0123456789abcdef0",
            "../escape",
        ] {
            let id = ProjectId::from(raw);
            assert_eq!(id.as_str(), raw);
            let json = serde_json::to_string(&id).expect("serialize");
            assert_eq!(json, serde_json::to_string(raw).expect("serialize str"));
            let back: ProjectId = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(back, id);
        }
    }

    #[test]
    fn canonical_means_safe_as_a_path_component() {
        assert!(ProjectId::from("0123456789abcdef").is_canonical());
        // Uppercase hex is still a safe filename, so it stays canonical.
        assert!(ProjectId::from("ABCDEF0123456789").is_canonical());
        assert!(!ProjectId::from("../escape").is_canonical());
        assert!(!ProjectId::from("").is_canonical());
        assert!(!ProjectId::from("0123456789abcdef0").is_canonical());
        assert!(!ProjectId::from("0123456789abcdeg").is_canonical());
    }
}
