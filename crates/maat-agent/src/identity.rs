//! Agent identity.

use maat::{Keypair, PublicKey, Signer, Signature, Result as MaatResult};

/// An autonomous agent's cryptographic identity.
///
/// Wraps [`maat::Keypair`] with a stable, semantically clearer name —
/// integrators reason about "this is the agent's identity" rather than
/// "this is a keypair." Implements [`Signer`] so it can be passed
/// anywhere the protocol library expects a signing identity.
///
/// ## Persistence
///
/// Use [`AgentIdentity::generate`] for new agents and persist the seed
/// returned by [`AgentIdentity::secret_seed`]. Restore on subsequent runs
/// with [`AgentIdentity::from_seed`]. The 32-byte seed is the agent's
/// only secret — protect it at rest with the same care as any
/// cryptographic key.
///
/// ```
/// use maat_agent::AgentIdentity;
///
/// // First run: generate.
/// let id = AgentIdentity::generate();
/// let seed = id.secret_seed();
/// // ... store `seed` somewhere safe ...
///
/// // Subsequent runs: restore.
/// let restored = AgentIdentity::from_seed(seed);
/// assert_eq!(id.public_key().key_data, restored.public_key().key_data);
/// ```
#[derive(Debug)]
pub struct AgentIdentity {
    keypair: Keypair,
}

impl Clone for AgentIdentity {
    fn clone(&self) -> Self {
        // Keypair doesn't impl Clone (ed25519-dalek deliberate choice on
        // signing keys). Round-trip through secret_seed; identical
        // identity, same public key. Allocates a new SigningKey.
        AgentIdentity::from_seed(self.keypair.secret_seed())
    }
}

impl AgentIdentity {
    /// Generate a fresh identity with a random keypair.
    pub fn generate() -> Self {
        AgentIdentity {
            keypair: Keypair::generate(),
        }
    }

    /// Restore an identity from a previously stored 32-byte seed.
    pub fn from_seed(seed: [u8; 32]) -> Self {
        AgentIdentity {
            keypair: Keypair::from_seed(seed),
        }
    }

    /// The agent's public key. Share this with principals issuing
    /// delegations.
    pub fn public_key(&self) -> &PublicKey {
        &self.keypair.public_key
    }

    /// The 32-byte secret seed. Persist this securely.
    pub fn secret_seed(&self) -> [u8; 32] {
        self.keypair.secret_seed()
    }

    /// Borrow the underlying [`Keypair`] for protocol-library APIs that
    /// require it directly.
    pub fn as_keypair(&self) -> &Keypair {
        &self.keypair
    }
}

impl Signer for AgentIdentity {
    fn public_key(&self) -> PublicKey {
        self.keypair.public_key.clone()
    }

    fn sign(&self, message: &[u8]) -> MaatResult<Signature> {
        self.keypair.sign(message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_seed() {
        let id = AgentIdentity::generate();
        let seed = id.secret_seed();
        let restored = AgentIdentity::from_seed(seed);
        assert_eq!(
            id.public_key().key_data,
            restored.public_key().key_data
        );
    }

    #[test]
    fn signs_consistently_under_signer_trait() {
        let id = AgentIdentity::generate();
        let msg = b"test message";
        let sig1 = Signer::sign(&id, msg).unwrap();
        let sig2 = Signer::sign(&id, msg).unwrap();
        // Ed25519 is deterministic.
        assert_eq!(sig1.value, sig2.value);
    }
}
