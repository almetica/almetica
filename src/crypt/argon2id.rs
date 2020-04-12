/// Implements helper functions for the argon2id password hasher.
use argon2::{hash_raw, verify_raw, Config, ThreadMode, Variant, Version};
use base64::{decode, encode};
use rand::rngs::OsRng;
use rand_core::RngCore;
use regex::Regex;
use tracing::error;

use lazy_static::lazy_static;

use crate::{Error, Result};

lazy_static! {
    static ref RE: Regex = Regex::new(
        r#"^\$(\w*)*\$v=(\d{2})\$m=(\d*),t=(\d*),p=(\d*)\$([0-9a-zA-Z+/=]*)\$([0-9a-zA-Z+/=]*)$"#
    )
    .unwrap();
}

/// Creates a String that contains the argon2id hash of the given password. Creates a random salt,
/// which is saved alongside the password hash and the used configuration.
/// Format of the resulting hash:
/// $argon2id$v=19$m=65536,t=2,p=4$c29tZXNhbHQ$RdescudvJCsgt3ub+b+dWRWJTmaaJObG
pub fn create_hash(password_data: &[u8]) -> Result<String> {
    let l = 32; // length of the hash in bytes
    let s = 16; // length of the salt in bytes
    let m = 128 * 1024; // memory to use in KiB
    let t = 3; // iterations
    let p = 8; // lanes used

    let config = create_argon2id_config(l, m, t, p);
    let mut salt_data = vec![0u8; s];
    OsRng.fill_bytes(&mut salt_data);
    let hash_data = hash_raw(&password_data, &salt_data, &config)?;

    let salt = encode(&salt_data);
    let hash = encode(&hash_data);

    let hash_string = format!("$argon2id$v=19$m={},t={},p={}${}${}", m, t, p, salt, hash);
    Ok(hash_string)
}

/// Verifies the given password hash and password. Uses the configuration and salt stored inside
/// the password hash. Returns true if the password can produce the given hash.
pub fn verify_hash(password_data: &[u8], hash_string: String) -> Result<bool> {
    if let Some(captures) = RE.captures(&hash_string) {
        if captures.len() != 8 {
            error!("password hash is stored in a wrong format: {}", hash_string);
            return Err(Error::PasswordHashWrongFormat);
        }

        let hash_function = captures.get(1).map_or("", |m| m.as_str());
        let hash_version: u32 = captures.get(2).map_or("0", |m| m.as_str()).parse()?;
        let m: u32 = captures.get(3).map_or("0", |m| m.as_str()).parse()?;
        let t: u32 = captures.get(4).map_or("0", |m| m.as_str()).parse()?;
        let p: u32 = captures.get(5).map_or("0", |m| m.as_str()).parse()?;
        let salt_base64 = captures.get(6).map_or("", |m| m.as_str());
        let hash_base64 = captures.get(7).map_or("", |m| m.as_str());
        let salt_data = decode(salt_base64)?;
        let hash_data = decode(hash_base64)?;

        if hash_function != "argon2id" || hash_version != 19 {
            error!(
                "unsupported password hash: {}:{}",
                hash_function, hash_version
            );
            return Err(Error::UnsupportedPasswordHash);
        }

        let config = create_argon2id_config(hash_data.len() as u32, m, t, p);
        let result = verify_raw(&password_data, &salt_data, &hash_data, &config)?;
        Ok(result)
    } else {
        error!("password hash is stored in a wrong format: {}", hash_string);
        Err(Error::PasswordHashWrongFormat)
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
    use super::*;

    #[test]
    fn test_argon2id_hash_creation() -> Result<()> {
        let password = "testpassword123";
        let hash_string = create_hash(password.as_bytes())?;

        if let Some(captures) = RE.captures(&hash_string) {
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
                hash_string
            )
        }
        Ok(())
    }

    #[test]
    fn test_argon2id_hash_verification() -> Result<()> {
        let password = "testpassword123";
        let hash_string = create_hash(password.as_bytes())?;
        let result = verify_hash(password.as_bytes(), hash_string)?;
        assert_eq!(result, true);
        Ok(())
    }
}
