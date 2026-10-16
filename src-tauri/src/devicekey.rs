//! This device's key: the Ed25519 key your other devices know it by (peers.rs). Over iroh it's the
//! address they dial; over Tailscale each side proves it holds its key when a connection
//! opens. It's made the first time `[devices]` is turned on and kept in the login keychain
//! (service `Codealong device key`, os/secrets.rs), like Jev's API key, so it never sits in a file
//! and a reinstall keeps this device's links.

use anyhow::{anyhow, bail, Result};
use data_encoding::HEXLOWER;
use iroh::SecretKey;

const SERVICE: &str = "Codealong device key";

/// The key in the keychain, made and saved there first if there's none. `CODEALONG_DEVICE_KEY`
/// (hex) stands in for it, so a second copy of the app on one computer can be a device of its own.
pub fn load_or_create() -> Result<SecretKey> {
    if let Ok(hex) = std::env::var("CODEALONG_DEVICE_KEY") {
        return decode(&hex);
    }
    if let Some(k) = read() {
        return Ok(k);
    }
    let key = SecretKey::generate();
    save(&key)?;
    // The keychain is where the key lives; one it didn't keep would change on every launch.
    match read() {
        Some(k) if k.public() == key.public() => Ok(k),
        _ => bail!("The keychain didn't keep this device's key."),
    }
}

fn read() -> Option<SecretKey> {
    decode(&crate::os::secrets::read(SERVICE)?).ok()
}

fn save(key: &SecretKey) -> Result<()> {
    crate::os::secrets::save(SERVICE, &HEXLOWER.encode(&key.to_bytes())).map_err(|_| anyhow!("The keychain didn't take this device's key."))
}

fn decode(hex: &str) -> Result<SecretKey> {
    let bytes = HEXLOWER.decode(hex.trim().to_lowercase().as_bytes()).map_err(|_| anyhow!("The device key isn't hex."))?;
    let bytes: [u8; 32] = bytes.try_into().map_err(|_| anyhow!("The device key is the wrong length."))?;
    Ok(SecretKey::from_bytes(&bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_decode_from_hex() {
        let k = SecretKey::generate();
        let hex = HEXLOWER.encode(&k.to_bytes());
        assert_eq!(decode(&hex).unwrap().public(), k.public());
        assert_eq!(decode(&format!(" {} ", hex.to_uppercase())).unwrap().public(), k.public());
        assert!(decode("abcd").is_err());
    }
}
