//! Ownership is established at spawn, never inferred from a process name.
//! The server and every process it spawns die with the owner: Windows job
//! (`KILL_ON_JOB_CLOSE`, so also when Sherka crashes), Unix process group
//! (killed on exit; on Linux also when Sherka dies, through `PR_SET_PDEATHSIG`).
use std::process::{Child, Command, ExitStatus};
pub struct Process {
    child: Child,
    #[cfg(windows)]
    job: windows_sys::Win32::Foundation::HANDLE,
    // The leader died and what it left in the job was terminated.
    #[cfg(windows)]
    swept: bool,
    // The leader was reaped: its pid may be reused, so the group is not addressed.
    #[cfg(unix)]
    reaped: bool,
}
// The handle is owned by Process and all access is serialized by the service.
#[cfg(windows)]
unsafe impl Send for Process {}

// Windows: the process starts suspended and enters its job before running a
// single instruction, so nothing it spawns can be outside the job. It cannot
// leave it: breakaway is neither requested here nor allowed by the job.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
#[cfg(windows)]
const CREATE_SUSPENDED: u32 = 0x0000_0004;

#[cfg(windows)]
fn resume(pid: u32) -> bool {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, INVALID_HANDLE_VALUE},
        System::{
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First,
                Thread32Next,
            },
            Threading::{OpenThread, ResumeThread, THREAD_SUSPEND_RESUME},
        },
    };
    let mut resumed = false;
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return false;
        }
        let mut entry: THREADENTRY32 = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<THREADENTRY32>() as u32;
        let mut more = Thread32First(snapshot, &mut entry) != 0;
        while more {
            if entry.th32OwnerProcessID == pid {
                let thread = OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID);
                if !thread.is_null() {
                    resumed |= ResumeThread(thread) != u32::MAX;
                    CloseHandle(thread);
                }
            }
            more = Thread32Next(snapshot, &mut entry) != 0;
        }
        CloseHandle(snapshot);
    }
    resumed
}

// `PR_SET_PDEATHSIG` fires when the *thread* that forked the child exits, and
// the callers run on short-lived pool threads. Children are therefore forked
// by one thread that lives as long as Sherka.
#[cfg(target_os = "linux")]
fn spawn_anchored(command: &mut Command) -> std::io::Result<Child> {
    use std::os::unix::process::CommandExt;
    use std::sync::{OnceLock, mpsc};
    type Job = (Command, mpsc::Sender<std::io::Result<Child>>);
    static ANCHOR: OnceLock<mpsc::Sender<Job>> = OnceLock::new();
    let parent = unsafe { libc::getpid() };
    unsafe {
        command.pre_exec(move || {
            if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL as libc::c_ulong) == -1 {
                return Err(std::io::Error::last_os_error());
            }
            // Sherka may already be gone: the signal would never come.
            if libc::getppid() != parent {
                return Err(std::io::Error::other("parent exited"));
            }
            Ok(())
        });
    }
    let anchor = ANCHOR.get_or_init(|| {
        let (sender, jobs) = mpsc::channel::<Job>();
        std::thread::Builder::new()
            .name("owned-spawn".into())
            .spawn(move || {
                for (mut command, reply) in jobs {
                    let _ = reply.send(command.spawn());
                }
            })
            .expect("owned-spawn thread");
        sender
    });
    let (reply, result) = mpsc::channel();
    let command = std::mem::replace(command, Command::new("true"));
    anchor
        .send((command, reply))
        .map_err(|_| std::io::Error::other("spawner stopped"))?;
    result
        .recv()
        .map_err(|_| std::io::Error::other("spawner stopped"))?
}

#[cfg(unix)]
fn leader_gone(pid: u32) -> bool {
    // Peek without reaping: the pid (hence the group id) stays reserved.
    let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
    let result = unsafe {
        libc::waitid(
            libc::P_PID,
            pid as libc::id_t,
            &mut info,
            libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
        )
    };
    result == 0 && unsafe { info.si_pid() } != 0
}

impl Process {
    pub fn spawn(command: &mut Command) -> Result<Self, String> {
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
            #[cfg(target_os = "linux")]
            let child = spawn_anchored(command);
            #[cfg(not(target_os = "linux"))]
            let child = command.spawn();
            Ok(Self {
                child: child.map_err(|e| e.to_string())?,
                reaped: false,
            })
        }
        #[cfg(windows)]
        {
            use std::os::windows::{io::AsRawHandle, process::CommandExt};
            use windows_sys::Win32::{
                Foundation::CloseHandle,
                System::JobObjects::{
                    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
                    SetInformationJobObject,
                },
            };
            command.creation_flags(CREATE_NO_WINDOW | CREATE_SUSPENDED);
            let mut child = command.spawn().map_err(|e| e.to_string())?;
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
                || !resume(child.id())
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
            Ok(Self {
                child,
                job,
                swept: false,
            })
        }
    }
    pub fn try_wait(&mut self) -> std::io::Result<Option<ExitStatus>> {
        // A dead server leaves no worker behind holding its port.
        #[cfg(unix)]
        if !self.reaped && leader_gone(self.child.id()) {
            unsafe {
                libc::kill(-(self.child.id() as i32), libc::SIGKILL);
            }
        }
        let status = self.child.try_wait()?;
        #[cfg(unix)]
        {
            self.reaped = status.is_some();
        }
        #[cfg(windows)]
        if status.is_some() && !self.swept {
            self.swept = true;
            unsafe {
                windows_sys::Win32::System::JobObjects::TerminateJobObject(self.job, 0);
            }
        }
        Ok(status)
    }
    #[cfg(test)]
    pub fn kill_leader_only(&mut self) {
        let _ = self.child.kill();
    }
    /// Close the whole tree. False when the tree could not be proven closed.
    pub fn stop(&mut self) -> bool {
        #[cfg(windows)]
        {
            let terminated = unsafe {
                windows_sys::Win32::System::JobObjects::TerminateJobObject(self.job, 0) != 0
            } || self.child.kill().is_ok();
            terminated & self.child.wait().is_ok()
        }
        #[cfg(unix)]
        {
            if !self.reaped {
                // Only the private process group created by spawn is addressed.
                let group = -(self.child.id() as i32);
                unsafe {
                    libc::kill(group, libc::SIGTERM);
                }
                let grace = std::time::Instant::now() + std::time::Duration::from_secs(2);
                while std::time::Instant::now() < grace
                    && !self.try_wait().is_ok_and(|status| status.is_some())
                {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                if !self.reaped {
                    unsafe {
                        libc::kill(group, libc::SIGKILL);
                    }
                    self.reaped = self.child.wait().is_ok();
                }
            }
            self.reaped
        }
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        // Closing the job kills what is left in it (this service only: servers
        // that were already running never enter it); a group is killed likewise.
        #[cfg(windows)]
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.job);
        }
        #[cfg(unix)]
        if !self.reaped {
            unsafe {
                libc::kill(-(self.child.id() as i32), libc::SIGKILL);
            }
            let _ = self.child.wait();
        }
    }
}
