use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Verify a GitHub webhook's `X-Hub-Signature-256` header against the raw request body using
/// the repo's webhook secret. Must be checked against raw bytes, not a re-serialized payload.
pub fn verify(secret: &str, raw_body: &[u8], signature_header: &str) -> bool {
    let Some(hex_sig) = signature_header.strip_prefix("sha256=") else {
        return false;
    };
    let Ok(expected) = hex::decode(hex_sig) else {
        return false;
    };

    let Ok(mut mac) = HmacSha256::new_from_slice(secret.as_bytes()) else {
        return false;
    };
    mac.update(raw_body);

    mac.verify_slice(&expected).is_ok()
}

pub fn generate_secret() -> String {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    use rand::RngCore;

    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verifies_valid_signature() {
        let secret = "test-secret";
        let body = b"{\"hello\":\"world\"}";
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body);
        let sig = format!("sha256={}", hex::encode(mac.finalize().into_bytes()));

        assert!(verify(secret, body, &sig));
        assert!(!verify(secret, body, "sha256=deadbeef"));
        assert!(!verify("wrong-secret", body, &sig));
    }
}
