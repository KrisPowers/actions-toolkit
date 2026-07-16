//! Windows Bucket backend: AppContainer profiles for filesystem/network isolation, Job Objects
//! for resource limits and guaranteed teardown.
//!
//! AppContainer (not restricted tokens + low integrity level) is the primary isolation
//! primitive here: an AppContainer token has no access to any file or registry key that isn't
//! explicitly ACL'd to its SID, and it has no network capability at all unless a capability SID
//! (`internetClient` etc.) is present at process-creation time — both filesystem and network
//! default-deny come from one coherent, Microsoft-supported mechanism, rather than needing a
//! hand-rolled Windows Filtering Platform policy on top of a restricted token. Job Objects are
//! the orthogonal third leg: `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` guarantees every process
//! assigned to the job dies when the job is closed or the owning process (this one) crashes and
//! the kernel closes its handles, which is what makes teardown reliable without a separate
//! reaper process.
//!
//! Everything the backend needs (the AppContainer profile, the Job Object) is named
//! deterministically from the bucket's own id, so nothing OS-specific needs to round-trip
//! through `BucketHandle` or the database: `exec_step`/`remove_bucket`/crash reconciliation all
//! just re-derive or re-open by name.

use std::io::{BufRead, BufReader};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::FromRawHandle;
use std::path::Path;

use anyhow::{bail, Context, Result};
use sqlx::SqlitePool;
use uuid::Uuid;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, LocalFree, HANDLE, HLOCAL};
use windows::Win32::Security::Authorization::ConvertSidToStringSidW;
use windows::Win32::Security::Isolation::{CreateAppContainerProfile, DeleteAppContainerProfile};
use windows::Win32::Security::{FreeSid, PSECURITY_DESCRIPTOR};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation, OpenJobObjectW, SetInformationJobObject,
    TerminateJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};
use windows::Win32::System::Pipes::CreatePipe;

/// Not defined as a named constant in the `windows` crate; value from `winnt.h`
/// (`STANDARD_RIGHTS_REQUIRED | SYNCHRONIZE | 0x3F`), enough to assign processes, set limits,
/// and terminate the job when reopening it by name for teardown/crash reconciliation.
const JOB_OBJECT_ALL_ACCESS: u32 = 0x001F_003F;
use windows::Win32::System::Threading::{
    CreateProcessW, DeleteProcThreadAttributeList, GetExitCodeProcess, InitializeProcThreadAttributeList, ResumeThread,
    UpdateProcThreadAttribute, WaitForSingleObject, CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT, EXTENDED_STARTUPINFO_PRESENT,
    INFINITE, LPPROC_THREAD_ATTRIBUTE_LIST, PROCESS_INFORMATION, PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES, STARTUPINFOEXW,
    STARTUPINFOW,
};

use super::{BucketCapability, BucketHandle, BucketSpec, ExecResult};
use crate::db::queries::buckets as bucket_queries;

fn appcontainer_profile_name(id: &str) -> String {
    format!("atk-bucket-{id}")
}

fn job_object_name(id: &str) -> String {
    format!("Local\\atk-job-{id}")
}

