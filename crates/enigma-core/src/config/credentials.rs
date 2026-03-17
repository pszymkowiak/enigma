//! Credential encryption/decryption for storing secrets in TOML config.
//!
//! Encrypted format: `enc:<nonce_hex>:<ciphertext_base64>`
//! If a value doesn't start with `enc:`, it's returned as-is (plaintext passthrough).

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use rand::RngCore;

/// Fixed AAD (Associated Authenticated Data) to bind ciphertext to its context.
const CREDENTIAL_AAD: &[u8] = b"enigma-credential-v1";

/// Encrypt a plaintext credential using a 32-byte key.
/// Returns a string in the format `enc:<nonce_hex>:<ciphertext_base64>`.
pub fn encrypt_credential(plaintext: &str, key: &[u8; 32]) -> anyhow::Result<String> {
    let cipher =
        Aes256Gcm::new_from_slice(key).map_err(|e| anyhow::anyhow!("AES key error: {e}"))?;

    let mut nonce_bytes = [0u8; 12];
    rand::rngs::OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let payload = Payload {
        msg: plaintext.as_bytes(),
        aad: CREDENTIAL_AAD,
    };

    let ciphertext = cipher
        .encrypt(nonce, payload)
        .map_err(|e| anyhow::anyhow!("Encryption failed: {e}"))?;

    let nonce_hex = hex::encode(nonce_bytes);
    let ct_b64 = BASE64.encode(&ciphertext);

    Ok(format!("enc:{nonce_hex}:{ct_b64}"))
}

/// Decrypt a credential value. If it doesn't start with `enc:`, returns it as-is.
pub fn decrypt_credential(value: &str, key: &[u8; 32]) -> anyhow::Result<String> {
    if !value.starts_with("enc:") {
        return Ok(value.to_string());
    }

    let parts: Vec<&str> = value.splitn(3, ':').collect();
    if parts.len() != 3 {
        anyhow::bail!("Invalid encrypted credential format");
    }

    let nonce_bytes = hex::decode(parts[1])?;
    if nonce_bytes.len() != 12 {
        anyhow::bail!(
            "Invalid nonce length: expected 12, got {}",
            nonce_bytes.len()
        );
    }

    let ciphertext = BASE64.decode(parts[2])?;

    let cipher =
        Aes256Gcm::new_from_slice(key).map_err(|e| anyhow::anyhow!("AES key error: {e}"))?;
    let nonce = Nonce::from_slice(&nonce_bytes);

    let payload = Payload {
        msg: ciphertext.as_ref(),
        aad: CREDENTIAL_AAD,
    };

    let plaintext = cipher
        .decrypt(nonce, payload)
        .map_err(|e| anyhow::anyhow!("Decryption failed: {e}"))?;

    Ok(String::from_utf8(plaintext)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_encrypt_decrypt() {
        let key = [42u8; 32];
        let secret = "my-super-secret-access-key";

        let encrypted = encrypt_credential(secret, &key).unwrap();
        assert!(encrypted.starts_with("enc:"));

        let decrypted = decrypt_credential(&encrypted, &key).unwrap();
        assert_eq!(decrypted, secret);
    }

    #[test]
    fn plaintext_passthrough() {
        let key = [0u8; 32];
        let plain = "just-a-plain-value";
        let result = decrypt_credential(plain, &key).unwrap();
        assert_eq!(result, plain);
    }
}
