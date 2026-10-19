//! Ownership is established at spawn, never inferred from a process name.
use std::process::{Child, Command, ExitStatus};
pub struct Process {
    child: Child,
    #[cfg(windows)]
    job: windows_sys::Win32::Foundation::HANDLE,
}
// The handle is owned by Process and all access is serialized by the service.
#[cfg(windows)]
unsafe impl Send for Process {}
impl Process {
    pub fn spawn(command: &mut Command) -> Result<Self, String> {
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        let mut child = command.spawn().map_err(|e| e.to_string())?;
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            use windows_sys::Win32::{
                Foundation::CloseHandle,
                System::JobObjects::{
                    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
                    SetInformationJobObject,
                },
            };
            let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if job.is_null()
                || unsafe {
                    SetInformationJobObject(
                        job,
                        JobObjectExtendedLimitInformation,
                        &limits as *const _ as *const _,
                        std::mem::size_of_val(&limits) as u32,
                    )
                } == 0
                || unsafe { AssignProcessToJobObject(job, child.as_raw_handle()) } == 0
            {
                let error = std::io::Error::last_os_error();
                let _ = child.kill();
                let _ = child.wait();
                if !job.is_null() {
                    unsafe {
                        CloseHandle(job);
                    }
                }
                return Err(format!("Impossible de suivre le serveur : {error}"));
            }
            Ok(Self { child, job })
        }
        #[cfg(not(windows))]
        {
            let _ = &mut child;
            Ok(Self { child })
        }
    }
    pub fn try_wait(&mut self) -> std::io::Result<Option<ExitStatus>> {
        self.child.try_wait()
    }
    pub fn stop(&mut self) {
        #[cfg(windows)]
        unsafe {
            if windows_sys::Win32::System::JobObjects::TerminateJobObject(self.job, 0) == 0 {
                let _ = self.child.kill();
            }
        }
        #[cfg(unix)]
        if self.child.try_wait().ok().flatten().is_none() {
            // Only the private process group created by spawn is addressed.
            unsafe {
                libc::kill(-(self.child.id() as i32), libc::SIGTERM);
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
            unsafe {
                libc::kill(-(self.child.id() as i32), libc::SIGKILL);
            }
        }
        let _ = self.child.wait();
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        // Release only this service job; pre-existing servers never enter it.
        #[cfg(windows)]
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.job);
        }
    }
}
