//! The account vault: connected ChatGPT accounts and their tokens, encrypted for the current
//! Windows user with DPAPI (what Electron's safeStorage used underneath) and written
//! atomically. The screens never see tokens; only the status in [`super::Status`] leaves.
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub client_id: String,
    pub subject: String,
    pub email: String,
    pub access_token: String,
    pub refresh_token: String,
    pub id_token: String,
    /// Milliseconds since the Unix epoch.
    pub expires_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub earliest_refresh_at: Option<serde_json::Value>,
    pub scopes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_model: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Vault {
    /// This installation's agent host id, sent when registering a new client.
    pub host: String,
    pub active: Option<String>,
    pub profiles: Vec<Profile>,
}

impl Vault {
    pub fn new(host: String) -> Self {
        Self {
            host,
            active: None,
            profiles: Vec::new(),
        }
    }

    pub fn active_profile(&self) -> Option<&Profile> {
        let id = self.active.as_deref()?;
        self.profiles.iter().find(|p| p.client_id == id)
    }
}

/// Mixed into the encryption so another app's DPAPI blob for this user does not decrypt here.
const ENTROPY: &[u8] = b"OnSet ChatGPT account vault v1";

pub struct Store {
    path: PathBuf,
}

impl Store {
    pub fn new(home: &Path) -> Self {
        Self {
            path: home.join("onset-accounts.dpapi"),
        }
    }

    /// The saved vault; `Ok(None)` when none has been saved yet.
    pub fn load(&self) -> Result<Option<Vault>, String> {
        let bytes = match std::fs::read(&self.path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err("Protected account storage could not be opened".into()),
        };
        let plain = dpapi::unprotect(&bytes)
            .map_err(|()| "Protected account storage could not be opened")?;
        serde_json::from_slice(&plain)
            .map(Some)
            .map_err(|_| "Protected account storage is damaged".into())
    }

    pub fn save(&self, vault: &Vault) -> Result<(), String> {
        let plain = serde_json::to_vec(vault).map_err(|e| e.to_string())?;
        let sealed = dpapi::protect(&plain)
            .map_err(|()| "Windows credential protection is unavailable".to_string())?;
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let tmp = self.path.with_extension("dpapi.tmp");
        std::fs::write(&tmp, sealed).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &self.path).map_err(|e| e.to_string())
    }
}

#[cfg(windows)]
mod dpapi {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
    };

    fn blob(data: &[u8]) -> CRYPT_INTEGER_BLOB {
        CRYPT_INTEGER_BLOB {
            cbData: u32::try_from(data.len()).unwrap_or(u32::MAX),
            pbData: data.as_ptr().cast_mut(),
        }
    }

    /// Copies DPAPI's output and frees it.
    fn take(out: &CRYPT_INTEGER_BLOB) -> Vec<u8> {
        // SAFETY: DPAPI returned `cbData` bytes at `pbData`, allocated with LocalAlloc.
        let v = unsafe { std::slice::from_raw_parts(out.pbData, out.cbData as usize) }.to_vec();
        // SAFETY: the buffer came from DPAPI and is freed exactly once.
        unsafe { LocalFree(out.pbData.cast()) };
        v
    }

    pub fn protect(data: &[u8]) -> Result<Vec<u8>, ()> {
        let input = blob(data);
        let entropy = blob(super::ENTROPY);
        let mut out = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: std::ptr::null_mut(),
        };
        // SAFETY: input and entropy point at live slices for the call; out receives a
        // DPAPI-owned buffer that `take` frees.
        let ok = unsafe {
            CryptProtectData(
                &raw const input,
                std::ptr::null(),
                &raw const entropy,
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &raw mut out,
            )
        };
        if ok == 0 { Err(()) } else { Ok(take(&out)) }
    }

    pub fn unprotect(data: &[u8]) -> Result<Vec<u8>, ()> {
        let input = blob(data);
        let entropy = blob(super::ENTROPY);
        let mut out = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: std::ptr::null_mut(),
        };
        // SAFETY: as in `protect`.
        let ok = unsafe {
            CryptUnprotectData(
                &raw const input,
                std::ptr::null_mut(),
                &raw const entropy,
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &raw mut out,
            )
        };
        if ok == 0 { Err(()) } else { Ok(take(&out)) }
    }
}

#[cfg(not(windows))]
mod dpapi {
    pub fn protect(_: &[u8]) -> Result<Vec<u8>, ()> {
        Err(())
    }
    pub fn unprotect(_: &[u8]) -> Result<Vec<u8>, ()> {
        Err(())
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    fn profile() -> Profile {
        Profile {
            client_id: "oaiapp_1".into(),
            subject: "user-1".into(),
            email: "dj@example.invalid".into(),
            access_token: "access".into(),
            refresh_token: "refresh".into(),
            id_token: "id".into(),
            expires_at: 1,
            earliest_refresh_at: None,
            scopes: vec!["chatgpt.tokens.use.direct".into()],
            selected_model: Some("m".into()),
        }
    }

    #[test]
    fn round_trips_and_is_not_plaintext_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        assert_eq!(store.load().unwrap(), None);
        let mut vault = Vault::new("urn:uuid:test".into());
        vault.profiles.push(profile());
        vault.active = Some("oaiapp_1".into());
        store.save(&vault).unwrap();
        let raw = std::fs::read(dir.path().join("onset-accounts.dpapi")).unwrap();
        assert!(!raw.windows(7).any(|w| w == b"refresh"));
        assert_eq!(store.load().unwrap(), Some(vault.clone()));
        assert_eq!(vault.active_profile().unwrap().email, "dj@example.invalid");
    }

    #[test]
    fn a_damaged_file_is_an_error_not_an_empty_vault() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("onset-accounts.dpapi"), b"not dpapi").unwrap();
        assert!(Store::new(dir.path()).load().is_err());
    }
}
