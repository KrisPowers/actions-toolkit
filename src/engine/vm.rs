//! Virtual machines. Each hypervisor is reached through a "transport" that can run a command in
//! the guest with stdin attached: native CLIs where they exist (Lima, Multipass, WSL, Parallels)
//! and SSH for everything else (VirtualBox, VMware, QEMU, Hyper-V, Tart).
//!
//! Commands passed through a transport are always plain words (paths under a slug-only
//! directory), because several transports re-join argv with spaces. Scripts, env vars and the
//! working directory travel inside a generated wrapper script uploaded via stdin.

use std::collections::HashSet;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{bail, Context, Result};

use super::{
    apply_probe, find_binary, probe_unix, version_of, Availability, Engine, EngineInfo,
    ExecRequest, Kind, Session, SessionInfo, SessionRequest,
};
use crate::config::{Config, EngineConfig};
use crate::util::{self, shell_quote, Cancel, ExecOutcome};

fn locate(kind: Kind) -> Option<PathBuf> {
    match kind {
        Kind::Lima => find_binary(
            &["limactl"],
            &["/opt/homebrew/bin/limactl", "/usr/local/bin/limactl"],
        ),
        Kind::Multipass => find_binary(
            &["multipass"],
            &[
                "/usr/local/bin/multipass",
                "/snap/bin/multipass",
                r"C:\Program Files\Multipass\bin\multipass.exe",
            ],
        ),
        Kind::Wsl => find_binary(&["wsl"], &[r"C:\Windows\System32\wsl.exe"]),
        Kind::Parallels => find_binary(&["prlctl"], &["/usr/local/bin/prlctl"]),
        Kind::Tart => find_binary(&["tart"], &["/opt/homebrew/bin/tart"]),
        Kind::VirtualBox => find_binary(
            &["VBoxManage", "vboxmanage"],
            &[
                "/Applications/VirtualBox.app/Contents/MacOS/VBoxManage",
                r"C:\Program Files\Oracle\VirtualBox\VBoxManage.exe",
            ],
        ),
        Kind::VMware => find_binary(
            &["vmrun"],
            &[
                "/Applications/VMware Fusion.app/Contents/Library/vmrun",
                r"C:\Program Files (x86)\VMware\VMware Workstation\vmrun.exe",
                r"C:\Program Files\VMware\VMware Workstation\vmrun.exe",
            ],
        ),
        Kind::Qemu => find_binary(
            &["qemu-system-x86_64", "qemu-system-aarch64"],
            &[r"C:\Program Files\qemu\qemu-system-x86_64.exe"],
        ),
        Kind::HyperV => {
            let out = util::probe(
                "powershell",
                &[
                    "-NoProfile",
                    "-NonInteractive",
                    "-Command",
                    "if (Get-Command Get-VM -ErrorAction SilentlyContinue) { 'yes' }",
                ],
            )?;
            out.stdout
                .contains("yes")
                .then(|| PathBuf::from("powershell"))
        }
        _ => None,
    }
}

fn version(kind: Kind, bin: &Path) -> Option<String> {
    match kind {
        Kind::Lima | Kind::Parallels | Kind::Tart | Kind::VirtualBox | Kind::Qemu => {
            version_of(bin, &["--version"])
        }
        Kind::Multipass => version_of(bin, &["version"]),
        // vmrun has no version flag; wsl/Hyper-V print UTF-16 or need elevation.
        _ => None,
    }
}

fn default_instance(kind: Kind) -> Option<&'static str> {
    match kind {
        Kind::Lima => Some("default"),
        Kind::Multipass => Some("primary"),
        _ => None,
    }
}

fn setup_hint(kind: Kind) -> String {
    match kind {
        Kind::Parallels => "pick a Linux VM: maverick config set-target parallels <vm-name>".into(),
        _ => format!(
            "enable SSH in a Linux guest, then: maverick config set-target {} user@host[:port]",
            kind.id()
        ),
    }
}

