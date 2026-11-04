//! Passphrase encryption in the age format (07 §7.7): exports any age tool can open.

use zeroize::Zeroizing;

/// Why an age export or import failed. Never carries plaintext or ciphertext bytes (J-260).
#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    /// age could not write the file; the text is age's, about I/O or the recipient.
    #[error("age encryption failed: {0}")]
    Encrypt(age::EncryptError),
    /// Deliberately does not say which: a wrong passphrase and a file that is not ours look
    /// the same, so the message gives nothing away about the file.
    #[error("wrong passphrase or not a KeyHarbor export")]
    WrongPassphraseOrFormat,
}

/// A passphrase, zeroized on drop and never printed.
pub struct Passphrase(age::secrecy::SecretString);

impl Passphrase {
    pub fn new(value: String) -> Self {
        Self(age::secrecy::SecretString::from(value))
    }
}

impl std::fmt::Debug for Passphrase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Passphrase([REDACTED])")
    }
}

/// `plaintext` as an age file for the scrypt recipient of `passphrase`.
pub fn encrypt_age(passphrase: Passphrase, plaintext: &[u8]) -> Result<Vec<u8>, ExportError> {
    age::encrypt(&age::scrypt::Recipient::new(passphrase.0), plaintext)
        .map_err(ExportError::Encrypt)
}

/// The plaintext of an age file encrypted for `passphrase`.
pub fn decrypt_age(
    passphrase: Passphrase,
    encrypted: &[u8],
) -> Result<Zeroizing<Vec<u8>>, ExportError> {
    age::decrypt(&age::scrypt::Identity::new(passphrase.0), encrypted)
        .map(Zeroizing::new)
        .map_err(|_| ExportError::WrongPassphraseOrFormat)
}
