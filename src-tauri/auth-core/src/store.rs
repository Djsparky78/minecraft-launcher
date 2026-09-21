use crate::model::{AuthError, Secret};

pub trait CredentialStore: Send + Sync {
    fn load(&self, client_id: &str) -> Result<Option<Secret>, AuthError>;
    fn save(&self, client_id: &str, token: &Secret) -> Result<(), AuthError>;
    fn delete(&self, client_id: &str) -> Result<(), AuthError>;
}
pub struct OsStore;
#[cfg(windows)]
fn entry(client_id: &str) -> Result<keyring::Entry, AuthError> {
    keyring::Entry::new("com.ember.launcher.microsoft", client_id).map_err(|_| storage_error())
}
fn storage_error() -> AuthError {
    AuthError::new("secure_storage", "Windows Credential Manager could not access the saved sign-in. Check your Windows user session and retry. Ember will not save credentials in a plain-text file.")
}
impl CredentialStore for OsStore {
    fn load(&self, client_id: &str) -> Result<Option<Secret>, AuthError> {
        #[cfg(windows)]
        {
            match entry(client_id)?.get_password() {
                Ok(token) => Ok(Some(Secret::new(token))),
                Err(keyring::Error::NoEntry) => Ok(None),
                Err(_) => Err(storage_error()),
            }
        }
        #[cfg(not(windows))]
        {
            let _ = client_id;
            Err(storage_error())
        }
    }
    fn save(&self, client_id: &str, token: &Secret) -> Result<(), AuthError> {
        #[cfg(windows)]
        {
            entry(client_id)?
                .set_password(token.expose())
                .map_err(|_| storage_error())
        }
        #[cfg(not(windows))]
        {
            let _ = (client_id, token);
            Err(storage_error())
        }
    }
    fn delete(&self, client_id: &str) -> Result<(), AuthError> {
        #[cfg(windows)]
        {
            match entry(client_id)?.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
                Err(_) => Err(storage_error()),
            }
        }
        #[cfg(not(windows))]
        {
            let _ = client_id;
            Err(storage_error())
        }
    }
}