pub(super) fn detect(kind: Kind, cfg: &Config) -> EngineInfo {
    let Some(bin) = locate(kind) else {
        return EngineInfo::missing(kind);
    };
    let version = version(kind, &bin);
    let ec = cfg.engine(kind.id());
    if let Some(target) = &ec.ssh {
        if find_binary(&["ssh"], &[]).is_none() {
            return EngineInfo::new(
                kind,
                Availability::NeedsSetup,
                version,
                "install an OpenSSH client".to_string(),
            );
        }
        return EngineInfo::new(
            kind,
            Availability::Ready,
            version,
            format!("via ssh {target}"),
        );
    }
    let instance = ec
        .instance
        .clone()
        .or(default_instance(kind).map(String::from));
    match (kind, instance) {
        (Kind::Lima, Some(inst)) => {
            let listing = util::probe(&bin, &["list", "--format", "{{.Name}} {{.Status}}"]);
            let state = listing.and_then(|o| {
                o.stdout.lines().find_map(|l| {
                    l.split_once(' ')
                        .filter(|(n, _)| *n == inst)
                        .map(|(_, s)| s.trim().to_string())
                })
            });
            instance_state(
                kind,
                version,
                &inst,
                state,
                &format!("limactl start {inst}"),
            )
        }
        (Kind::Multipass, Some(inst)) => {
            let listing = util::probe(&bin, &["list", "--format", "csv"]);
            let state = listing.and_then(|o| {
                o.stdout.lines().skip(1).find_map(|l| {
                    let mut cols = l.split(',');
                    (cols.next() == Some(inst.as_str()))
                        .then(|| cols.next().unwrap_or_default().to_string())
                })
            });
            instance_state(
                kind,
                version,
                &inst,
                state,
                &format!("multipass launch --name {inst}"),
            )
        }
        (Kind::Wsl, inst) => {
            let ok = util::probe(&bin, &["--status"]).is_some_and(|o| o.success);
            let target = inst
                .map(|i| format!("distro {i}"))
                .unwrap_or_else(|| "default distro".into());
            if ok {
                EngineInfo::new(kind, Availability::Ready, version, target)
            } else {
                EngineInfo::new(
                    kind,
                    Availability::NeedsSetup,
                    version,
                    "install a distro: wsl --install".to_string(),
                )
            }
        }
        (Kind::Parallels, Some(vm)) => {
            let listing = util::probe(
                &bin,
                &["list", "--all", "--output", "name,status", "--no-header"],
            );
            let state = listing.and_then(|o| {
                o.stdout.lines().find_map(|l| {
                    let l = l.trim();
                    l.strip_prefix(vm.as_str())
                        .map(|rest| rest.trim().to_string())
                })
            });
            instance_state(kind, version, &vm, state, &format!("prlctl start \"{vm}\""))
        }
        _ => EngineInfo::new(kind, Availability::NeedsSetup, version, setup_hint(kind)),
    }
}

fn instance_state(
    kind: Kind,
    version: Option<String>,
    name: &str,
    state: Option<String>,
    fix: &str,
) -> EngineInfo {
    match state {
        Some(s) if s.eq_ignore_ascii_case("running") => EngineInfo::new(
            kind,
            Availability::Ready,
            version,
            format!("instance {name}"),
        ),
        Some(s) => EngineInfo::new(
            kind,
            Availability::NotRunning,
            version,
            format!("instance {name} is {}: {fix}", s.to_lowercase()),
        ),
        None => EngineInfo::new(
            kind,
            Availability::NeedsSetup,
            version,
            format!("no instance {name}: {fix}"),
        ),
    }
}

#[derive(Clone, Debug)]
enum Transport {
    Lima {
        bin: PathBuf,
        instance: String,
    },
    Multipass {
        bin: PathBuf,
        instance: String,
    },
    Wsl {
        bin: PathBuf,
        distro: Option<String>,
    },
    Parallels {
        bin: PathBuf,
        vm: String,
    },
    Ssh {
        bin: PathBuf,
        target: String,
        port: Option<String>,
        identity: Option<String>,
    },
}

impl Transport {
    fn from_config(kind: Kind, ec: &EngineConfig) -> Result<Self> {
        if let Some(target) = &ec.ssh {
            let bin = find_binary(&["ssh"], &[]).context("no ssh client found")?;
            let (target, port) = split_port(target);
            return Ok(Transport::Ssh {
                bin,
                target,
                port,
                identity: ec
                    .identity
                    .as_ref()
                    .map(|p| super::expand_home(p).to_string_lossy().into_owned()),
            });
        }
        let bin = locate(kind).with_context(|| format!("{} is not installed", kind.name()))?;
        let instance = ec
            .instance
            .clone()
            .or(default_instance(kind).map(String::from));
        Ok(match (kind, instance) {
            (Kind::Lima, Some(instance)) => Transport::Lima { bin, instance },
            (Kind::Multipass, Some(instance)) => Transport::Multipass { bin, instance },
            (Kind::Wsl, distro) => Transport::Wsl { bin, distro },
            (Kind::Parallels, Some(vm)) => Transport::Parallels { bin, vm },
            _ => bail!("{} needs setup: {}", kind.name(), setup_hint(kind)),
        })
    }

