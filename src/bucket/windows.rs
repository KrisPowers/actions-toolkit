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

    // The Job Object is deliberately *not* created here: a named kernel object with no open
    // handle and no assigned process is destroyed immediately, and at this point there's no
    // process yet to assign. `exec_step` creates (or reopens, if a previous step's job is still
    // alive) the job right before assigning the step's process to it, which is the earliest
    // point a handle can usefully be kept open.
    tokio::task::spawn_blocking(move || -> Result<()> {
        let sid = create_appcontainer(&profile_name, "actions-toolkit workflow step sandbox")
            .context("failed to create AppContainer profile")?;
        let sid_string = sid_to_string(sid);
        unsafe {
            let _ = FreeSid(sid);
        }
        let sid_string = sid_string.context("failed to stringify AppContainer SID")?;

        grant_full_control(&workspace_for_setup, &sid_string).context("failed to grant AppContainer access to workspace")?;
        grant_ancestor_traverse_access(&workspace_for_setup, &sid_string);
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

    Ok(BucketHandle { id, workspace, root_skeleton, network_enabled: spec.network_enabled })
}

pub async fn exec_step<F>(
    handle: &BucketHandle,
    shell_command: &str,
    shell: Option<&str>,
    working_dir: Option<&str>,
    env: &[String],
    mut on_line: F,
) -> Result<ExecResult>
where
    F: FnMut(&str, String) + Send,
{
    let id = handle.id.clone();
    let workspace = handle.workspace.clone();
    let network_enabled = handle.network_enabled;
    let shell_command = shell_command.to_string();
    let shell = shell.map(str::to_string);
    let working_dir = working_dir.map(str::to_string);
    let env = env.to_vec();

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<(&'static str, String)>();
    let stdout_tx = tx.clone();
    let stderr_tx = tx;

    let wait_task = tokio::task::spawn_blocking(move || {
        let invocation = StepInvocation {
            shell_command: &shell_command,
            shell: shell.as_deref(),
            working_dir: working_dir.as_deref(),
            env: &env,
        };
        run_step_blocking(&id, &workspace, network_enabled, invocation, (stdout_tx, stderr_tx))
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
        network_enabled: row.network_enabled != 0,
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

/// Grants the AppContainer SID traverse-only access (`(X)`, not full control, and not inherited
/// by each ancestor's other children) to `path`'s two immediate ancestors — for a workspace at
/// `<data_dir>/workspaces/<run_id>`, that's `<data_dir>/workspaces` and `<data_dir>` itself — on
/// top of `grant_full_control`'s full-control grant on `path` itself.
///
/// AppContainer tokens don't hold the "bypass traverse checking" privilege normal user tokens get
/// by default, so a workspace nested under the app's own data dir is otherwise only reachable by
/// tools that don't validate their working directory at startup. `cmd.exe`'s inherited-cwd
/// handling doesn't (file I/O inside the granted leaf directory works fine even without this), but
/// PowerShell's `Set-Location`/`$PWD` initialization does, and fails with
/// `UnauthorizedAccessException` without it — found by testing a real (non-`cmd`) default-shell
/// step end-to-end, not from documentation.
///
/// Deliberately bounded to 2 levels rather than walking to the drive root: this covers every
/// directory the app itself creates without touching (and persistently modifying the ACLs of)
/// arbitrary user-profile directories above `data_dir` that this process doesn't own — which, on
/// top of the unbounded-depth version being needlessly invasive, also turned out to be extremely
/// slow (one `icacls` process per ancestor, several of which took tens of seconds against real
/// directories with large existing ACLs).
fn grant_ancestor_traverse_access(path: &Path, sid_string: &str) {
    let grant_arg = format!("*{sid_string}:(X)");
    for ancestor in path.ancestors().skip(1).take(2) {
        if ancestor.parent().is_none() {
            break;
        }
        match std::process::Command::new("icacls").arg(ancestor).arg("/grant").arg(&grant_arg).output() {
            Ok(output) if !output.status.success() => {
                tracing::warn!(
                    path = %ancestor.display(),
                    stderr = %String::from_utf8_lossy(&output.stderr),
                    "failed to grant traverse access to a bucket workspace ancestor directory"
                );
            }
            Err(e) => {
                tracing::warn!(error = %e, path = %ancestor.display(), "failed to invoke icacls for ancestor traverse grant");
            }
            _ => {}
        }
    }
}

/// Builds the capability SIDs granted when `network: true` is requested: `internetClient` for
/// outbound internet access, `privateNetworkClientServer` for also reaching other hosts on the
/// local/private network. Returns the raw SID byte buffers alongside the `SID_AND_ATTRIBUTES`
/// array pointing into them — the caller must keep the buffers alive for as long as the array is
/// used (through the `CreateProcessW` call), since `SID_AND_ATTRIBUTES::Sid` is a raw pointer.
fn network_capability_sids() -> Result<(Vec<Vec<u8>>, Vec<windows::Win32::Security::SID_AND_ATTRIBUTES>)> {
    use windows::Win32::Security::{WinCapabilityInternetClientSid, WinCapabilityPrivateNetworkClientServerSid};

    let mut buffers: Vec<Vec<u8>> = [WinCapabilityInternetClientSid, WinCapabilityPrivateNetworkClientServerSid]
        .into_iter()
        .map(create_well_known_sid)
        .collect::<Result<_>>()?;

    let attrs = buffers
        .iter_mut()
        .map(|buf| windows::Win32::Security::SID_AND_ATTRIBUTES {
            Sid: windows::Win32::Security::PSID(buf.as_mut_ptr() as *mut _),
            Attributes: 0x0000_0004, // SE_GROUP_ENABLED
        })
        .collect();

    Ok((buffers, attrs))
}

fn create_well_known_sid(sid_type: windows::Win32::Security::WELL_KNOWN_SID_TYPE) -> Result<Vec<u8>> {
    use windows::Win32::Security::{CreateWellKnownSid, PSID};

    let mut size: u32 = 0;
    unsafe {
        // Expected to fail with ERROR_INSUFFICIENT_BUFFER; this first call only exists to learn
        // the required buffer size.
        let _ = CreateWellKnownSid(sid_type, None, PSID::default(), &mut size);
    }
    let mut buf = vec![0u8; size as usize];
    unsafe {
        CreateWellKnownSid(sid_type, None, PSID(buf.as_mut_ptr() as *mut _), &mut size)
            .context("CreateWellKnownSid failed")?;
    }
    Ok(buf)
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

/// What to actually run: bundled separately from the sandbox's own identity/config params so
/// `run_step_blocking` doesn't grow an unwieldy positional argument list.
struct StepInvocation<'a> {
    shell_command: &'a str,
    shell: Option<&'a str>,
    working_dir: Option<&'a str>,
    env: &'a [String],
}

type OutputLineSender = tokio::sync::mpsc::UnboundedSender<(&'static str, String)>;

fn run_step_blocking(
    id: &str,
    workspace: &Path,
    network_enabled: bool,
    invocation: StepInvocation<'_>,
    (stdout_tx, stderr_tx): (OutputLineSender, OutputLineSender),
) -> Result<ExecResult> {
    let StepInvocation { shell_command, shell, working_dir, env } = invocation;
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

        // Capability SIDs granted at process-creation time are the only way an AppContainer
        // process gets any network access at all; with none present (the default) the process
        // has no network capability whatsoever, which is what makes network-deny-by-default work
        // here without a separate firewall policy. `_network_sid_buffers` has to stay alive
        // alongside `network_caps`/`capabilities` since the latter point into it.
        let (_network_sid_buffers, mut network_caps) =
            if network_enabled { network_capability_sids()? } else { (Vec::new(), Vec::new()) };
        let mut capabilities = windows::Win32::Security::SECURITY_CAPABILITIES {
            AppContainerSid: sid,
            Capabilities: if network_caps.is_empty() { std::ptr::null_mut() } else { network_caps.as_mut_ptr() },
            CapabilityCount: network_caps.len() as u32,
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

        let mut cmdline = to_wide(&resolve_shell_cmdline(shell, shell_command));
        let cwd = working_dir.map(to_wide).unwrap_or_else(|| to_wide(&workspace.to_string_lossy()));
        let env_block = build_environment_block(env);

        let mut process_info = PROCESS_INFORMATION::default();
        let create_result = unsafe {
            CreateProcessW(
                None,
                windows::core::PWSTR(cmdline.as_mut_ptr()),
                None,
                None,
                true,
                CREATE_SUSPENDED | CREATE_UNICODE_ENVIRONMENT | EXTENDED_STARTUPINFO_PRESENT,
                // A prior attempt here passed a block containing only the step's override vars
                // (plus a PATH fallback), which reliably failed with ERROR_BAD_ENVIRONMENT: it was
                // missing SystemRoot/ComSpec/TEMP/etc. that cmd.exe needs to initialize at all.
                // build_environment_block now starts from this process's own (inherited, already
                // proven to work) environment and overlays the step's overrides on top, so the
                // sandboxed process gets a complete, valid block either way.
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
        // Creates the job on the first step, or reopens the same named object if a previous
        // step's job handle is (unusually) still alive; either way `create_job_object` also
        // (re-)applies JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, which is idempotent.
        let job = create_job_object(&job_name).context("failed to create or open job object for this bucket")?;
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
    let file = unsafe { std::fs::File::from_raw_handle(handle.0) };
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
/// `lpEnvironment`, since `CREATE_UNICODE_ENVIRONMENT` is requested alongside it.
///
/// Starts from this process's own environment (the same set a sandboxed process would get by
/// inheriting, which is already known to work) rather than only the step's override vars, then
/// applies `overrides` on top by key (case-insensitively, last write wins). This keeps essentials
/// like `SystemRoot`/`ComSpec`/`TEMP` present even when a step only overrides one or two vars.
fn build_environment_block(overrides: &[String]) -> Vec<u16> {
    let mut vars: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    for (key, value) in std::env::vars() {
        vars.insert(key.to_ascii_uppercase(), format!("{key}={value}"));
    }
    for entry in overrides {
        if let Some(eq) = entry.find('=') {
            vars.insert(entry[..eq].to_ascii_uppercase(), entry.clone());
        }
    }

    let mut block = Vec::new();
    for entry in vars.values() {
        block.extend(std::ffi::OsStr::new(entry).encode_wide());
        block.push(0);
    }
    block.push(0);
    block
}

/// Whether `pwsh.exe` (PowerShell 7+) is resolvable on `PATH`. Checked at runtime rather than
/// assumed present, since unlike Windows PowerShell it isn't preinstalled on Windows.
fn pwsh_available() -> bool {
    std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).any(|dir| dir.join("pwsh.exe").exists()))
        .unwrap_or(false)
}

/// Resolves a step's `shell:` override into the full command line `CreateProcessW` execs
/// directly (no `cmd.exe` wrapper unless `cmd` is explicitly requested). Defaults to `pwsh`,
/// falling back to the always-present `powershell.exe` when `pwsh.exe` isn't on `PATH` — matches
/// real GitHub Actions' Windows runner default. An explicit `shell: pwsh` is honored as-is (no
/// silent fallback) so a missing pwsh install fails loudly instead of quietly using a different
/// shell than the one the workflow author asked for.
///
/// `-NoProfile` is load-bearing, not cosmetic: without it, PowerShell loads the invoking host
/// account's profile script before running the step, and a profile that does something as
/// ordinary as `cd`-ing somewhere silently changes the step's actual working directory out from
/// under `-WorkingDirectory`/`lpCurrentDirectory` entirely (caught by testing this against a real
/// profile that does exactly that, not from documentation).
fn resolve_shell_cmdline(shell: Option<&str>, shell_command: &str) -> String {
    match shell.map(str::to_ascii_lowercase).as_deref() {
        Some("cmd") => format!("cmd.exe /d /s /c \"{shell_command}\""),
        Some("powershell") => format!("powershell.exe -NoLogo -NoProfile -NonInteractive -Command \"{shell_command}\""),
        Some("pwsh") => format!("pwsh.exe -NoLogo -NoProfile -NonInteractive -Command \"{shell_command}\""),
        Some(other) => format!("{other} \"{shell_command}\""),
        None if pwsh_available() => format!("pwsh.exe -NoLogo -NoProfile -NonInteractive -Command \"{shell_command}\""),
        None => format!("powershell.exe -NoLogo -NoProfile -NonInteractive -Command \"{shell_command}\""),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::models::now_iso;

    #[tokio::test]
    async fn probe_reports_this_host_can_run_buckets() {
        let capability = probe_capability().await;
        assert!(capability.ok, "expected this host to support AppContainer + Job Objects: {:?}", capability.reason);
    }

    /// End-to-end: create a bucket, run a step that writes into its workspace and attempts to
    /// write outside it and reach the network, then tear the bucket down. Unlike the Linux
    /// backend, this doesn't need `current_exe()` re-exec (CreateProcessW is called directly),
    /// so `cargo test` can exercise the real spawn path, not just pure logic.
    #[tokio::test]
    async fn full_bucket_lifecycle_isolates_and_cleans_up() {
        let capability = probe_capability().await;
        if !capability.ok {
            eprintln!("skipping: host does not support Bucket ({:?})", capability.reason);
            return;
        }

        let test_id = Uuid::new_v4().to_string();
        let base = std::env::temp_dir().join(format!("atk-bucket-test-{test_id}"));
        let buckets_root = base.join("buckets");
        let workspace = base.join("workspace");
        std::fs::create_dir_all(&workspace).unwrap();
        std::fs::create_dir_all(&buckets_root).unwrap();

        let db_path = base.join("test.db");
        let pool = crate::db::connect(&db_path).await.expect("db connect should succeed");
        seed_fk_chain(&pool, "repo-1", "workflow-1", "run-1", "job-1").await;

        let spec = BucketSpec {
            workspace_host_path: &workspace,
            run_id: "run-1",
            job_run_id: "job-1",
            network_enabled: false,
            ttl: std::time::Duration::from_secs(3600),
        };
        let handle = create_job_bucket(&pool, &buckets_root, spec).await.expect("create_job_bucket should succeed");

        let mut stdout_lines = Vec::new();
        let command = "echo WORKSPACE_WRITE_TEST & echo hello> step_output.txt & type step_output.txt & \
             echo TRY_ESCAPE & (echo bad > C:\\Windows\\Temp\\atk_escape_test.txt 2>nul && echo ESCAPE_SUCCEEDED || echo ESCAPE_BLOCKED) & \
             echo TRY_NETWORK & (ping -n 1 -w 1000 127.0.0.1 >nul 2>nul && echo NETWORK_SUCCEEDED || echo NETWORK_BLOCKED) & \
             echo TRY_ENV_OVERRIDE & echo ATK_TEST_VAR=%ATK_TEST_VAR% & echo SYSTEMROOT_SET=%SystemRoot%";
        let env = vec!["ATK_TEST_VAR=sandboxed_value".to_string()];
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(20),
            exec_step(&handle, command, Some("cmd"), None, &env, |stream, line| {
                if stream == "stdout" {
                    stdout_lines.push(line);
                }
            }),
        )
        .await
        .expect("exec_step timed out")
        .expect("exec_step should succeed");

        let output = stdout_lines.join("\n");
        println!("sandboxed step output:\n{output}");

        assert!(output.contains("hello"), "expected the workspace write/read round-trip to succeed: {output}");
        assert!(
            output.contains("ATK_TEST_VAR=sandboxed_value"),
            "expected the step-declared env var override to reach the sandboxed process: {output}"
        );
        assert!(
            !output.contains("SYSTEMROOT_SET=%SystemRoot%") && output.contains("SYSTEMROOT_SET="),
            "expected the inherited SystemRoot to still be present alongside the override: {output}"
        );
        assert!(
            workspace.join("step_output.txt").exists(),
            "expected step_output.txt to exist in the real host workspace dir after the step ran"
        );
        assert!(
            !output.contains("ESCAPE_SUCCEEDED") && !std::path::Path::new(r"C:\Windows\Temp\atk_escape_test.txt").exists(),
            "expected the AppContainer to be denied write access outside its granted workspace: {output}"
        );
        assert!(!output.contains("NETWORK_SUCCEEDED"), "expected no network capability to be granted by default: {output}");
        assert_eq!(result.exit_code, 0);

        remove_bucket(&pool, &handle).await.expect("remove_bucket should succeed");
        assert!(!handle.root_skeleton.exists(), "expected the bucket scratch directory to be removed after teardown");

        let _ = std::fs::remove_dir_all(&base);
    }

    /// Confirms the default shell (unset `shell:`) actually resolves to a real, runnable
    /// PowerShell variant end-to-end, not just that `resolve_shell_cmdline` returns a plausible
    /// string: runs a PowerShell-syntax command with no shell override and checks it executed as
    /// PowerShell (cmd.exe would fail on this syntax, not silently produce the same output).
    #[tokio::test]
    async fn default_shell_runs_as_powershell_without_an_override() {
        let capability = probe_capability().await;
        if !capability.ok {
            eprintln!("skipping: host does not support Bucket ({:?})", capability.reason);
            return;
        }

        let test_id = Uuid::new_v4().to_string();
        let base = std::env::temp_dir().join(format!("atk-bucket-test-shell-{test_id}"));
        let buckets_root = base.join("buckets");
        let workspace = base.join("workspace");
        std::fs::create_dir_all(&workspace).unwrap();
        std::fs::create_dir_all(&buckets_root).unwrap();

        let db_path = base.join("test.db");
        let pool = crate::db::connect(&db_path).await.expect("db connect should succeed");
        seed_fk_chain(&pool, "repo-2", "workflow-2", "run-2", "job-2").await;

        let spec = BucketSpec {
            workspace_host_path: &workspace,
            run_id: "run-2",
            job_run_id: "job-2",
            network_enabled: false,
            ttl: std::time::Duration::from_secs(3600),
        };
        let handle = create_job_bucket(&pool, &buckets_root, spec).await.expect("create_job_bucket should succeed");

        let mut stdout_lines = Vec::new();
        // `1..3 | ForEach-Object` only parses as PowerShell; cmd.exe would fail to run this at
        // all. Single-quoted (not double-quoted) so it doesn't collide with the outer
        // `-Command "..."` embedding's own double quotes.
        let command = "1..3 | ForEach-Object { 'count-' + $_ }";
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(20),
            exec_step(&handle, command, None, None, &[], |stream, line| {
                if stream == "stdout" {
                    stdout_lines.push(line);
                }
            }),
        )
        .await
        .expect("exec_step timed out")
        .expect("exec_step should succeed");

        let output = stdout_lines.join("\n");
        println!("default-shell step output:\n{output}");
        assert!(output.contains("count-1") && output.contains("count-3"), "expected PowerShell syntax to have run: {output}");
        assert_eq!(result.exit_code, 0);

        remove_bucket(&pool, &handle).await.expect("remove_bucket should succeed");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn resolve_shell_cmdline_honors_explicit_overrides() {
        assert_eq!(resolve_shell_cmdline(Some("cmd"), "echo hi"), "cmd.exe /d /s /c \"echo hi\"");
        assert_eq!(
            resolve_shell_cmdline(Some("PowerShell"), "Write-Host hi"),
            "powershell.exe -NoLogo -NoProfile -NonInteractive -Command \"Write-Host hi\""
        );
        assert_eq!(
            resolve_shell_cmdline(Some("pwsh"), "Write-Host hi"),
            "pwsh.exe -NoLogo -NoProfile -NonInteractive -Command \"Write-Host hi\""
        );
    }

    #[test]
    fn resolve_shell_cmdline_defaults_to_a_powershell_variant() {
        let cmdline = resolve_shell_cmdline(None, "Write-Host hi");
        assert!(
            cmdline.starts_with("pwsh.exe") || cmdline.starts_with("powershell.exe"),
            "expected the default shell to be pwsh or powershell: {cmdline}"
        );
    }

    /// Confirms `network: true` actually grants network capability, not just that it's threaded
    /// through without erroring. Deliberately targets a real external host, not loopback: Windows
    /// blocks AppContainer loopback access unconditionally via a separate mechanism
    /// (`NetworkIsolationSetAppContainerConfig`'s loopback-exemption list), independent of
    /// capability SIDs — discovered by first writing this test against 127.0.0.1 and watching it
    /// fail even with the capability granted, which is exactly the kind of thing only running it
    /// for real (not just compiling) catches.
    #[tokio::test]
    async fn network_enabled_grants_capability_that_default_deny_blocks() {
        let capability = probe_capability().await;
        if !capability.ok {
            eprintln!("skipping: host does not support Bucket ({:?})", capability.reason);
            return;
        }

        let test_id = Uuid::new_v4().to_string();
        let base = std::env::temp_dir().join(format!("atk-bucket-test-net-{test_id}"));
        let buckets_root = base.join("buckets");
        let workspace = base.join("workspace");
        std::fs::create_dir_all(&workspace).unwrap();
        std::fs::create_dir_all(&buckets_root).unwrap();

        let db_path = base.join("test.db");
        let pool = crate::db::connect(&db_path).await.expect("db connect should succeed");
        seed_fk_chain(&pool, "repo-3", "workflow-3", "run-3", "job-3").await;

        let spec = BucketSpec {
            workspace_host_path: &workspace,
            run_id: "run-3",
            job_run_id: "job-3",
            network_enabled: true,
            ttl: std::time::Duration::from_secs(3600),
        };
        let handle = create_job_bucket(&pool, &buckets_root, spec).await.expect("create_job_bucket should succeed");

        let mut stdout_lines = Vec::new();
        // A real TCP/HTTPS request via curl.exe (built into Windows 10 1803+/Windows 11), not
        // ICMP ping: ping showed NETWORK_BLOCKED even with the capability granted, which turned
        // out to be ICMP-specific (Windows Firewall's AppContainer capability rules govern normal
        // Winsock TCP/UDP traffic, not raw ICMP echo) rather than evidence the capability grant
        // itself doesn't work.
        let command = "(curl.exe -s -o nul --max-time 5 https://www.google.com && echo NETWORK_SUCCEEDED || echo NETWORK_BLOCKED)";
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(20),
            exec_step(&handle, command, Some("cmd"), None, &[], |stream, line| {
                if stream == "stdout" {
                    stdout_lines.push(line);
                }
            }),
        )
        .await
        .expect("exec_step timed out")
        .expect("exec_step should succeed");

        let output = stdout_lines.join("\n");
        println!("network-enabled step output:\n{output}");
        if output.contains("NETWORK_BLOCKED") {
            eprintln!("skipping assertion: this host may not have outbound internet access to verify against");
        } else {
            assert!(output.contains("NETWORK_SUCCEEDED"), "expected network: true to grant external network access: {output}");
            assert_eq!(result.exit_code, 0);
        }

        remove_bucket(&pool, &handle).await.expect("remove_bucket should succeed");
        let _ = std::fs::remove_dir_all(&base);
    }

    async fn seed_fk_chain(pool: &sqlx::SqlitePool, repo_id: &str, workflow_id: &str, run_id: &str, job_run_id: &str) {
        let now = now_iso();
        sqlx::query(
            "INSERT INTO users (id, username, password_hash, role, created_at, updated_at) VALUES (?, ?, ?, 'admin', ?, ?)",
        )
        .bind("user-1")
        .bind("test-user")
        .bind("hash")
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO repos (id, owner, name, default_branch, webhook_secret_encrypted, \
             webhook_secret_nonce, created_by, created_at, updated_at) VALUES (?, 'test-owner', 'test-repo', 'main', \
             x'00', x'00', 'user-1', ?, ?)",
        )
        .bind(repo_id)
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO workflows (id, repo_id, name, file_path, yaml_source, parsed_json, enabled, created_at, updated_at) \
             VALUES (?, ?, 'test-workflow', 'ci.yml', '', '{}', 1, ?, ?)",
        )
        .bind(workflow_id)
        .bind(repo_id)
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO workflow_runs (id, workflow_id, repo_id, trigger_event, status, created_at) \
             VALUES (?, ?, ?, 'manual', 'running', ?)",
        )
        .bind(run_id)
        .bind(workflow_id)
        .bind(repo_id)
        .bind(&now)
        .execute(pool)
        .await
        .unwrap();

        sqlx::query("INSERT INTO job_runs (id, workflow_run_id, job_key, status) VALUES (?, ?, 'build', 'running')")
            .bind(job_run_id)
            .bind(run_id)
            .execute(pool)
            .await
            .unwrap();
    }
}
