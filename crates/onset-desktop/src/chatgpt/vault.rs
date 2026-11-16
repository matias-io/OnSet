//! The account vault: connected ChatGPT accounts and their tokens, encrypted for the current
//! Windows user with DPAPI and written
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

    /// Brings accounts over from the earlier Electron desktop app, once.
    ///
    /// That app kept the same vault JSON in `accounts.enc`, sealed with Electron `safeStorage`,
    /// whose key lives in its profile's `Local State`. After a successful import the old file is
    /// renamed so that disconnecting every account later does not bring them back.
    pub fn import_electron(&self, local_states: &[PathBuf]) -> Result<Option<Vault>, String> {
        let Some(home) = self.path.parent() else {
            return Ok(None);
        };
        let old = home.join("accounts.enc");
        let Ok(sealed) = std::fs::read(&old) else {
            return Ok(None);
        };
        let mut last = "The earlier app's key file (Local State) was not found".to_string();
        for state in local_states {
            let Ok(local_state) = std::fs::read(state) else {
                continue;
            };
            match open_electron(&sealed, &local_state) {
                Ok(vault) => {
                    self.save(&vault)?;
                    std::fs::rename(&old, home.join("accounts.enc.imported"))
                        .map_err(|e| e.to_string())?;
                    return Ok(Some(vault));
                }
                Err(e) => last = e,
            }
        }
        Err(last)
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

/// Opens Electron `safeStorage` data on Windows: `v10`, a 12-byte nonce, then AES-256-GCM
/// ciphertext and tag. The key is `os_crypt.encrypted_key` in `Local State`: base64 of `DPAPI`
/// followed by a DPAPI blob for this Windows user, without extra entropy.
fn open_electron(sealed: &[u8], local_state: &[u8]) -> Result<Vault, String> {
    use base64::Engine as _;
    use ring::aead::{AES_256_GCM, Aad, LessSafeKey, Nonce, UnboundKey};
    let state: serde_json::Value = serde_json::from_slice(local_state)
        .map_err(|_| "The earlier app's key file (Local State) is unreadable")?;
    let encoded = state["os_crypt"]["encrypted_key"]
        .as_str()
        .ok_or("The earlier app's key file (Local State) has no key")?;
    let wrapped = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| "The earlier app's key is malformed")?;
    let wrapped = wrapped
        .strip_prefix(b"DPAPI")
        .ok_or("The earlier app's key is not protected for this Windows user")?;
    let key = dpapi::unprotect_with(wrapped, &[])
        .map_err(|()| "The earlier app's key belongs to a different Windows user or profile")?;
    let body = sealed
        .strip_prefix(b"v10")
        .ok_or("The earlier account file is not in a recognised format")?;
    if body.len() < 12 + AES_256_GCM.tag_len() {
        return Err("The earlier account file is truncated".into());
    }
    let (nonce, ciphertext) = body.split_at(12);
    let key = UnboundKey::new(&AES_256_GCM, &key)
        .map(LessSafeKey::new)
        .map_err(|_| "The earlier app's key has the wrong length")?;
    let nonce = Nonce::try_assume_unique_for_key(nonce).map_err(|_| "Bad nonce")?;
    let mut buffer = ciphertext.to_vec();
    let plain = key
        .open_in_place(nonce, Aad::empty(), &mut buffer)
        .map_err(|_| "The earlier account file does not match its key file")?;
    serde_json::from_slice(plain).map_err(|_| "The earlier account file is damaged".into())
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
        protect_with(data, super::ENTROPY)
    }

    pub fn unprotect(data: &[u8]) -> Result<Vec<u8>, ()> {
        unprotect_with(data, super::ENTROPY)
    }

    /// No entropy is passed as a null pointer, as Chromium and Electron do.
    fn entropy_ptr(entropy: &CRYPT_INTEGER_BLOB) -> *const CRYPT_INTEGER_BLOB {
        if entropy.cbData == 0 {
            std::ptr::null()
        } else {
            &raw const *entropy
        }
    }

    pub fn protect_with(data: &[u8], entropy: &[u8]) -> Result<Vec<u8>, ()> {
        let input = blob(data);
        let entropy = blob(entropy);
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
                entropy_ptr(&entropy),
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &raw mut out,
            )
        };
        if ok == 0 { Err(()) } else { Ok(take(&out)) }
    }

    pub fn unprotect_with(data: &[u8], entropy: &[u8]) -> Result<Vec<u8>, ()> {
        let input = blob(data);
        let entropy = blob(entropy);
        let mut out = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: std::ptr::null_mut(),
        };
        // SAFETY: as in `protect`.
        let ok = unsafe {
            CryptUnprotectData(
                &raw const input,
                std::ptr::null_mut(),
                entropy_ptr(&entropy),
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
    pub fn unprotect_with(_: &[u8], _: &[u8]) -> Result<Vec<u8>, ()> {
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

    /// Writes `accounts.enc` and a `Local State` the way Electron `safeStorage` does on Windows.
    fn electron_files(dir: &Path, vault: &Vault, key: [u8; 32]) -> PathBuf {
        use base64::Engine as _;
        use ring::aead::{AES_256_GCM, Aad, LessSafeKey, Nonce, UnboundKey};
        let wrapped = [
            b"DPAPI".as_slice(),
            &dpapi::protect_with(&key, &[]).unwrap(),
        ]
        .concat();
        let state = serde_json::json!({"os_crypt": {
            "encrypted_key": base64::engine::general_purpose::STANDARD.encode(wrapped)}});
        let local_state = dir.join("Local State");
        std::fs::write(&local_state, serde_json::to_vec(&state).unwrap()).unwrap();
        let nonce = [7u8; 12];
        let mut data = serde_json::to_vec(vault).unwrap();
        LessSafeKey::new(UnboundKey::new(&AES_256_GCM, &key).unwrap())
            .seal_in_place_append_tag(Nonce::assume_unique_for_key(nonce), Aad::empty(), &mut data)
            .unwrap();
        std::fs::write(
            dir.join("accounts.enc"),
            [b"v10".as_slice(), &nonce, &data].concat(),
        )
        .unwrap();
        local_state
    }

    #[test]
    fn imports_the_earlier_electron_vault_once() {
        let dir = tempfile::tempdir().unwrap();
        let mut old = Vault::new("urn:uuid:electron-host".into());
        old.profiles.push(profile());
        old.active = Some("oaiapp_1".into());
        let local_state = electron_files(dir.path(), &old, [3; 32]);
        let store = Store::new(dir.path());
        let missing = dir.path().join("missing").join("Local State");
        assert_eq!(
            store
                .import_electron(&[missing, local_state.clone()])
                .unwrap(),
            Some(old.clone())
        );
        // Saved to the DPAPI vault, and the old file is set aside so it is not imported again.
        assert_eq!(store.load().unwrap(), Some(old));
        assert!(!dir.path().join("accounts.enc").exists());
        assert!(dir.path().join("accounts.enc.imported").exists());
        assert_eq!(store.import_electron(&[local_state]).unwrap(), None);
    }

    #[test]
    fn a_mismatched_key_file_leaves_everything_in_place() {
        let dir = tempfile::tempdir().unwrap();
        let mut old = Vault::new("urn:uuid:electron-host".into());
        old.profiles.push(profile());
        electron_files(dir.path(), &old, [3; 32]);
        let other = dir.path().join("other");
        std::fs::create_dir(&other).unwrap();
        let wrong = electron_files(&other, &old, [9; 32]);
        let store = Store::new(dir.path());
        assert!(store.import_electron(&[wrong]).is_err());
        assert!(dir.path().join("accounts.enc").exists());
        assert_eq!(store.load().unwrap(), None);
        assert!(store.import_electron(&[]).is_err());
    }

    #[test]
    fn a_damaged_file_is_an_error_not_an_empty_vault() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("onset-accounts.dpapi"), b"not dpapi").unwrap();
        assert!(Store::new(dir.path()).load().is_err());
    }
}
