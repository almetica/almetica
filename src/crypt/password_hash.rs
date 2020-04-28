/// Implements helper functions for the password hasher.
use anyhow::bail;
use argon2::{hash_encoded, verify_encoded, Config, ThreadMode, Variant, Version};
use rand::rngs::OsRng;
use rand_core::RngCore;

use crate::model::PasswordHashAlgorithm;
use crate::{AlmeticaError, Result};

/// Creates a String that contains the hash of the given password hashed with the chosen password
/// hash algorithm. Creates a random salt, which is saved alongside the password hash and the used
/// configuration.
/// Example format of the resulting hash for argon2id:
/// $argon2id$v=19$m=65536,t=2,p=4$c29tZXNhbHQ$RdescudvJCsgt3ub+b+dWRWJTmaaJObG
pub fn create_hash(password_data: &[u8], algorithm: PasswordHashAlgorithm) -> Result<String> {
    if algorithm == PasswordHashAlgorithm::Argon2 {
        let l = 32; // length of the hash in bytes
        let s = 16; // length of the salt in bytes
        let m = 128 * 1024; // memory to use in KiB
        let t = 3; // iterations
        let p = 8; // lanes used

        let config = create_argon2id_config(l, m, t, p);
        let mut salt_data = vec![0u8; s];
        OsRng.fill_bytes(&mut salt_data);

        Ok(hash_encoded(&password_data, &salt_data, &config)?)
    } else {
        bail!(AlmeticaError::UnsupportedPasswordHash);
    }
}

/// Verifies the given password hash, password and algorithm. Returns true if the password can produce the given hash.
pub fn verify_hash(
    password_data: &[u8],
    hash_string: &str,
    algorithm: PasswordHashAlgorithm,
) -> Result<bool> {
    if algorithm == PasswordHashAlgorithm::Argon2 {
        Ok(verify_encoded(hash_string, password_data)?)
    } else {
        bail!(AlmeticaError::UnsupportedPasswordHash);
    }
}

// l = length of hash, m = memory, t = iterations, p = number of lanes
fn create_argon2id_config<'a>(l: u32, m: u32, t: u32, p: u32) -> Config<'a> {
    Config {
        variant: Variant::Argon2id,
        version: Version::Version13, // 0x13 hex = 19 decimal
        mem_cost: m,
        time_cost: t,
        lanes: p,
        thread_mode: ThreadMode::Parallel,
        secret: &[],
        ad: &[],
        hash_length: l,
    }
}

#[cfg(test)]
mod tests {
    use base64::decode;
    use regex::Regex;

    use super::*;

    #[test]
    fn test_argon2id_hash_creation() -> Result<()> {
        let password = "testpassword123";
        let hash = create_hash(password.as_bytes(), PasswordHashAlgorithm::Argon2)?;

        let hash_re: Regex = Regex::new(
            r#"^\$(\w*)*\$v=(\d{2})\$m=(\d*),t=(\d*),p=(\d*)\$([0-9a-zA-Z+/=]*)\$([0-9a-zA-Z+/=]*)$"#,
        ).unwrap();

        if let Some(captures) = hash_re.captures(&hash) {
            assert_eq!(captures.len(), 8);
            assert_eq!(captures.get(1).unwrap().as_str(), "argon2id");
            assert_eq!(captures.get(2).unwrap().as_str(), "19");
            assert_eq!(captures.get(3).unwrap().as_str(), "131072");
            assert_eq!(captures.get(4).unwrap().as_str(), "3");
            assert_eq!(captures.get(5).unwrap().as_str(), "8");

            let salt_base64 = captures.get(6).unwrap().as_str();
            let hash_base64 = captures.get(7).unwrap().as_str();

            let salt_data = decode(salt_base64)?;
            let hash_data = decode(hash_base64)?;

            assert_eq!(salt_data.len(), 16);
            assert_eq!(hash_data.len(), 32);
        } else {
            panic!(
                "generated hash string wasn't captured by regular expression validator: {}",
                hash
            )
        }
        Ok(())
    }

    #[test]
    fn test_argon2id_hash_verification() -> Result<()> {
        let password = "testpassword123";
        let hash_string = create_hash(password.as_bytes(), PasswordHashAlgorithm::Argon2)?;
        assert!(verify_hash(
            password.as_bytes(),
            &hash_string,
            PasswordHashAlgorithm::Argon2,
        )?);
        Ok(())
    }
}
