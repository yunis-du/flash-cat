use std::fmt::Debug;

use aes_gcm::{
    aead::{Aead, AeadInPlace, generic_array::GenericArray},
    {Aes256Gcm, KeyInit},
};
use anyhow::{Result, anyhow, bail};
use bytes::{Bytes, BytesMut};
use hex::encode_upper;
use sha2::{Digest, Sha256};

use crate::consts;

#[derive(Clone)]
pub struct CustomAes256Gcm(Aes256Gcm);

#[derive(Debug, Clone)]
pub struct Encryptor {
    share_code: String,
    cipher: CustomAes256Gcm,
}

impl Encryptor {
    pub fn new(share_code: String) -> Result<Self> {
        if share_code.len() != 12 {
            bail!("share code length must be 12");
        }
        Ok(Self {
            share_code,
            cipher: CustomAes256Gcm(aes_gcm::Aes256Gcm::new(consts::DEFAULT_SECRET_KEY.as_bytes().into())),
        })
    }

    pub fn encrypt(
        &self,
        plaintext: &[u8],
    ) -> Result<Vec<u8>> {
        let ciphertext = self.cipher.0.encrypt(GenericArray::from_slice(self.share_code.as_bytes()), plaintext).map_err(|op| anyhow!(op.to_string()))?;
        Ok(ciphertext)
    }

    pub fn decrypt(
        &self,
        ciphertext: &[u8],
    ) -> Result<Vec<u8>> {
        let plaintext = self.cipher.0.decrypt(GenericArray::from_slice(self.share_code.as_bytes()), ciphertext).map_err(|op| anyhow!(op.to_string()))?;
        Ok(plaintext)
    }

    /// Encrypt the owned read buffer, appending the same GCM tag as encrypt().
    pub fn encrypt_in_place(
        &self,
        plaintext: &mut Vec<u8>,
    ) -> Result<()> {
        self.cipher.0.encrypt_in_place(GenericArray::from_slice(self.share_code.as_bytes()), b"", plaintext).map_err(|e| anyhow!(e.to_string()))
    }

    /// Reuse an exclusively owned protobuf buffer; shared network slices need a
    /// single copy before mutation. Authenticate before handing bytes to a writer.
    pub fn decrypt_owned(
        &self,
        ciphertext: Bytes,
    ) -> Result<Bytes> {
        const TAG_SIZE: usize = 16;
        if ciphertext.len() < TAG_SIZE {
            bail!("ciphertext is shorter than the authentication tag");
        }
        let mut buffer = ciphertext.try_into_mut().unwrap_or_else(|shared| BytesMut::from(shared.as_ref()));
        let plaintext_len = buffer.len() - TAG_SIZE;
        let (plaintext, tag) = buffer.split_at_mut(plaintext_len);
        self.cipher
            .0
            .decrypt_in_place_detached(
                GenericArray::from_slice(self.share_code.as_bytes()),
                b"",
                plaintext,
                GenericArray::from_slice(tag),
            )
            .map_err(|e| anyhow!(e.to_string()))?;
        buffer.truncate(plaintext_len);
        Ok(buffer.freeze())
    }

    pub fn get_share_code(&self) -> String {
        self.share_code.clone()
    }

    pub fn encrypt_share_code(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.share_code.clone());
        let result = hasher.finalize();
        encode_upper(result)
    }

    pub fn encrypt_share_code_bytes(&self) -> Bytes {
        let mut hasher = Sha256::new();
        hasher.update(self.share_code.clone());
        let result = hasher.finalize();
        Bytes::copy_from_slice(result.as_slice())
    }
}

impl Debug for CustomAes256Gcm {
    fn fmt(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        f.debug_tuple("CustomAes256Gcm").finish()
    }
}
