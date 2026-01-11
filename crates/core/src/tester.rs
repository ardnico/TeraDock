use crate::profile::{Profile, ProfileType};
use serde::Serialize;
use serde_json::Value;
use std::ffi::OsString;
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize)]
pub struct TestCheck {
    pub name: String,
    pub ok: bool,
    pub skipped: bool,
    pub duration_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TestReport {
    pub profile_id: String,
    pub profile_type: String,
    pub host: String,
    pub port: u16,
    pub duration_ms: i64,
    pub ok: bool,
    pub checks: Vec<TestCheck>,
}

#[derive(Debug, Clone)]
pub struct SshBatchCommand {
    pub path: PathBuf,
    pub user: String,
    pub host: String,
    pub port: u16,
    pub auth_args: Vec<OsString>,
    pub connect_timeout: Duration,
}

#[derive(Debug, Clone)]
pub struct TestOptions {
    pub tcp_timeout: Duration,
    pub ssh: Option<SshBatchCommand>,
}

pub fn run_profile_test(profile: &Profile, options: &TestOptions) -> TestReport {
    let started = Instant::now();
    let mut checks = Vec::new();

    let (dns_ok, addresses, dns_detail, dns_duration) = resolve_dns(&profile.host, profile.port);
    checks.push(TestCheck {
        name: "dns".into(),
        ok: dns_ok,
        skipped: false,
        duration_ms: Some(dns_duration),
        detail: dns_detail.clone(),
        data: if dns_ok {
            Some(serde_json::json!({
                "addresses": addresses.iter().map(|addr| addr.to_string()).collect::<Vec<_>>()
            }))
        } else {
            None
        },
        exit_code: None,
    });

    let (tcp_ok, tcp_detail, tcp_duration, connected_addr) = if dns_ok && !addresses.is_empty() {
        connect_tcp(&addresses, options.tcp_timeout)
    } else if dns_ok {
        (
            false,
            Some("no addresses resolved".to_string()),
            0,
            None,
        )
    } else {
        (
            false,
            Some("skipped (dns failed)".to_string()),
            0,
            None,
        )
    };

    checks.push(TestCheck {
        name: "tcp".into(),
        ok: tcp_ok,
        skipped: dns_ok && addresses.is_empty() || !dns_ok,
        duration_ms: Some(tcp_duration),
        detail: tcp_detail.clone(),
        data: if !addresses.is_empty() {
            Some(serde_json::json!({
                "addresses": addresses.iter().map(|addr| addr.to_string()).collect::<Vec<_>>(),
                "connected": connected_addr.map(|addr| addr.to_string()),
            }))
        } else {
            None
        },
        exit_code: None,
    });

    if let Some(ssh) = options.ssh.as_ref() {
        let (ssh_ok, ssh_detail, ssh_duration, exit_code, stderr) = if tcp_ok {
            run_ssh_batch(ssh)
        } else if dns_ok {
            (
                false,
                "skipped (tcp failed)".to_string(),
                0,
                None,
                None,
            )
        } else {
            (
                false,
                "skipped (dns failed)".to_string(),
                0,
                None,
                None,
            )
        };
        checks.push(TestCheck {
            name: "ssh".into(),
            ok: ssh_ok,
            skipped: !tcp_ok,
            duration_ms: Some(ssh_duration),
            detail: Some(ssh_detail),
            data: Some(serde_json::json!({
                "client": ssh.path.to_string_lossy(),
                "stderr": stderr,
            })),
            exit_code,
        });
    }

    let ok = checks.iter().all(|check| check.ok || check.skipped);
    let duration_ms = started.elapsed().as_millis() as i64;

    TestReport {
        profile_id: profile.profile_id.clone(),
        profile_type: profile.profile_type.to_string(),
        host: profile.host.clone(),
        port: profile.port,
        duration_ms,
        ok,
        checks,
    }
}

fn resolve_dns(host: &str, port: u16) -> (bool, Vec<SocketAddr>, Option<String>, i64) {
    let started = Instant::now();
    let addrs = format!("{host}:{port}");
    let result = addrs.to_socket_addrs();
    let duration_ms = started.elapsed().as_millis() as i64;
    match result {
        Ok(iter) => {
            let addresses: Vec<SocketAddr> = iter.collect();
            if addresses.is_empty() {
                (false, addresses, Some("no addresses resolved".to_string()), duration_ms)
            } else {
                (
                    true,
                    addresses,
                    Some(format!("{} address(es) resolved", addresses.len())),
                    duration_ms,
                )
            }
        }
        Err(err) => (false, vec![], Some(err.to_string()), duration_ms),
    }
}

fn connect_tcp(
    addresses: &[SocketAddr],
    timeout: Duration,
) -> (bool, Option<String>, i64, Option<SocketAddr>) {
    let started = Instant::now();
    let mut last_error: Option<String> = None;
    for addr in addresses {
        match TcpStream::connect_timeout(addr, timeout) {
            Ok(_) => {
                let duration_ms = started.elapsed().as_millis() as i64;
                return (
                    true,
                    Some(format!("connected to {addr}")),
                    duration_ms,
                    Some(*addr),
                );
            }
            Err(err) => {
                last_error = Some(err.to_string());
            }
        }
    }
    let duration_ms = started.elapsed().as_millis() as i64;
    (
        false,
        last_error.or_else(|| Some("unable to connect".to_string())),
        duration_ms,
        None,
    )
}

fn run_ssh_batch(
    ssh: &SshBatchCommand,
) -> (bool, String, i64, Option<i32>, Option<String>) {
    let started = Instant::now();
    let timeout_secs = ssh.connect_timeout.as_secs().max(1);
    let mut command = Command::new(&ssh.path);
    command
        .arg("-p")
        .arg(ssh.port.to_string())
        .args(&ssh.auth_args)
        .arg("-o")
        .arg("BatchMode=yes")
        .arg("-o")
        .arg(format!("ConnectTimeout={timeout_secs}"))
        .arg("-T")
        .arg(format!("{}@{}", ssh.user, ssh.host))
        .arg("exit 0")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let output = match command.output() {
        Ok(output) => output,
        Err(err) => {
            let duration_ms = started.elapsed().as_millis() as i64;
            return (
                false,
                format!("failed to execute ssh: {err}"),
                duration_ms,
                None,
                None,
            );
        }
    };

    let duration_ms = started.elapsed().as_millis() as i64;
    let exit_code = output.status.code();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if output.status.success() {
        (true, "batch mode auth ok".to_string(), duration_ms, exit_code, None)
    } else {
        let detail = if stderr.is_empty() {
            "ssh batch mode failed".to_string()
        } else {
            stderr.clone()
        };
        (
            false,
            detail,
            duration_ms,
            exit_code,
            Some(stderr),
        )
    }
}

impl Default for TestOptions {
    fn default() -> Self {
        Self {
            tcp_timeout: Duration::from_secs(5),
            ssh: None,
        }
    }
}

impl TestReport {
    pub fn ssh_exit_code(&self) -> Option<i32> {
        self.checks
            .iter()
            .find(|check| check.name == "ssh")
            .and_then(|check| check.exit_code)
    }
}

impl TestOptions {
    pub fn with_ssh(mut self, ssh: SshBatchCommand) -> Self {
        self.ssh = Some(ssh);
        self
    }
}

impl SshBatchCommand {
    pub fn new(
        path: PathBuf,
        user: String,
        host: String,
        port: u16,
        auth_args: Vec<OsString>,
        connect_timeout: Duration,
    ) -> Self {
        Self {
            path,
            user,
            host,
            port,
            auth_args,
            connect_timeout,
        }
    }

    pub fn client_label(&self) -> String {
        self.path.to_string_lossy().into_owned()
    }
}

impl TestCheck {
    pub fn is_failed(&self) -> bool {
        !self.ok && !self.skipped
    }
}

pub fn is_network_profile(profile: &Profile) -> bool {
    matches!(profile.profile_type, ProfileType::Ssh | ProfileType::Telnet)
}
