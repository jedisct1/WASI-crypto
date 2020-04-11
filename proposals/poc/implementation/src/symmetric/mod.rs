mod aes_gcm;
mod hmac_sha2;
mod key;
mod key_manager;
mod sha2;
mod state;
mod tag;

use crate::error::*;
use crate::handles::*;
use crate::options::*;
use aes_gcm::*;
use hmac_sha2::*;
use parking_lot::Mutex;
use sha2::*;
use std::any::Any;
use std::convert::TryFrom;
use std::sync::Arc;

pub use key::SymmetricKey;
pub use key_manager::*;
pub use state::SymmetricState;
pub use tag::SymmetricTag;

#[derive(Debug, Default)]
pub struct SymmetricOptionsInner {
    context: Option<Vec<u8>>,
    salt: Option<Vec<u8>>,
    nonce: Option<Vec<u8>>,
    memory_limit: Option<u64>,
    ops_limit: Option<u64>,
    parallelism: Option<u64>,
}

#[derive(Clone, Debug)]
pub struct SymmetricOptions {
    inner: Arc<Mutex<SymmetricOptionsInner>>,
}

impl Default for SymmetricOptions {
    fn default() -> Self {
        SymmetricOptions {
            inner: Default::default(),
        }
    }
}

impl OptionsLike for SymmetricOptions {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn set(&mut self, name: &str, value: &[u8]) -> Result<(), CryptoError> {
        let mut inner = self.inner.lock();
        let option = match name.to_lowercase().as_str() {
            "context" => &mut inner.context,
            "salt" => &mut inner.salt,
            "nonce" => &mut inner.nonce,
            _ => bail!(CryptoError::UnsupportedOption),
        };
        *option = Some(value.to_vec());
        Ok(())
    }

    fn get(&self, name: &str) -> Result<Vec<u8>, CryptoError> {
        let inner = self.inner.lock();
        let value = match name.to_lowercase().as_str() {
            "context" => &inner.context,
            "salt" => &inner.salt,
            "nonce" => &inner.nonce,
            _ => bail!(CryptoError::UnsupportedOption),
        };
        value.as_ref().cloned().ok_or(CryptoError::OptionNotSet)
    }

    fn set_u64(&mut self, name: &str, value: u64) -> Result<(), CryptoError> {
        let mut inner = self.inner.lock();
        let option = match name.to_lowercase().as_str() {
            "memory_limit" => &mut inner.memory_limit,
            "ops_limit" => &mut inner.ops_limit,
            "parallelism" => &mut inner.parallelism,
            _ => bail!(CryptoError::UnsupportedOption),
        };
        *option = Some(value);
        Ok(())
    }

