//! A verified new executable acts as a short-lived helper after the old app exits.
#[cfg(any(windows, test))]
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::Duration,
};
use tauri::Manager;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Plan {
    target: PathBuf,
    incoming: PathBuf,
    backup: PathBuf,
    parent: u32,
    signature: String,
    version: String,
}

pub struct Helper {
    child: Child,
    directory: PathBuf,
    incoming: PathBuf,
    committed: bool,
}

impl Helper {
    pub fn cancel(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
    pub fn commit(mut self) {
        self.committed = true;
    }
}

impl Drop for Helper {
    fn drop(&mut self) {
        if !self.committed {
            self.cancel();
            let _ = fs::remove_file(&self.incoming);
            let _ = fs::remove_dir_all(&self.directory);
        }
    }
}

pub async fn prepare(
    app: &tauri::AppHandle,
    bytes: &[u8],
    signature: &str,
    version: &str,
) -> Result<Helper, String> {
    let target = std::env::current_exe()
        .and_then(fs::canonicalize)
        .map_err(|e| e.to_string())?;
    let parent = target
        .parent()
        .ok_or("Dossier de l’application introuvable.")?;
    let id = uuid::Uuid::new_v4();
    let cache = std::env::var_os("SHERKA_DATA_DIR")
        .map(PathBuf::from)
        .map(Ok)
        .unwrap_or_else(|| app.path().app_cache_dir().map_err(|e| e.to_string()))?;
    let directory = cache.join("updates").join(id.to_string());
    fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let incoming = parent.join(format!(".sherka-{id}.new"));
    let backup = parent.join(format!(".sherka-{id}.previous.exe"));
    // Preparing in the same directory also verifies write access before stopping the app.
    if let Err(error) = fs::write(&incoming, bytes) {
        let _ = fs::remove_dir_all(&directory);
        return Err(format!(
            "Ce dossier n’est pas accessible en écriture. Déplace Sherka dans un dossier à toi ou télécharge la version depuis GitHub : {error}"
        ));
    }
    let result = (|| {
        let exe = directory.join("sherka-compagnon.exe");
        fs::write(&exe, bytes).map_err(|e| e.to_string())?;
        fs::write(
            directory.join("plan.json"),
            serde_json::to_vec(&Plan {
                target,
                incoming: incoming.clone(),
                backup,
                parent: std::process::id(),
                signature: signature.into(),
                version: version.into(),
            })
            .map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        let mut command = Command::new(exe);
        command
            .arg("--sherka-apply-update")
            .arg(directory.join("plan.json"))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let child = command.spawn().map_err(|e| e.to_string())?;
        Ok::<_, String>(Helper {
            child,
            directory: directory.clone(),
            incoming: incoming.clone(),
            committed: false,
        })
    })();
    let mut helper = match result {
        Ok(helper) => helper,
        Err(error) => {
            let _ = fs::remove_file(incoming);
            let _ = fs::remove_dir_all(directory);
            return Err(error);
        }
    };
    for _ in 0..100 {
        if helper.directory.join("ready").is_file() {
            return Ok(helper);
        }
        if helper
            .child
            .try_wait()
            .map_err(|e| e.to_string())?
            .is_some()
        {
            return Err(
                "Le remplacement n’a pas pu être préparé. L’application reste ouverte.".into(),
            );
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err("La préparation de la mise à jour a expiré. L’application reste ouverte.".into())
}

pub fn run_if_requested() -> bool {
    let mut args = std::env::args_os().skip(1);
    if args.next().as_deref() != Some(std::ffi::OsStr::new("--sherka-apply-update")) {
        return false;
    }
    if let Some(plan) = args.next()
        && args.next().is_none()
    {
        let path = PathBuf::from(plan);
        if let Err(error) = apply(&path) {
            let _ = fs::write(path.with_file_name("result.txt"), &error);
            #[cfg(windows)]
            unsafe {
                use windows_sys::Win32::UI::WindowsAndMessaging::{
                    MB_ICONERROR, MB_OK, MessageBoxW,
                };
                MessageBoxW(std::ptr::null_mut(), wide(format!("La mise à jour n’a pas abouti. La version précédente a été conservée.\n{error}")).as_ptr(), wide("Sherka · mise à jour").as_ptr(), MB_OK | MB_ICONERROR);
            }
        }
    }
    true
}

#[cfg(any(windows, test))]
fn verify(bytes: &[u8], signature: &str) -> Result<(), String> {
    let config: serde_json::Value =
        serde_json::from_str(include_str!("../tauri.conf.json")).map_err(|e| e.to_string())?;
    let decode = |value: &str| -> Result<String, String> {
        String::from_utf8(STANDARD.decode(value.trim()).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())
    };
    let key = minisign_verify::PublicKey::decode(&decode(
        config["plugins"]["updater"]["pubkey"]
            .as_str()
            .ok_or("Clé publique absente.")?,
    )?)
    .map_err(|e| e.to_string())?;
    let signature =
        minisign_verify::Signature::decode(&decode(signature)?).map_err(|e| e.to_string())?;
    key.verify(bytes, &signature, true)
        .map_err(|_| "Signature invalide. Aucun remplacement effectué.".into())
}

#[cfg(windows)]
fn wide(value: impl AsRef<std::ffi::OsStr>) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    value.as_ref().encode_wide().chain(Some(0)).collect()
}

#[cfg(windows)]
fn apply(path: &Path) -> Result<(), String> {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, WAIT_OBJECT_0},
        Storage::FileSystem::{REPLACEFILE_IGNORE_MERGE_ERRORS, ReplaceFileW},
        System::Threading::{
            OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
            WaitForSingleObject,
        },
    };
    let plan: Plan = serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let target = fs::canonicalize(&plan.target).map_err(|e| e.to_string())?;
    if plan.version != env!("CARGO_PKG_VERSION") {
        return Err("La version téléchargée ne correspond pas à la mise à jour annoncée.".into());
    }
    if target.file_name() != Some(std::ffi::OsStr::new("sherka-compagnon.exe"))
        || fs::canonicalize(plan.incoming.parent().ok_or("Dossier absent.")?)
            .map_err(|e| e.to_string())?
            != target.parent().unwrap()
        || plan.backup.parent() != plan.incoming.parent()
        || !plan
            .incoming
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .starts_with(".sherka-")
        || plan.backup.exists()
    {
        return Err("Plan de remplacement invalide.".into());
    }
    let bytes = fs::read(&plan.incoming).map_err(|e| e.to_string())?;
    verify(&bytes, &plan.signature)?;
    // Bind the target to the still-running parent process, preventing PID/path confusion.
    let handle = unsafe {
        OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | 0x00100000,
            0,
            plan.parent,
        )
    };
    if handle.is_null() {
        return Err("L’application d’origine est introuvable.".into());
    }
    let result = (|| {
        let mut buffer = vec![0u16; 32768];
        let mut length = buffer.len() as u32;
        if unsafe { QueryFullProcessImageNameW(handle, 0, buffer.as_mut_ptr(), &mut length) } == 0 {
            return Err("Le processus d’origine ne peut pas être vérifié.".into());
        }
        let actual = fs::canonicalize(String::from_utf16_lossy(&buffer[..length as usize]))
            .map_err(|e| e.to_string())?;
        if actual != target {
            return Err("Le processus d’origine ne correspond pas à l’exécutable.".into());
        }
        fs::write(path.with_file_name("ready"), b"ready").map_err(|e| e.to_string())?;
        if unsafe { WaitForSingleObject(handle, 180_000) } != WAIT_OBJECT_0 {
            return Err("La fermeture de Sherka a expiré. Aucun remplacement effectué.".into());
        }
        // Reverify immediately before using bytes; the helper does not trust the cache.
        verify(
            &fs::read(&plan.incoming).map_err(|e| e.to_string())?,
            &plan.signature,
        )?;
        let mut replaced = false;
        for _ in 0..100 {
            if unsafe {
                ReplaceFileW(
                    wide(&target).as_ptr(),
                    wide(&plan.incoming).as_ptr(),
                    wide(&plan.backup).as_ptr(),
                    REPLACEFILE_IGNORE_MERGE_ERRORS,
                    std::ptr::null(),
                    std::ptr::null(),
                )
            } != 0
            {
                replaced = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        if !replaced {
            let _ = Command::new(&target)
                .current_dir(target.parent().unwrap())
                .spawn();
            return Err(
                "Le fichier est encore verrouillé ou protégé. Réessaie depuis GitHub.".into(),
            );
        }
        // Keep notices current even when a portable update downloads only the executable.
        let folder = target.parent().unwrap();
        for (name, bytes) in [
            ("LICENSE", include_bytes!("../../LICENSE").as_slice()),
            (
                "THIRD_PARTY_NOTICES.md",
                include_bytes!("../../THIRD_PARTY_NOTICES.md").as_slice(),
            ),
        ] {
            let _ = fs::write(folder.join(name), bytes);
        }
        let _ = fs::create_dir_all(folder.join("licenses"));
        for (name, bytes) in [
            (
                "Silero.txt",
                include_bytes!("../../crates/sherka-core/models/LICENSE.silero").as_slice(),
            ),
            (
                "ONNXRuntime.txt",
                include_bytes!("../../crates/sherka-core/models/LICENSE.onnxruntime").as_slice(),
            ),
            (
                "ONNXRuntime-ThirdPartyNotices.txt",
                include_bytes!("../../crates/sherka-core/models/ONNXRUNTIME-ThirdPartyNotices.txt")
                    .as_slice(),
            ),
        ] {
            let _ = fs::write(folder.join("licenses").join(name), bytes);
        }
        let restart = Command::new(&target)
            .current_dir(target.parent().unwrap())
            .spawn();
        if let Err(error) = restart {
            unsafe {
                ReplaceFileW(
                    wide(&target).as_ptr(),
                    wide(&plan.backup).as_ptr(),
                    std::ptr::null(),
                    REPLACEFILE_IGNORE_MERGE_ERRORS,
                    std::ptr::null(),
                    std::ptr::null(),
                );
            }
            let _ = Command::new(&target)
                .current_dir(target.parent().unwrap())
                .spawn();
            return Err(format!("Le redémarrage a échoué : {error}"));
        }
        let _ = fs::write(
            path.with_file_name("result.json"),
            serde_json::to_vec(&serde_json::json!({"restarted_pid":restart.unwrap().id()}))
                .unwrap(),
        );
        let _ = fs::remove_file(&plan.backup);
        let _ = fs::remove_file(path);
        let _ = fs::remove_file(path.with_file_name("ready"));
        Ok(())
    })();
    unsafe {
        CloseHandle(handle);
    }
    result
}

#[cfg(not(windows))]
fn apply(_: &Path) -> Result<(), String> {
    Err("Cette mise à jour est destinée à Windows.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bad_signature_never_authorizes_a_replacement() {
        assert!(verify(b"not an executable", "invalid").is_err());
    }
}
