//! Where the refresh token lives: the macOS Keychain ([`Keychain`]: service
//! the app's bundle identifier, account [`ACCOUNT`]), behind the
//! [`Secrets`] trait so the sync is tested with [`MemorySecrets`] and never
//! touches the learner's Keychain. The access token is never stored: it
//! lives in the sync's memory.

use std::fmt;
use std::sync::Mutex;

use palicanon_app_identity::BUNDLE_IDENTIFIER;

/// The Keychain item's account.
pub const ACCOUNT: &str = "google-drive";

/// Why a store refused a secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecretError {
    /// An item is already there that the store would not replace (the
    /// Keychain's `errSecDuplicateItem`: an item another build created is
    /// invisible to this build's lookup, so the update misses it and the
    /// add collides). Deleting it, then setting again, is the way out.
    Duplicate,
    /// Any other refusal (the reason).
    Other(String),
}

impl fmt::Display for SecretError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Duplicate => f.write_str("the item already exists"),
            Self::Other(why) => f.write_str(why),
        }
    }
}

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
    /// The store refused it ([`SecretError::Duplicate`] when an item it
    /// cannot replace is there).
    fn set(&self, value: &str) -> Result<(), SecretError>;

    /// Forgets it, by every way the store has; nothing to forget is no
    /// error.
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

    fn set(&self, value: &str) -> Result<(), SecretError> {
        let entry = Self::entry().map_err(SecretError::Other)?;
        entry.set_password(value).map_err(|error| {
            if is_duplicate(&error) {
                SecretError::Duplicate
            } else {
                SecretError::Other(error.to_string())
            }
        })
    }

    fn delete(&self) -> Result<(), String> {
        let through_keyring = match Self::entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(error.to_string()),
        };
        // The lookup behind `delete_credential` does not see an item
        // another build created; the delete by attributes does.
        let natively = delete_natively();
        through_keyring.and(natively)
    }
}

/// `errSecDuplicateItem`.
#[cfg(target_os = "macos")]
const DUPLICATE_ITEM: i32 = -25299;
/// `errSecItemNotFound`.
#[cfg(target_os = "macos")]
const ITEM_NOT_FOUND: i32 = -25300;

/// Whether keyring's refusal is the Keychain's duplicate item (by its
/// code, which keyring keeps as the platform failure's source).
fn is_duplicate(error: &keyring::Error) -> bool {
    #[cfg(target_os = "macos")]
    if let keyring::Error::PlatformFailure(source) = error {
        return source
            .downcast_ref::<security_framework::base::Error>()
            .is_some_and(|found| found.code() == DUPLICATE_ITEM);
    }
    let _ = error;
    false
}

/// The item deleted by service and account straight through the Security
/// framework; an absent item is no error.
#[cfg(target_os = "macos")]
fn delete_natively() -> Result<(), String> {
    match security_framework::passwords::delete_generic_password(Keychain::SERVICE, ACCOUNT) {
        Err(error) if error.code() != ITEM_NOT_FOUND => Err(error.to_string()),
        _ => Ok(()),
    }
}

#[cfg(not(target_os = "macos"))]
#[allow(clippy::unnecessary_wraps)]
fn delete_natively() -> Result<(), String> {
    Ok(())
}

/// A secret kept in memory: the tests' Keychain.
#[derive(Debug, Default)]
pub struct MemorySecrets(Mutex<Option<String>>);

impl Secrets for MemorySecrets {
    fn get(&self) -> Result<Option<String>, String> {
        Ok(self.0.lock().map_err(|e| e.to_string())?.clone())
    }

    fn set(&self, value: &str) -> Result<(), SecretError> {
        *self
            .0
            .lock()
            .map_err(|e| SecretError::Other(e.to_string()))? = Some(value.to_string());
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

    #[cfg(target_os = "macos")]
    #[test]
    fn only_the_duplicate_item_code_is_the_duplicate() {
        let platform = |code| {
            keyring::Error::PlatformFailure(Box::new(security_framework::base::Error::from_code(
                code,
            )))
        };
        assert!(is_duplicate(&platform(-25299)));
        assert!(!is_duplicate(&platform(-25293)));
        assert!(!is_duplicate(&keyring::Error::NoEntry));
        assert!(!is_duplicate(&keyring::Error::PlatformFailure(
            "item already exists".into()
        )));
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
