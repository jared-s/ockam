use crate::secret::Secret;
use crate::types::SecretAttributes;
use crate::SmallBuffer;
use ockam_core::Result;
use zeroize::Zeroize;

/// A trait for hashing data into fixed length output
pub trait Hasher: Zeroize {
    /// Compute the SHA-256 digest given input `data`
    fn sha256(&mut self, data: &[u8]) -> Result<[u8; 32]>;
    /// Derive multiple output [`Secret`]s with given attributes using the HKDF-SHA256 using
    /// specified salt, input key material and info.
    fn hkdf_sha256(
        &mut self,
        salt: &Secret,
        info: &[u8],
        ikm: Option<&Secret>,
        output_attributes: SmallBuffer<SecretAttributes>,
    ) -> Result<SmallBuffer<Secret>>;
}
