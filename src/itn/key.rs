use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use ed25519_dalek::{Signer, SigningKey, SECRET_KEY_LENGTH};

use crate::error::{Error, Result};

/// An ed25519 key that signs requests to a daemon's ITN GraphQL server.
///
/// The daemon accepts a request only when the key's public half is in its
/// `--itn-keys` list. Keys are exchanged as standard base64 with padding:
/// the private key as its 32-byte seed (the format of the orchestrator's
/// `key` and the log fetcher's `fetcher_sk`), and the public key as the
/// 32-byte value that goes into `--itn-keys`.
///
/// ```
/// # #[cfg(feature = "itn")] {
/// use mina_sdk::itn::ItnKey;
///
/// let key = ItnKey::generate();
/// let restored = ItnKey::from_base64(&key.to_base64()).unwrap();
/// assert_eq!(key.public_key_base64(), restored.public_key_base64());
/// # }
/// ```
#[derive(Clone)]
pub struct ItnKey {
    signing: SigningKey,
}

impl ItnKey {
    /// Load a key from the base64 encoding of its 32-byte seed.
    pub fn from_base64(seed_b64: &str) -> Result<Self> {
        let seed = BASE64
            .decode(seed_b64.trim())
            .map_err(|e| Error::InvalidItnKey(format!("not base64: {e}")))?;
        let seed: [u8; SECRET_KEY_LENGTH] = seed.try_into().map_err(|v: Vec<u8>| {
            Error::InvalidItnKey(format!(
                "expected {SECRET_KEY_LENGTH} bytes, got {}",
                v.len()
            ))
        })?;
        Ok(Self::from_seed(seed))
    }

    /// Create a key from its 32-byte seed.
    pub fn from_seed(seed: [u8; SECRET_KEY_LENGTH]) -> Self {
        Self {
            signing: SigningKey::from_bytes(&seed),
        }
    }

    /// Generate a new random key.
    pub fn generate() -> Self {
        let mut seed = [0u8; SECRET_KEY_LENGTH];
        getrandom::getrandom(&mut seed).expect("the operating system's random source failed");
        Self::from_seed(seed)
    }

    /// The base64 encoding of the 32-byte seed; the inverse of [`ItnKey::from_base64`].
    pub fn to_base64(&self) -> String {
        BASE64.encode(self.signing.to_bytes())
    }

    /// The base64 public key, as the daemon expects it in `--itn-keys`.
    pub fn public_key_base64(&self) -> String {
        BASE64.encode(self.signing.verifying_key().as_bytes())
    }

    /// Sign `msg` and return the base64 signature.
    pub(crate) fn sign_base64(&self, msg: &[u8]) -> String {
        BASE64.encode(self.signing.sign(msg).to_bytes())
    }
}

impl std::fmt::Debug for ItnKey {
    /// Shows the public key only, so a key never ends up in a log.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ItnKey")
            .field("public_key", &self.public_key_base64())
            .finish_non_exhaustive()
    }
}
