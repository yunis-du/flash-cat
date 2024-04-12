use aes_gcm::{
    aead::{generic_array::GenericArray, Aead},
    {Aes256Gcm, KeyInit},
};
use bytes::Bytes;
use hex::encode_upper;
use sha2::{Digest, Sha256};
use anyhow::Result;

use crate::consts;

#[derive(Clone)]
pub struct Encryptor {
    share_code: String,
    cipher: Aes256Gcm,
}

impl Encryptor {
    pub fn new(share_code: String) -> Result<Self> {
        if share_code.len() != 12 {
            return Err(anyhow::Error::msg(
                "share code length must be 12".to_string(),
            ));
        }
        Ok(Self {
            share_code,
            cipher: aes_gcm::Aes256Gcm::new(consts::DEFAULT_SECRET_KEY.as_bytes().into()),
        })
    }

    pub fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>> {
        let ciphertext = self
            .cipher
            .encrypt(
                GenericArray::from_slice(self.share_code.as_bytes()),
                plaintext,
            )
            .map_err(|op| anyhow::Error::msg(op.to_string()))?;
        Ok(ciphertext)
    }

    pub fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>> {
        let plaintext = self
            .cipher
            .decrypt(
                GenericArray::from_slice(self.share_code.as_bytes()),
                ciphertext,
            )
            .map_err(|op| anyhow::Error::msg(op.to_string()))?;
        Ok(plaintext)
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

#[cfg(test)]
mod test {
    use anyhow::Result;

    use crate::utils::gen_share_code;

    use super::Encryptor;

    #[test]
    fn encryptor_test() -> Result<()> {
        let plaintext = b"Hello, Bob! This is a secret message.";

        let share_code = gen_share_code();
        let encryptor = Encryptor::new(share_code.clone())?;
        let encrypted_text = encryptor.encrypt(plaintext)?;
        println!("encrypted_text len: {}", encrypted_text.len());

        let encryptor = Encryptor::new(share_code.clone())?;
        let decrypted_text = match encryptor.decrypt(&encrypted_text) {
            Ok(decrypted_text) => decrypted_text,
            Err(err) => return Err(anyhow::Error::msg(err.to_string())),
        };
        println!(
            "Decrypted Text: {:?}",
            String::from_utf8_lossy(&decrypted_text)
        );
        Ok(())
    }
}
