//! API keys belong to the current Windows account and an exact provider/base URL.
//! They never enter SQLite, profile JSON, snapshots, diagnostics or exports.
use crate::{Error, Result, connections::Connections};

fn unavailable() -> Error {
    Error::Validation(
        "Le coffre de clés API n’est pas accessible. Vérifie ta session Windows.".into(),
    )
}
pub fn validate_key(value: &str) -> Result<&str> {
    let value = value.trim();
    if value.is_empty() || value.len() > 2560 || !value.bytes().all(|b| (33..=126).contains(&b)) {
        return Err(Error::Validation("La clé API est vide ou invalide.".into()));
    }
    Ok(value)
}
fn target(config: &Connections) -> Result<String> {
    let config = config.clone().validated()?;
    Ok(format!(
        "Sherka/llm/{:?}/{}",
        config.llm_provider,
        config.api_base()?
    ))
}
pub fn load(config: &Connections) -> Result<Option<String>> {
    if config.llm_provider.is_local() {
        return Ok(None);
    }
    platform::load(&target(config)?)
}
pub fn set(config: &Connections, key: Option<&str>) -> Result<()> {
    if config.llm_provider.is_local() {
        return Err(Error::Validation(
            "Choisis un fournisseur API pour enregistrer une clé.".into(),
        ));
    }
    platform::set(&target(config)?, key.map(validate_key).transpose()?)
}

#[cfg(windows)]
mod platform {
    use super::*;
    use windows_sys::Win32::{
        Foundation::{ERROR_NOT_FOUND, GetLastError},
        Security::Credentials::*,
    };
    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(Some(0)).collect()
    }
    pub fn load(target: &str) -> Result<Option<String>> {
        let name = wide(target);
        let mut raw = std::ptr::null_mut();
        // SAFETY: null-terminated target, out pointer freed by CredFree below.
        if unsafe { CredReadW(name.as_ptr(), CRED_TYPE_GENERIC, 0, &mut raw) } == 0 {
            return if unsafe { GetLastError() } == ERROR_NOT_FOUND {
                Ok(None)
            } else {
                Err(unavailable())
            };
        }
        // SAFETY: CredReadW owns raw and its blob for the duration of this read.
        let result = unsafe {
            let credential = &*raw;
            if credential.CredentialBlobSize == 0
                || credential.CredentialBlobSize > 2560
                || credential.CredentialBlob.is_null()
            {
                Err(unavailable())
            } else {
                let bytes = std::slice::from_raw_parts(
                    credential.CredentialBlob,
                    credential.CredentialBlobSize as usize,
                );
                std::str::from_utf8(bytes)
                    .map(str::to_owned)
                    .map_err(|_| unavailable())
            }
        };
        unsafe { CredFree(raw.cast()) };
        result.map(Some)
    }
    pub fn set(target: &str, key: Option<&str>) -> Result<()> {
        let mut name = wide(target);
        let Some(key) = key else {
            let success = unsafe { CredDeleteW(name.as_ptr(), CRED_TYPE_GENERIC, 0) } != 0;
            return if success || unsafe { GetLastError() } == ERROR_NOT_FOUND {
                Ok(())
            } else {
                Err(unavailable())
            };
        };
        let mut blob = key.as_bytes().to_vec();
        let credential = CREDENTIALW {
            Type: CRED_TYPE_GENERIC,
            TargetName: name.as_mut_ptr(),
            CredentialBlobSize: blob.len() as u32,
            CredentialBlob: blob.as_mut_ptr(),
            Persist: CRED_PERSIST_LOCAL_MACHINE,
            ..Default::default()
        };
        // SAFETY: name and blob remain alive throughout the synchronous call.
        let success = unsafe { CredWriteW(&credential, 0) } != 0;
        blob.fill(0);
        if success { Ok(()) } else { Err(unavailable()) }
    }
}
#[cfg(not(windows))]
mod platform {
    use super::*;
    pub fn load(_: &str) -> Result<Option<String>> {
        Ok(None)
    }
    pub fn set(_: &str, _: Option<&str>) -> Result<()> {
        Err(unavailable())
    }
}