fn to_wide(s: &str) -> Vec<u16> {
    std::ffi::OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

pub async fn probe_capability() -> BucketCapability {
    match tokio::task::spawn_blocking(probe_capability_blocking).await {
        Ok(result) => result,
        Err(e) => BucketCapability { ok: false, reason: Some(format!("probe task panicked: {e}")) },
    }
}

fn probe_capability_blocking() -> BucketCapability {
    let profile_name = format!("atk-probe-{}", Uuid::new_v4());
    match create_appcontainer(&profile_name, "actions-toolkit bucket capability probe") {
        Ok(sid) => {
            unsafe {
                let _ = FreeSid(sid);
            }
            let _ = delete_appcontainer(&profile_name);
        }
        Err(e) => return BucketCapability { ok: false, reason: Some(format!("{e:#}")) },
    }

    let job_name = format!("Local\\atk-probe-job-{}", Uuid::new_v4());
    match create_job_object(&job_name) {
        Ok(handle) => unsafe {
            let _ = CloseHandle(handle);
        },
        Err(e) => return BucketCapability { ok: false, reason: Some(format!("{e:#}")) },
    }

    BucketCapability { ok: true, reason: None }
}

pub async fn create_job_bucket(pool: &SqlitePool, buckets_root: &Path, spec: BucketSpec<'_>) -> Result<BucketHandle> {
    let id = Uuid::new_v4().to_string();
    let root_skeleton = buckets_root.join(&id);
    std::fs::create_dir_all(&root_skeleton).context("failed to create bucket scratch directory")?;

    let workspace = spec.workspace_host_path.to_path_buf();
    let profile_name = appcontainer_profile_name(&id);
    let workspace_for_setup = workspace.clone();
    let job_name = job_object_name(&id);

    tokio::task::spawn_blocking(move || -> Result<()> {
        let sid = create_appcontainer(&profile_name, "actions-toolkit workflow step sandbox")
            .context("failed to create AppContainer profile")?;
        let sid_string = sid_to_string(sid);
        unsafe {
            let _ = FreeSid(sid);
        }
        let sid_string = sid_string.context("failed to stringify AppContainer SID")?;

        grant_full_control(&workspace_for_setup, &sid_string).context("failed to grant AppContainer access to workspace")?;

        let job = create_job_object(&job_name).context("failed to create job object")?;
        unsafe {
            let _ = CloseHandle(job);
        }
        Ok(())
    })
    .await
    .context("bucket setup task panicked")??;

    let ttl_expires_at = (chrono::Utc::now() + chrono::Duration::seconds(spec.ttl.as_secs() as i64)).to_rfc3339();
    bucket_queries::create(
        pool,
        &id,
        spec.job_run_id,
        spec.run_id,
        &spec.workspace_host_path.to_string_lossy(),
        spec.network_enabled,
        &ttl_expires_at,
    )
    .await
    .context("failed to record bucket in database")?;

    Ok(BucketHandle { id, workspace, root_skeleton })
}

pub async fn exec_step<F>(
    handle: &BucketHandle,
    shell_command: &str,
    working_dir: Option<&str>,
    env: &[String],
    mut on_line: F,
) -> Result<ExecResult>
where
    F: FnMut(&str, String) + Send,
{
    let id = handle.id.clone();
    let workspace = handle.workspace.clone();
    let shell_command = shell_command.to_string();
    let working_dir = working_dir.map(str::to_string);
    let env = env.to_vec();

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<(&'static str, String)>();
    let stdout_tx = tx.clone();
    let stderr_tx = tx;

    let wait_task = tokio::task::spawn_blocking(move || {
        run_step_blocking(&id, &workspace, &shell_command, working_dir.as_deref(), &env, stdout_tx, stderr_tx)
    });

    while let Some((stream, line)) = rx.recv().await {
        on_line(stream, line);
    }

    wait_task.await.context("step execution task panicked")?
}

pub async fn remove_bucket(pool: &SqlitePool, handle: &BucketHandle) -> Result<()> {
    let _ = pool; // DB status transition is the caller's job (see bucket::reaper).
    let id = handle.id.clone();
    let root_skeleton = handle.root_skeleton.clone();
    tokio::task::spawn_blocking(move || -> Result<()> {
        let job_name = job_object_name(&id);
        if let Ok(job) = open_job_object(&job_name) {
            unsafe {
                let _ = TerminateJobObject(job, 1);
                let _ = CloseHandle(job);
            }
        }
        let _ = delete_appcontainer(&appcontainer_profile_name(&id));
        if root_skeleton.exists() {
            std::fs::remove_dir_all(&root_skeleton).context("failed to remove bucket scratch directory")?;
        }
        Ok(())
    })
    .await
    .context("bucket teardown task panicked")?
}

pub(crate) fn handle_from_bucket_row(buckets_root: &Path, row: &crate::db::models::Bucket) -> BucketHandle {
    BucketHandle {
        id: row.id.clone(),
        workspace: std::path::PathBuf::from(&row.workspace_path),
        root_skeleton: buckets_root.join(&row.id),
    }
}

/// Creates the AppContainer profile and returns the freshly-derived SID (caller must
/// `FreeSid` it). No capability SIDs are granted, i.e. no network access — matches the Linux
/// backend's current default-deny-only behavior (opt-in `network: true` support is a shared
/// follow-up on both backends, not yet wired on either).
fn create_appcontainer(profile_name: &str, description: &str) -> Result<windows::Win32::Security::PSID> {
    let name_wide = to_wide(profile_name);
    let display_wide = to_wide(profile_name);
    let desc_wide = to_wide(description);

    let sid = unsafe {
        CreateAppContainerProfile(
            PCWSTR(name_wide.as_ptr()),
            PCWSTR(display_wide.as_ptr()),
            PCWSTR(desc_wide.as_ptr()),
            None,
        )
    }
    .context("CreateAppContainerProfile failed")?;

    Ok(sid)
}

fn delete_appcontainer(profile_name: &str) -> Result<()> {
    let name_wide = to_wide(profile_name);
    unsafe { DeleteAppContainerProfile(PCWSTR(name_wide.as_ptr())) }.context("DeleteAppContainerProfile failed")
}

fn sid_to_string(sid: windows::Win32::Security::PSID) -> Result<String> {
    unsafe {
        let mut pwstr = windows::core::PWSTR::null();
        ConvertSidToStringSidW(sid, &mut pwstr).context("ConvertSidToStringSidW failed")?;
        let s = pwstr.to_string().context("SID string was not valid UTF-16")?;
        let _ = LocalFree(HLOCAL(pwstr.0 as *mut core::ffi::c_void));
        Ok(s)
    }
}

/// Grants the AppContainer SID full control over the workspace directory, recursively, via
/// `icacls` rather than hand-rolled `SetNamedSecurityInfoW`/ACL-builder calls — `icacls` is the
/// standard, well-tested tool for exactly this operation, which meaningfully lowers the risk of
/// a subtle ACL-construction bug in security-critical code compared to reimplementing it with
/// raw FFI.
fn grant_full_control(path: &Path, sid_string: &str) -> Result<()> {
    let grant_arg = format!("*{sid_string}:(OI)(CI)F");
    let output = std::process::Command::new("icacls")
        .arg(path)
        .arg("/grant")
        .arg(&grant_arg)
        .arg("/T")
        .output()
        .context("failed to invoke icacls")?;
    if !output.status.success() {
        bail!("icacls failed: {}", String::from_utf8_lossy(&output.stderr));
    }
    Ok(())
}

fn create_job_object(name: &str) -> Result<HANDLE> {
    let name_wide = to_wide(name);
    let job = unsafe { CreateJobObjectW(None, PCWSTR(name_wide.as_ptr())) }.context("CreateJobObjectW failed")?;

    let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    let result = unsafe {
        SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const core::ffi::c_void,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
    };
    if let Err(e) = result {
        unsafe {
            let _ = CloseHandle(job);
        }
        return Err(e).context("SetInformationJobObject failed");
    }

    Ok(job)
}

fn open_job_object(name: &str) -> Result<HANDLE> {
    let name_wide = to_wide(name);
    unsafe { OpenJobObjectW(JOB_OBJECT_ALL_ACCESS, false, PCWSTR(name_wide.as_ptr())) }.context("OpenJobObjectW failed")
}

fn run_step_blocking(
    id: &str,
    workspace: &Path,
    shell_command: &str,
    working_dir: Option<&str>,
    env: &[String],
    stdout_tx: tokio::sync::mpsc::UnboundedSender<(&'static str, String)>,
    stderr_tx: tokio::sync::mpsc::UnboundedSender<(&'static str, String)>,
) -> Result<ExecResult> {
    let profile_name = appcontainer_profile_name(id);
    let name_wide = to_wide(&profile_name);
    let sid = unsafe {
        windows::Win32::Security::Isolation::DeriveAppContainerSidFromAppContainerName(PCWSTR(name_wide.as_ptr()))
    }
    .context("failed to derive AppContainer SID; was the bucket created?")?;

    let result = (|| -> Result<ExecResult> {
        let (stdout_read, stdout_write) = create_inheritable_pipe()?;
        let (stderr_read, stderr_write) = create_inheritable_pipe()?;

        let mut startup_info = STARTUPINFOEXW {
            StartupInfo: STARTUPINFOW {
                cb: std::mem::size_of::<STARTUPINFOEXW>() as u32,
                dwFlags: windows::Win32::System::Threading::STARTF_USESTDHANDLES,
                hStdOutput: stdout_write,
                hStdError: stderr_write,
                ..Default::default()
            },
            lpAttributeList: LPPROC_THREAD_ATTRIBUTE_LIST::default(),
        };

        let mut attr_list_size: usize = 0;
        unsafe {
            // Expected to fail with ERROR_INSUFFICIENT_BUFFER; this first call only exists to
            // learn the required buffer size.
            let _ = InitializeProcThreadAttributeList(LPPROC_THREAD_ATTRIBUTE_LIST::default(), 1, 0, &mut attr_list_size);
        }
        let mut attr_list_buf = vec![0u8; attr_list_size];
        let attr_list = LPPROC_THREAD_ATTRIBUTE_LIST(attr_list_buf.as_mut_ptr() as *mut _);
        unsafe {
            InitializeProcThreadAttributeList(attr_list, 1, 0, &mut attr_list_size)
                .context("InitializeProcThreadAttributeList failed")?;
        }

        let mut capabilities = windows::Win32::Security::SECURITY_CAPABILITIES {
            AppContainerSid: sid,
            Capabilities: std::ptr::null_mut(),
            CapabilityCount: 0,
            Reserved: 0,
        };

        unsafe {
            UpdateProcThreadAttribute(
                attr_list,
                0,
                PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES as usize,
                Some(&mut capabilities as *mut _ as *const core::ffi::c_void),
                std::mem::size_of::<windows::Win32::Security::SECURITY_CAPABILITIES>(),
                None,
                None,
            )
            .context("UpdateProcThreadAttribute failed")?;
        }
        startup_info.lpAttributeList = attr_list;

        let mut cmdline = to_wide(&format!("cmd.exe /d /s /c \"{shell_command}\""));
        let env_block = build_environment_block(env);
        let cwd = working_dir.map(|d| to_wide(d)).unwrap_or_else(|| to_wide(&workspace.to_string_lossy()));

        let mut process_info = PROCESS_INFORMATION::default();
        let create_result = unsafe {
            CreateProcessW(
                None,
                windows::core::PWSTR(cmdline.as_mut_ptr()),
                None,
                None,
                true,
                CREATE_SUSPENDED | CREATE_UNICODE_ENVIRONMENT | EXTENDED_STARTUPINFO_PRESENT,
                Some(env_block.as_ptr() as *const core::ffi::c_void),
                PCWSTR(cwd.as_ptr()),
                &startup_info.StartupInfo,
                &mut process_info,
            )
        };

        unsafe {
            let _ = CloseHandle(stdout_write);
            let _ = CloseHandle(stderr_write);
            DeleteProcThreadAttributeList(attr_list);
        }

        create_result.context("CreateProcessW failed")?;

        let job_name = job_object_name(id);
        let job = open_job_object(&job_name).context("failed to open job object for this bucket")?;
        let assign_result = unsafe { AssignProcessToJobObject(job, process_info.hProcess) };
        if let Err(e) = assign_result {
            unsafe {
                let _ = windows::Win32::System::Threading::TerminateProcess(process_info.hProcess, 1);
                let _ = CloseHandle(job);
                let _ = CloseHandle(process_info.hProcess);
                let _ = CloseHandle(process_info.hThread);
            }
            return Err(e).context("AssignProcessToJobObject failed");
        }

        unsafe {
            ResumeThread(process_info.hThread);
        }

        let stdout_handle = process_info.hProcess;
        // HANDLE wraps a raw pointer, so it isn't `Send`; the numeric value itself is a plain OS
        // handle safe to hand to another thread, so cross the thread boundary as `isize` and
        // reconstruct the HANDLE on the other side.
        let stdout_reader_handle = stdout_read.0 as isize;
        let stderr_reader_handle = stderr_read.0 as isize;
        let stdout_reader =
            std::thread::spawn(move || read_pipe_lines(HANDLE(stdout_reader_handle as *mut _), "stdout", stdout_tx));
        let stderr_reader =
            std::thread::spawn(move || read_pipe_lines(HANDLE(stderr_reader_handle as *mut _), "stderr", stderr_tx));

        unsafe {
            WaitForSingleObject(stdout_handle, INFINITE);
        }
        let _ = stdout_reader.join();
        let _ = stderr_reader.join();

        let mut exit_code: u32 = 1;
        unsafe {
            let _ = GetExitCodeProcess(process_info.hProcess, &mut exit_code);
            let _ = CloseHandle(process_info.hProcess);
            let _ = CloseHandle(process_info.hThread);
            let _ = CloseHandle(job);
        }

        Ok(ExecResult { exit_code: exit_code as i64 })
    })();

    unsafe {
        let _ = FreeSid(sid);
    }
    result
}

fn create_inheritable_pipe() -> Result<(HANDLE, HANDLE)> {
    let mut read = HANDLE::default();
    let mut write = HANDLE::default();
    let attrs = windows::Win32::Security::SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<windows::Win32::Security::SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: PSECURITY_DESCRIPTOR::default().0,
        bInheritHandle: true.into(),
    };
    unsafe { CreatePipe(&mut read, &mut write, Some(&attrs), 0) }.context("CreatePipe failed")?;
    Ok((read, write))
}

fn read_pipe_lines(handle: HANDLE, stream: &'static str, tx: tokio::sync::mpsc::UnboundedSender<(&'static str, String)>) {
    let file = unsafe { std::fs::File::from_raw_handle(handle.0 as *mut core::ffi::c_void) };
    let reader = BufReader::new(file);
    for line in reader.lines() {
        match line {
            Ok(line) => {
                if tx.send((stream, line)).is_err() {
                    break;
                }
            }
            Err(_) => break,
        }
    }
}

/// Builds a double-null-terminated `KEY=VALUE\0...\0\0` UTF-16 block for `CreateProcessW`'s
/// `lpEnvironment`, since `CREATE_UNICODE_ENVIRONMENT` was requested.
fn build_environment_block(env: &[String]) -> Vec<u16> {
    let mut vars: Vec<String> = env.to_vec();
    if !vars.iter().any(|e| e.to_ascii_uppercase().starts_with("PATH=")) {
        if let Ok(path) = std::env::var("PATH") {
            vars.push(format!("PATH={path}"));
        }
    }
    let mut block = Vec::new();
    for var in vars {
        block.extend(std::ffi::OsStr::new(&var).encode_wide());
        block.push(0);
    }
    block.push(0);
    block
}
