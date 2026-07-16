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
        let _ = env; // see the TODO at the CreateProcessW call below: env overrides aren't wired yet on Windows
        let cwd = working_dir.map(|d| to_wide(d)).unwrap_or_else(|| to_wide(&workspace.to_string_lossy()));

        let mut process_info = PROCESS_INFORMATION::default();
        let create_result = unsafe {
            CreateProcessW(
                None,
                windows::core::PWSTR(cmdline.as_mut_ptr()),
                None,
                None,
                true,
                CREATE_SUSPENDED | EXTENDED_STARTUPINFO_PRESENT,
                None,
                // TODO(bucket-windows-env): passing a hand-built lpEnvironment block here
                // consistently fails CreateProcessW with ERROR_BAD_ENVIRONMENT (0xCB) when
                // combined with PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES; suspect AppContainer
                // process creation expects a block built via CreateEnvironmentBlock() for the
                // target identity (which auto-injects APPDATA/LOCALAPPDATA/USERPROFILE rebased to
                // the per-AppContainer isolated paths) rather than an arbitrary hand-rolled one.
                // Inheriting the parent's environment (None here) works reliably; step-level env
                // var overrides are not yet wired on the Windows backend as a result — tracked as
                // a follow-up, `env` is accepted but currently unused for the actual process.
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
             echo TRY_NETWORK & (ping -n 1 -w 1000 127.0.0.1 >nul 2>nul && echo NETWORK_SUCCEEDED || echo NETWORK_BLOCKED)";
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(20),
            exec_step(&handle, command, None, &[], |stream, line| {
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
