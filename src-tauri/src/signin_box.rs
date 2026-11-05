//! The Google sign-in box: the site seals the session to a one-time key of
//! the app, so the lovearound://signin link that carries it back is useless
//! to whoever else sees it (the process list, the browser's history, another
//! app that registered the scheme).
//!
//! ECDH P-256 between our key and the page's (`epk`, the raw uncompressed
//! point), HKDF-SHA256 (no salt, info "lovearound signin v1") → AES-256-GCM
//! key, the state as associated data; box = 12-byte nonce + ciphertext + tag.
//! Base64url without padding. The page's half is in
//! site/src/signin/index.html; the Mac and Linux apps do the same.

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use hkdf::Hkdf;
use p256::elliptic_curve::sec1::ToEncodedPoint;
use p256::{PublicKey, SecretKey};
use sha2::Sha256;

const INFO: &[u8] = b"lovearound signin v1";

/// What the box holds.
#[derive(serde::Deserialize, Debug, PartialEq)]
pub struct Sealed {
    pub uid: String,
    #[serde(default)]
    pub email: String,
    pub rt: String,
}

/// One sign-in's key pair; the private half never leaves memory.
pub struct SignInKey(SecretKey);

impl SignInKey {
    pub fn new() -> Self {
        Self(SecretKey::random(&mut rand_core::OsRng))
    }

    /// For the page's key= parameter.
    pub fn public(&self) -> String {
        URL_SAFE_NO_PAD.encode(self.0.public_key().to_encoded_point(false).as_bytes())
    }

    /// The session from the link's epk and box; None when it isn't ours or
    /// was tampered with.
    pub fn open(&self, epk: &str, sealed: &str, state: &str) -> Option<Sealed> {
        let theirs = PublicKey::from_sec1_bytes(&URL_SAFE_NO_PAD.decode(epk).ok()?).ok()?;
        let shared = p256::ecdh::diffie_hellman(self.0.to_nonzero_scalar(), theirs.as_affine());
        let mut key = [0u8; 32];
        Hkdf::<Sha256>::new(None, shared.raw_secret_bytes()).expand(INFO, &mut key).ok()?;
        let sealed = URL_SAFE_NO_PAD.decode(sealed).ok()?;
        if sealed.len() < 12 + 16 {
            return None;
        }
        let (nonce, body) = sealed.split_at(12);
        let plain = Aes256Gcm::new_from_slice(&key)
            .ok()?
            .decrypt(Nonce::from_slice(nonce), Payload { msg: body, aad: state.as_bytes() })
            .ok()?;
        serde_json::from_slice(&plain).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Sealed by the site's own code (site/src/signin/index.html, run in Node)
    // to this test-only key; the Mac and Linux tests open the same box.
    const PRIVATE_HEX: &str = "4c2a58504a9ae04a67d8f4ea50fd1f79943658449178467b6854456604816a29";
    const STATE: &str = "TEST-STATE-0001";
    const EPK: &str = "BPGVN5XMnukYvRdgRHgIG9npiQ9dDmJh93RUP-HARwZY8WpuEJ0p3M1s40BK0zyhmSBi_bC_d9PggnPnV9SvmUE";
    const BOX: &str = "nJB8DhyMR8ozaHyBDHHt6iXCFnYqjbcs-b39tBZVSjYV8CoVjf6xMtk5VXeXAoSYb895DC4vCFBw1KgUPaM6chi1kCMRjCSuwqvWHBb-ls8bxc1ElvhJIBo3iPUIuV35";

    fn key() -> SignInKey {
        let bytes: Vec<u8> = (0..32).map(|i| u8::from_str_radix(&PRIVATE_HEX[i * 2..i * 2 + 2], 16).unwrap()).collect();
        SignInKey(SecretKey::from_slice(&bytes).unwrap())
    }

    #[test]
    fn opens_what_the_site_sealed() {
        assert_eq!(
            key().open(EPK, BOX, STATE),
            Some(Sealed { uid: "uid-42".into(), email: "hleb@example.com".into(), rt: "AMf-refresh-token".into() })
        );
    }

    #[test]
    fn rejects_another_state_key_or_box() {
        assert_eq!(key().open(EPK, BOX, "OTHER"), None);
        assert_eq!(SignInKey::new().open(EPK, BOX, STATE), None);
        let mut flipped = BOX.to_string();
        flipped.replace_range(20..21, if &BOX[20..21] == "A" { "B" } else { "A" });
        assert_eq!(key().open(EPK, &flipped, STATE), None);
        assert_eq!(key().open("junk", BOX, STATE), None);
    }

    #[test]
    fn public_key_is_an_uncompressed_point() {
        let public = SignInKey::new().public();
        assert_eq!(public.len(), 87);
        assert_eq!(URL_SAFE_NO_PAD.decode(&public).unwrap()[0], 4);
    }
}