    fn get_u64(&self, name: &str) -> Result<u64, CryptoError> {
        let inner = self.inner.lock();
        let value = match name.to_lowercase().as_str() {
            "memory_limit" => &inner.memory_limit,
            "ops_limit" => &inner.ops_limit,
            "parallelism" => &inner.parallelism,
            _ => bail!(CryptoError::UnsupportedOption),
        };
        value.ok_or(CryptoError::OptionNotSet)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SymmetricAlgorithm {
    None,
    HmacSha256,
    HmacSha512,
    Sha256,
    Sha512,
    Sha512_256,
    Aes128Gcm,
    Aes256Gcm,
}

impl TryFrom<&str> for SymmetricAlgorithm {
    type Error = CryptoError;

    fn try_from(alg_str: &str) -> Result<Self, CryptoError> {
        match alg_str {
            "HMAC/SHA-256" => Ok(SymmetricAlgorithm::HmacSha256),
            "HMAC/SHA-512" => Ok(SymmetricAlgorithm::HmacSha512),
            "SHA-256" => Ok(SymmetricAlgorithm::Sha256),
            "SHA-512" => Ok(SymmetricAlgorithm::Sha512),
            "SHA-512/256" => Ok(SymmetricAlgorithm::Sha512_256),
            "AES-128-GCM" => Ok(SymmetricAlgorithm::Aes128Gcm),
            "AES-256-GCM" => Ok(SymmetricAlgorithm::Aes256Gcm),
            _ => bail!(CryptoError::UnsupportedAlgorithm),
        }
    }
}

#[test]
fn test_hash() {
    use crate::CryptoCtx;

    let ctx = CryptoCtx::new();

    let state_handle = ctx.symmetric_state_open("SHA-256", None, None).unwrap();
    ctx.symmetric_state_absorb(state_handle, b"data").unwrap();
    ctx.symmetric_state_absorb(state_handle, b"more_data")
        .unwrap();
    let mut out = [0u8; 32];
    ctx.symmetric_state_squeeze(state_handle, &mut out).unwrap();
    let expected = [
        227, 176, 196, 66, 152, 252, 28, 20, 154, 251, 244, 200, 153, 111, 185, 36, 39, 174, 65,
        228, 100, 155, 147, 76, 164, 149, 153, 27, 120, 82, 184, 85,
    ];
    assert_eq!(out, expected);
    ctx.symmetric_state_close(state_handle).unwrap();
}

#[test]
fn test_hmac() {
    use crate::CryptoCtx;

    let ctx = CryptoCtx::new();

    let key_handle = ctx.symmetric_key_generate("HMAC/SHA-512", None).unwrap();
    let state_handle = ctx
        .symmetric_state_open("HMAC/SHA-512", Some(key_handle), None)
        .unwrap();
    ctx.symmetric_state_absorb(state_handle, b"data").unwrap();
    ctx.symmetric_state_absorb(state_handle, b"more_data")
        .unwrap();

    let tag_handle = ctx.symmetric_state_squeeze_tag(state_handle).unwrap();
    let raw_tag = tag_to_vec(&ctx, tag_handle).unwrap();

    let tag_handle = ctx.symmetric_state_squeeze_tag(state_handle).unwrap();
    ctx.symmetric_tag_verify(tag_handle, &raw_tag).unwrap();

    ctx.symmetric_state_close(state_handle).unwrap();
    ctx.symmetric_key_close(key_handle).unwrap();
    ctx.symmetric_tag_close(tag_handle).unwrap();
}

#[test]
fn test_encryption() {
    use crate::CryptoCtx;

    let ctx = CryptoCtx::new();

    let msg = b"test";
    let nonce = [42u8; 12];
    let key_handle = ctx.symmetric_key_generate("AES-256-GCM", None).unwrap();

    let options_handle = ctx.options_open(OptionsType::Symmetric).unwrap();
    ctx.options_set(options_handle, "nonce", &nonce).unwrap();

    let symmetric_state = ctx
        .symmetric_state_open("AES-256-GCM", Some(key_handle), Some(options_handle))
        .unwrap();
    let mut observed_nonce = [0u8; 12];
    ctx.symmetric_state_options_get(symmetric_state, "nonce", &mut observed_nonce)
        .unwrap();
    assert_eq!(&nonce, &observed_nonce);

    let mut ciphertext_with_tag =
        vec![0u8; msg.len() + ctx.symmetric_state_max_tag_len(symmetric_state).unwrap()];
    ctx.symmetric_state_encrypt(symmetric_state, &mut ciphertext_with_tag, msg)
        .unwrap();
    ctx.symmetric_state_close(symmetric_state).unwrap();

    let symmetric_state = ctx
        .symmetric_state_open("AES-256-GCM", Some(key_handle), Some(options_handle))
        .unwrap();
    let mut msg2 = vec![0u8; msg.len()];
    ctx.symmetric_state_decrypt(symmetric_state, &mut msg2, &ciphertext_with_tag)
        .unwrap();
    ctx.symmetric_state_close(symmetric_state).unwrap();
    assert_eq!(msg, &msg2[..]);

    let symmetric_state = ctx
        .symmetric_state_open("AES-256-GCM", Some(key_handle), Some(options_handle))
        .unwrap();
    let mut ciphertext = vec![0u8; msg.len()];
    let tag_handle = ctx
        .symmetric_state_encrypt_detached(symmetric_state, &mut ciphertext, msg)
        .unwrap();
    ctx.symmetric_state_close(symmetric_state).unwrap();

    let raw_tag = tag_to_vec(&ctx, tag_handle).unwrap();

    let symmetric_state = ctx
        .symmetric_state_open("AES-256-GCM", Some(key_handle), Some(options_handle))
        .unwrap();
    let mut msg2 = vec![0u8; msg.len()];
    ctx.symmetric_state_decrypt_detached(symmetric_state, &mut msg2, &ciphertext, &raw_tag)
        .unwrap();
    ctx.symmetric_state_close(symmetric_state).unwrap();
    assert_eq!(msg, &msg2[..]);
}

#[cfg(test)]
fn tag_to_vec(ctx: &crate::CryptoCtx, symmetric_tag: Handle) -> Result<Vec<u8>, CryptoError> {
    let mut bytes = vec![0u8; ctx.symmetric_tag_len(symmetric_tag)?];
    ctx.symmetric_tag_pull(symmetric_tag, &mut bytes)?;
    Ok(bytes)
}