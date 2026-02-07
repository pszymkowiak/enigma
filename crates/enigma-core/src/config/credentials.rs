//! Credential encryption/decryption for storing secrets in TOML config.
//!
//! Encrypted format: `enc:<nonce_hex>:<ciphertext_base64>`
//! If a value doesn't start with `enc:`, it's returned as-is (plaintext passthrough).

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use rand::RngCore;

/// Encrypt a plaintext credential using a 32-byte key.
/// Returns a string in the format `enc:<nonce_hex>:<ciphertext_base64>`.
pub fn encrypt_credential(plaintext: &str, key: &[u8; 32]) -> anyhow::Result<String> {
    let cipher =
        Aes256Gcm::new_from_slice(key).map_err(|e| anyhow::anyhow!("AES key error: {e}"))?;

    let mut nonce_bytes = [0u8; 12];
    rand::rngs::OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| anyhow::anyhow!("Encryption failed: {e}"))?;

    let nonce_hex = hex_encode(&nonce_bytes);
    let ct_b64 = base64_encode(&ciphertext);

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

    let nonce_bytes = hex_decode(parts[1])?;
    if nonce_bytes.len() != 12 {
        anyhow::bail!(
            "Invalid nonce length: expected 12, got {}",
            nonce_bytes.len()
        );
    }

    let ciphertext = base64_decode(parts[2])?;

    let cipher =
        Aes256Gcm::new_from_slice(key).map_err(|e| anyhow::anyhow!("AES key error: {e}"))?;
    let nonce = Nonce::from_slice(&nonce_bytes);

    let plaintext = cipher
        .decrypt(nonce, ciphertext.as_ref())
        .map_err(|e| anyhow::anyhow!("Decryption failed: {e}"))?;

    Ok(String::from_utf8(plaintext)?)
}

fn hex_encode(data: &[u8]) -> String {
    data.iter().map(|b| format!("{b:02x}")).collect()
}

fn hex_decode(hex: &str) -> anyhow::Result<Vec<u8>> {
    (0..hex.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&hex[i..i + 2], 16).map_err(|e| anyhow::anyhow!("Invalid hex: {e}"))
        })
        .collect()
}

fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let n = (b0 << 16) | (b1 << 8) | b2;
        result.push(CHARS[((n >> 18) & 63) as usize] as char);
        result.push(CHARS[((n >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            result.push(CHARS[((n >> 6) & 63) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(CHARS[(n & 63) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}

fn base64_decode(s: &str) -> anyhow::Result<Vec<u8>> {
    fn val(c: u8) -> anyhow::Result<u32> {
        match c {
            b'A'..=b'Z' => Ok((c - b'A') as u32),
            b'a'..=b'z' => Ok((c - b'a' + 26) as u32),
            b'0'..=b'9' => Ok((c - b'0' + 52) as u32),
            b'+' => Ok(62),
            b'/' => Ok(63),
            b'=' => Ok(0),
            _ => anyhow::bail!("invalid base64 char: {c}"),
        }
    }
    let bytes = s.as_bytes();
    let mut result = Vec::new();
    for chunk in bytes.chunks(4) {
        if chunk.len() < 4 {
            break;
        }
        let n = (val(chunk[0])? << 18)
            | (val(chunk[1])? << 12)
            | (val(chunk[2])? << 6)
            | val(chunk[3])?;
        result.push(((n >> 16) & 0xFF) as u8);
        if chunk[2] != b'=' {
            result.push(((n >> 8) & 0xFF) as u8);
        }
        if chunk[3] != b'=' {
            result.push((n & 0xFF) as u8);
        }
    }
    Ok(result)
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
