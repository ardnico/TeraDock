use std::ffi::OsString;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use serde::Serialize;
use wait_timeout::ChildExt;

use crate::cmdset::{CmdSetStore, StepOnError};
use crate::error::{CoreError, Result};
use crate::oplog::{self, OpLogEntry};
use crate::parser::{parse_output, ParserSpec};
use crate::profile::{Profile, ProfileStore, ProfileType};

pub struct CmdSetRunRequest<'a> {
    pub profile_id: &'a str,
    pub cmdset_id: &'a str,
    pub ssh: &'a Path,
    pub ssh_auth_args: &'a [OsString],
}

#[derive(Debug, Clone, Serialize)]
pub struct CmdStepRunResult {
    pub ord: i64,
    pub cmd: String,
    pub ok: bool,
    pub exit_code: i32,
    pub duration_ms: i64,
    pub stdout: String,
    pub stderr: String,
    pub parsed: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct CmdSetRunResult {
    pub ok: bool,
    pub exit_code: i32,
    pub duration_ms: i64,
    pub stdout: String,
    pub stderr: String,
    pub steps: Vec<CmdStepRunResult>,
}

pub fn run_cmdset_ssh(
    profile_store: &ProfileStore,
    cmdset_store: &CmdSetStore,
    request: CmdSetRunRequest<'_>,
    mut on_step: impl FnMut(&CmdStepRunResult) -> Result<()>,
) -> Result<CmdSetRunResult> {
    let profile = profile_store
        .get(request.profile_id)?
        .ok_or_else(|| CoreError::NotFound(request.profile_id.to_string()))?;
    if profile.profile_type != ProfileType::Ssh {
        return Err(CoreError::InvalidCommandSpec(
            "run only supports SSH profiles for now".to_string(),
        ));
    }
    if cmdset_store.get(request.cmdset_id)?.is_none() {
        return Err(CoreError::NotFound(request.cmdset_id.to_string()));
    }
    let steps = cmdset_store.list_steps(request.cmdset_id)?;
    if steps.is_empty() {
        return Err(CoreError::InvalidCommandSpec(format!(
            "cmdset has no steps: {}",
            request.cmdset_id
        )));
    }

    let run_started = Instant::now();
    let mut stdout_all = String::new();
    let mut stderr_all = String::new();
    let mut step_results = Vec::new();
    let mut overall_ok = true;
    let mut last_exit_code = 0;

    for step in steps {
        let command = build_ssh_command(request.ssh, &profile, request.ssh_auth_args, &step.cmd);
        let step_started = Instant::now();
        let output = match step.timeout_ms {
            Some(ms) => run_with_timeout(command, Duration::from_millis(ms)).map_err(|err| {
                CoreError::CommandExecution(format!(
                    "step {} timed out after {ms}ms: {err}",
                    step.ord
                ))
            })?,
            None => command_output(command)?,
        };
        let duration_ms = step_started.elapsed().as_millis() as i64;
        let exit_code = output.status.code().unwrap_or_default();
        let ok = output.status.success();
        last_exit_code = exit_code;
        if !ok {
            overall_ok = false;
        }

        let stdout_text = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr_text = String::from_utf8_lossy(&output.stderr).to_string();
        stdout_all.push_str(&stdout_text);
        stderr_all.push_str(&stderr_text);

        let parser_def = match &step.parser_spec {
            ParserSpec::Regex(id) => cmdset_store.get_parser(id)?,
            _ => None,
        };
        let parsed = parse_output(&step.parser_spec, &stdout_text, parser_def.as_ref())?;

        let step_result = CmdStepRunResult {
            ord: step.ord,
            cmd: step.cmd,
            ok,
            exit_code,
            stdout: stdout_text,
            stderr: stderr_text,
            duration_ms,
            parsed,
        };
        on_step(&step_result)?;
        step_results.push(step_result);

        if !ok && step.on_error == StepOnError::Stop {
            break;
        }
    }

    let duration_ms = run_started.elapsed().as_millis() as i64;
    profile_store.touch_last_used(&profile.profile_id)?;
    oplog::log_operation(
        profile_store.conn(),
        OpLogEntry {
            op: "run".into(),
            profile_id: Some(profile.profile_id),
            client_used: Some(request.ssh.to_string_lossy().into_owned()),
            ok: overall_ok,
            exit_code: Some(last_exit_code),
            duration_ms: Some(duration_ms),
            meta_json: Some(serde_json::json!({
                "cmdset_id": request.cmdset_id,
                "steps_executed": step_results.len(),
            })),
        },
    )?;

    Ok(CmdSetRunResult {
        ok: overall_ok,
        exit_code: last_exit_code,
        duration_ms,
        stdout: stdout_all,
        stderr: stderr_all,
        steps: step_results,
    })
}

fn build_ssh_command(ssh: &Path, profile: &Profile, auth_args: &[OsString], cmd: &str) -> Command {
    let mut command = Command::new(ssh);
    command
        .arg("-p")
        .arg(profile.port.to_string())
        .args(auth_args)
        .arg(format!("{}@{}", profile.user, profile.host))
        .arg(cmd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

fn command_output(mut command: Command) -> Result<Output> {
    command.output().map_err(CoreError::Io)
}

fn run_with_timeout(mut command: Command, timeout: Duration) -> std::io::Result<Output> {
    let mut child = command.spawn()?;
    let status = child.wait_timeout(timeout)?;
    if status.is_none() {
        let _ = child.kill();
        let _ = child.wait();
        return Err(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            format!("timeout after {}ms", timeout.as_millis()),
        ));
    }
    child.wait_with_output()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmdset::{NewCmdSet, NewCmdStep};
    use crate::db;
    use crate::parser::ParserSpec;
    use crate::profile::{DangerLevel, NewProfile, ProfileType};
    use std::fs;

    fn temp_db_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "teradock-{name}-{}-{}.db",
            std::process::id(),
            crate::util::now_ms()
        ))
    }

    fn fake_ssh_path(name: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "teradock-fake-ssh-{name}-{}{}",
            std::process::id(),
            if cfg!(windows) { ".cmd" } else { "" }
        ));
        let script = if cfg!(windows) {
            "@echo off\r\nset \"cmd=%~4\"\r\nif \"%cmd%\"==\"ok-json\" (\r\n  echo {\"ok\":true}\r\n  exit /b 0\r\n)\r\nif \"%cmd%\"==\"fail\" (\r\n  echo bad\r\n  echo err 1>&2\r\n  exit /b 7\r\n)\r\necho %cmd%\r\nexit /b 0\r\n"
        } else {
            "#!/bin/sh\ncmd=\"$4\"\nif [ \"$cmd\" = \"ok-json\" ]; then\n  printf '{\"ok\":true}\\n'\n  exit 0\nfi\nif [ \"$cmd\" = \"fail\" ]; then\n  printf 'bad\\n'\n  printf 'err\\n' >&2\n  exit 7\nfi\nprintf '%s\\n' \"$cmd\"\n"
        };
        fs::write(&path, script).expect("write fake ssh");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&path).expect("metadata").permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&path, perms).expect("set executable");
        }
        path
    }

    fn stores(db_path: &Path) -> (ProfileStore, CmdSetStore, impl FnOnce()) {
        let profile_store = ProfileStore::new(db::init_connection_at(db_path).unwrap());
        let cmdset_store = CmdSetStore::new(db::init_connection_at(db_path).unwrap());
        let cleanup_path = db_path.to_path_buf();
        let cleanup = move || {
            let _ = fs::remove_file(cleanup_path);
        };
        (profile_store, cmdset_store, cleanup)
    }

    fn insert_profile(store: &ProfileStore) {
        store
            .insert(NewProfile {
                profile_id: Some("p_test".to_string()),
                name: "Test".to_string(),
                profile_type: ProfileType::Ssh,
                host: "example.com".to_string(),
                port: 22,
                user: "alice".to_string(),
                danger_level: DangerLevel::Normal,
                group: None,
                tags: Vec::new(),
                note: None,
                initial_send: None,
                client_overrides: None,
            })
            .unwrap();
    }

    fn insert_cmdset(store: &mut CmdSetStore, steps: Vec<NewCmdStep>) {
        store
            .insert(NewCmdSet {
                cmdset_id: Some("c_test".to_string()),
                name: "Test commands".to_string(),
                vars: None,
                steps,
            })
            .unwrap();
    }

    #[test]
    fn runs_steps_and_applies_parser() {
        let db_path = temp_db_path("cmdset-run");
        let (profile_store, mut cmdset_store, cleanup) = stores(&db_path);
        insert_profile(&profile_store);
        insert_cmdset(
            &mut cmdset_store,
            vec![NewCmdStep {
                cmd: "ok-json".to_string(),
                timeout_ms: Some(5_000),
                on_error: StepOnError::Stop,
                parser_spec: ParserSpec::Json,
            }],
        );
        let fake_ssh = fake_ssh_path("json");

        let result = run_cmdset_ssh(
            &profile_store,
            &cmdset_store,
            CmdSetRunRequest {
                profile_id: "p_test",
                cmdset_id: "c_test",
                ssh: &fake_ssh,
                ssh_auth_args: &[],
            },
            |_| Ok(()),
        )
        .unwrap();

        assert!(result.ok);
        assert_eq!(result.steps.len(), 1);
        assert_eq!(result.steps[0].parsed, serde_json::json!({ "ok": true }));
        let log_count: i64 = profile_store
            .conn()
            .query_row("SELECT COUNT(*) FROM op_logs WHERE op = 'run'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(log_count, 1);

        let _ = fs::remove_file(fake_ssh);
        cleanup();
    }

    #[test]
    fn stops_on_error_when_step_requests_stop() {
        let db_path = temp_db_path("cmdset-stop");
        let (profile_store, mut cmdset_store, cleanup) = stores(&db_path);
        insert_profile(&profile_store);
        insert_cmdset(
            &mut cmdset_store,
            vec![
                NewCmdStep {
                    cmd: "fail".to_string(),
                    timeout_ms: Some(5_000),
                    on_error: StepOnError::Stop,
                    parser_spec: ParserSpec::Raw,
                },
                NewCmdStep {
                    cmd: "after".to_string(),
                    timeout_ms: Some(5_000),
                    on_error: StepOnError::Stop,
                    parser_spec: ParserSpec::Raw,
                },
            ],
        );
        let fake_ssh = fake_ssh_path("stop");

        let result = run_cmdset_ssh(
            &profile_store,
            &cmdset_store,
            CmdSetRunRequest {
                profile_id: "p_test",
                cmdset_id: "c_test",
                ssh: &fake_ssh,
                ssh_auth_args: &[],
            },
            |_| Ok(()),
        )
        .unwrap();

        assert!(!result.ok);
        assert_eq!(result.exit_code, 7);
        assert_eq!(result.steps.len(), 1);

        let _ = fs::remove_file(fake_ssh);
        cleanup();
    }

    #[test]
    fn continues_on_error_when_step_requests_continue() {
        let db_path = temp_db_path("cmdset-continue");
        let (profile_store, mut cmdset_store, cleanup) = stores(&db_path);
        insert_profile(&profile_store);
        insert_cmdset(
            &mut cmdset_store,
            vec![
                NewCmdStep {
                    cmd: "fail".to_string(),
                    timeout_ms: Some(5_000),
                    on_error: StepOnError::Continue,
                    parser_spec: ParserSpec::Raw,
                },
                NewCmdStep {
                    cmd: "after".to_string(),
                    timeout_ms: Some(5_000),
                    on_error: StepOnError::Stop,
                    parser_spec: ParserSpec::Raw,
                },
            ],
        );
        let fake_ssh = fake_ssh_path("continue");

        let result = run_cmdset_ssh(
            &profile_store,
            &cmdset_store,
            CmdSetRunRequest {
                profile_id: "p_test",
                cmdset_id: "c_test",
                ssh: &fake_ssh,
                ssh_auth_args: &[],
            },
            |_| Ok(()),
        )
        .unwrap();

        assert!(!result.ok);
        assert_eq!(result.steps.len(), 2);
        assert!(result.stdout.contains("after"));

        let _ = fs::remove_file(fake_ssh);
        cleanup();
    }
}
