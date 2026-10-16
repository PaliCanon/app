//! Where the refresh token lives: the macOS Keychain ([`Keychain`]: service
//! the app's bundle identifier, account [`ACCOUNT`]), behind the
//! [`Secrets`] trait so the sync is tested with [`MemorySecrets`] and never
//! touches the learner's Keychain. The access token is never stored: it
//! lives in the sync's memory.

use std::sync::Mutex;

use palicanon_app_identity::BUNDLE_IDENTIFIER;

/// The Keychain item's account.
pub const ACCOUNT: &str = "google-drive";

/// One secret, kept somewhere: read, written, deleted.
pub trait Secrets: Send + Sync {
    /// The secret; none when there is none.
    ///
    /// # Errors
    /// The store could not be read (the reason).
    fn get(&self) -> Result<Option<String>, String>;

    /// Keeps `value`, replacing any.
    ///
    /// # Errors
    /// The store refused it (the reason).
    fn set(&self, value: &str) -> Result<(), String>;

    /// Forgets it; nothing to forget is no error.
    ///
    /// # Errors
    /// The store refused (the reason).
    fn delete(&self) -> Result<(), String>;
}

/// The macOS Keychain item `service` = the bundle identifier, `account` =
/// [`ACCOUNT`] (a generic password).
#[derive(Debug, Clone, Copy, Default)]
pub struct Keychain;

impl Keychain {
    /// The Keychain item's service: the app's bundle identifier.
    pub const SERVICE: &'static str = BUNDLE_IDENTIFIER;

    fn entry() -> Result<keyring::Entry, String> {
        keyring::Entry::new(Self::SERVICE, ACCOUNT).map_err(|e| e.to_string())
    }
}

impl Secrets for Keychain {
    fn get(&self) -> Result<Option<String>, String> {
        match Self::entry()?.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(error.to_string()),
        }
    }

    fn set(&self, value: &str) -> Result<(), String> {
        Self::entry()?
            .set_password(value)
            .map_err(|e| e.to_string())
    }

    fn delete(&self) -> Result<(), String> {
        match Self::entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(error.to_string()),
        }
    }
}

/// A secret kept in memory: the tests' Keychain.
#[derive(Debug, Default)]
pub struct MemorySecrets(Mutex<Option<String>>);

impl Secrets for MemorySecrets {
    fn get(&self) -> Result<Option<String>, String> {
        Ok(self.0.lock().map_err(|e| e.to_string())?.clone())
    }

    fn set(&self, value: &str) -> Result<(), String> {
        *self.0.lock().map_err(|e| e.to_string())? = Some(value.to_string());
        Ok(())
    }

    fn delete(&self) -> Result<(), String> {
        *self.0.lock().map_err(|e| e.to_string())? = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_keychain_item_is_the_apps_google_drive_entry() {
        assert_eq!(Keychain::SERVICE, "app.palicanon.learn");
        assert_eq!(ACCOUNT, "google-drive");
    }

    #[test]
    fn the_memory_secret_is_kept_replaced_and_forgotten() {
        let secrets = MemorySecrets::default();
        assert_eq!(secrets.get(), Ok(None));
        secrets.set("a").unwrap();
        secrets.set("b").unwrap();
        assert_eq!(secrets.get(), Ok(Some("b".into())));
        secrets.delete().unwrap();
        secrets.delete().unwrap();
        assert_eq!(secrets.get(), Ok(None));
    }
}