    fn command<S: AsRef<OsStr>>(&self, argv: &[S]) -> Command {
        match self {
            Transport::Lima { bin, instance } => {
                let mut c = Command::new(bin);
                c.args(["shell", "--workdir", "/", instance]).args(argv);
                c
            }
            Transport::Multipass { bin, instance } => {
                let mut c = Command::new(bin);
                c.args(["exec", instance, "--"]).args(argv);
                c
            }
            Transport::Wsl { bin, distro } => {
                let mut c = Command::new(bin);
                if let Some(d) = distro {
                    c.args(["-d", d]);
                }
                c.arg("--exec").args(argv);
                c
            }
            Transport::Parallels { bin, vm } => {
                let mut c = Command::new(bin);
                c.args(["exec", vm]).args(argv);
                c
            }
            Transport::Ssh {
                bin,
                target,
                port,
                identity,
            } => {
                let mut c = Command::new(bin);
                c.args([
                    "-o",
                    "BatchMode=yes",
                    "-o",
                    "StrictHostKeyChecking=accept-new",
                    "-o",
                    "LogLevel=ERROR",
                ]);
                if cfg!(unix) {
                    // Reuse one connection for the many short commands a job issues.
                    c.args([
                        "-o",
                        "ControlMaster=auto",
                        "-o",
                        "ControlPath=/tmp/maverick-ssh-%C",
                        "-o",
                        "ControlPersist=60",
                    ]);
                }
                if let Some(p) = port {
                    c.args(["-p", p]);
                }
                if let Some(i) = identity {
                    c.args(["-i", i]);
                }
                c.arg(target).arg("--").args(argv);
                c
            }
        }
    }
}

fn split_port(target: &str) -> (String, Option<String>) {
    match target.rsplit_once(':') {
        Some((host, port)) if !port.is_empty() && port.chars().all(|c| c.is_ascii_digit()) => {
            (host.to_string(), Some(port.to_string()))
        }
        _ => (target.to_string(), None),
    }
}

pub struct VmEngine {
    kind: Kind,
    transport: Transport,
}

impl VmEngine {
    pub fn new(kind: Kind, cfg: &Config) -> Result<Self> {
        Ok(Self {
            kind,
            transport: Transport::from_config(kind, &cfg.engine(kind.id()))?,
        })
    }
}

impl Engine for VmEngine {
    fn kind(&self) -> Kind {
        self.kind
    }

    fn start(&self, req: &SessionRequest, log: &mut dyn FnMut(String)) -> Result<Box<dyn Session>> {
        let base = format!("/tmp/maverick-{}", req.id);
        let workspace = format!("{base}/workspace");
        let runner = format!("{base}/runner");
        let mut session = VmSession {
            transport: self.transport.clone(),
            info: SessionInfo::linux(&workspace, &runner),
            base: base.clone(),
            dirs: HashSet::new(),
            seq: 0,
        };
        session.run(&["rm", "-rf", &base])?;
        session.run(&["mkdir", "-p", &workspace, &runner])?;
        session.dirs.extend([workspace.clone(), runner.clone()]);
        if req.bind {
            log("--bind is not available for VM engines; copying the project instead".into());
        }
        log(format!(
            "Uploading project to the {} guest",
            self.kind.name()
        ));
        session.upload(req.source, &workspace, true)?;
        let probe = probe_unix(&mut session)?;
        apply_probe(&mut session.info, probe);
        log(format!(
            "Guest: Linux {} · default shell: {}",
            session.info.arch, session.info.default_shell
        ));
        Ok(Box::new(session))
    }
}

struct VmSession {
    transport: Transport,
    info: SessionInfo,
    base: String,
    /// Directories already known to exist in the guest.
    dirs: HashSet<String>,
    seq: usize,
}
