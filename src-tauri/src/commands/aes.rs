use aes::Aes256;
use cbc::{Decryptor, Encryptor};
use cipher::{block_padding::Pkcs7, BlockDecryptMut, BlockEncryptMut, KeyIvInit};
use serde::{Deserialize, Serialize};

use crate::error::{DromeError, Result};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EncryptResult {
    pub iv: String,
    pub encrypted_data: String,
}

fn ensure_key_32(secret_key: &str) -> Result<[u8; 32]> {
    let bytes = secret_key.as_bytes();
    if bytes.len() != 32 {
        return Err(DromeError::Message(
            "secretKey must be 32 bytes for aes-256-cbc".into(),
        ));
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(bytes);
    Ok(out)
}

fn parse_iv_hex(iv_hex: &str) -> Result<[u8; 16]> {
    let iv_bytes =
        hex::decode(iv_hex).map_err(|e| DromeError::Message(format!("Invalid iv hex: {e}")))?;
    if iv_bytes.len() != 16 {
        return Err(DromeError::Message(
            "iv must be 16 bytes (32 hex chars)".into(),
        ));
    }
    let mut out = [0u8; 16];
    out.copy_from_slice(&iv_bytes);
    Ok(out)
}

pub fn aes_encrypt(text: String, secret_key: String, iv_hex: String) -> Result<EncryptResult> {
    let key = ensure_key_32(&secret_key)?;
    let iv = parse_iv_hex(&iv_hex)?;

    let mut buf = text.into_bytes();
    let pos = buf.len();
    // PKCS7 padding needs extra capacity up to block size.
    buf.resize(pos + 16, 0u8);

    let ciphertext = Encryptor::<Aes256>::new(&key.into(), &iv.into())
        .encrypt_padded_mut::<Pkcs7>(&mut buf, pos)
        .map_err(|e| DromeError::Message(format!("Encrypt failed: {e}")))?;

    Ok(EncryptResult {
        iv: hex::encode(iv),
        encrypted_data: hex::encode(ciphertext),
    })
}

pub fn aes_decrypt(encrypted_hex: String, iv_hex: String, secret_key: String) -> Result<String> {
    let key = ensure_key_32(&secret_key)?;
    let iv = parse_iv_hex(&iv_hex)?;

    let mut buf = hex::decode(encrypted_hex)
        .map_err(|e| DromeError::Message(format!("Invalid encrypted hex: {e}")))?;

    let plaintext = Decryptor::<Aes256>::new(&key.into(), &iv.into())
        .decrypt_padded_mut::<Pkcs7>(&mut buf)
        .map_err(|e| DromeError::Message(format!("Decrypt failed: {e}")))?;

    String::from_utf8(plaintext.to_vec())
        .map_err(|e| DromeError::Message(format!("Invalid utf8: {e}")))
}
