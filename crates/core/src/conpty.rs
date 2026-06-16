use std::ffi::OsString;
use std::fs::OpenOptions;
use std::io::{self, Read, Write};
use std::path::Path;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use portable_pty::{CommandBuilder, PtySize};

use crate::session_log;

fn conpty_debug(enabled: bool, message: impl std::fmt::Display) {
    if enabled {
        eprintln!("debug: {message}");
    }
}

struct RawModeGuard;

impl RawModeGuard {
    fn enter() -> Result<Self> {
        enable_raw_mode().context("failed to enable raw mode")?;
        Ok(Self)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
    }
}

#[derive(Clone, Copy)]
pub struct ConptyRunOptions {
    pub debug: bool,
    pub startup_timeout: Option<Duration>,
}

pub struct ConptyChildReport {
    pub exit_code: Option<i32>,
    pub first_output_received: bool,
}

struct ConptyOutputThread {
    handle: thread::JoinHandle<()>,
    result_rx: mpsc::Receiver<io::Result<u64>>,
}

struct ConptyInputThread {
    handle: thread::JoinHandle<()>,
    done_rx: mpsc::Receiver<()>,
}

struct ConptyWaitThread {
    handle: thread::JoinHandle<()>,
    done_rx: mpsc::Receiver<()>,
}

struct ConptyTimerThread {
    handle: thread::JoinHandle<()>,
    done_rx: mpsc::Receiver<()>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConptySyntheticResponse {
    CursorPosition,
    DeviceStatusOk,
}

impl ConptySyntheticResponse {
    fn bytes(self) -> &'static [u8] {
        match self {
            Self::CursorPosition => b"\x1b[1;1R",
            Self::DeviceStatusOk => b"\x1b[0n",
        }
    }

    fn debug_label(self) -> &'static str {
        match self {
            Self::CursorPosition => "cursor_position",
            Self::DeviceStatusOk => "device_status_ok",
        }
    }
}

enum ConptyInputCommand {
    WriteSynthetic { response: ConptySyntheticResponse },
}

#[derive(Debug)]
pub enum ConptyEvent {
    FirstOutput { bytes: usize },
    OutputChunk { bytes: usize },
    ChildExited { exit_code: Option<i32> },
    StartupTimeout,
    UserAbort,
    OutputError { message: String },
    InputError { message: String },
}

#[derive(Debug)]
pub enum ConptyLoopMessage {
    Event(ConptyEvent),
    ChildWaitError { message: String },
}

fn send_conpty_event(tx: &mpsc::Sender<ConptyLoopMessage>, event: ConptyEvent) {
    let _ = tx.send(ConptyLoopMessage::Event(event));
}

pub struct ConptyChildFailure {
    pub status: &'static str,
    pub phase: &'static str,
    pub reason: &'static str,
    pub error: anyhow::Error,
}

impl ConptyChildFailure {
    fn failed(phase: &'static str, reason: &'static str, error: anyhow::Error) -> Self {
        Self {
            status: session_log::SESSION_LOG_STATUS_FAILED,
            phase,
            reason,
            error,
        }
    }

    fn aborted(phase: &'static str, reason: &'static str, error: anyhow::Error) -> Self {
        Self {
            status: session_log::SESSION_LOG_STATUS_ABORTED,
            phase,
            reason,
            error,
        }
    }

    pub fn into_error(self) -> anyhow::Error {
        anyhow!(
            "ConPTY SSH {status} during {phase}: {reason}: {error}",
            status = self.status,
            phase = self.phase,
            reason = self.reason,
            error = self.error
        )
    }
}

pub fn run_conpty_ssh_child(
    executable: &Path,
    args: &[OsString],
    log_path: &Path,
    options: ConptyRunOptions,
) -> std::result::Result<ConptyChildReport, ConptyChildFailure> {
    conpty_debug(options.debug, "child spawn phase: create log");
    let log_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(log_path)
        .map_err(|err| {
            ConptyChildFailure::failed(
                session_log::SESSION_LOG_FAILURE_PHASE_CREATE_LOG,
                session_log::SESSION_LOG_FAILURE_REASON_CREATE_LOG_FAILED,
                err.into(),
            )
        })?;
    conpty_debug(options.debug, "child spawn phase: open pty");
    let pty_system = portable_pty::native_pty_system();
    let pair = pty_system.openpty(current_pty_size()).map_err(|err| {
        ConptyChildFailure::failed(
            session_log::SESSION_LOG_FAILURE_PHASE_OPEN_PTY,
            session_log::SESSION_LOG_FAILURE_REASON_OPEN_PTY_FAILED,
            err,
        )
    })?;
    let master = pair.master;
    let slave = pair.slave;
    let reader = master.try_clone_reader().map_err(|err| {
        ConptyChildFailure::failed(
            session_log::SESSION_LOG_FAILURE_PHASE_OPEN_PTY,
            session_log::SESSION_LOG_FAILURE_REASON_OPEN_PTY_FAILED,
            err,
        )
    })?;
    let writer = master.take_writer().map_err(|err| {
        ConptyChildFailure::failed(
            session_log::SESSION_LOG_FAILURE_PHASE_OPEN_PTY,
            session_log::SESSION_LOG_FAILURE_REASON_OPEN_PTY_FAILED,
            err,
        )
    })?;
    conpty_debug(options.debug, "child spawn phase: enter raw mode");
    let raw_mode = match RawModeGuard::enter() {
        Ok(guard) => guard,
        Err(err) => {
            drop(log_file);
            return Err(ConptyChildFailure::failed(
                session_log::SESSION_LOG_FAILURE_PHASE_ENTER_RAW_MODE,
                session_log::SESSION_LOG_FAILURE_REASON_RAW_MODE_FAILED,
                err,
            ));
        }
    };
    let mut cmd = CommandBuilder::new(executable.as_os_str());
    cmd.args(args);
    conpty_debug(options.debug, "child spawn phase: spawn child");
    let child = match slave.spawn_command(cmd) {
        Ok(child) => child,
        Err(err) => {
            drop(raw_mode);
            drop(log_file);
            return Err(ConptyChildFailure::failed(
                session_log::SESSION_LOG_FAILURE_PHASE_SPAWN_CHILD,
                session_log::SESSION_LOG_FAILURE_REASON_SPAWN_CHILD_FAILED,
                err,
            ));
        }
    };
    conpty_debug(options.debug, "child spawned");
    drop(slave);

    let first_output_received = Arc::new(AtomicBool::new(false));
    let cancel = Arc::new(AtomicBool::new(false));
    let (event_tx, event_rx) = mpsc::channel();
    let (input_command_tx, input_command_rx) = mpsc::channel();
    let output_thread = spawn_conpty_output_thread(
        reader,
        log_file,
        Arc::clone(&first_output_received),
        event_tx.clone(),
        input_command_tx,
        options,
    );
    let mut child_killer = child.clone_killer();
    let timeout_child_killer = child_killer.clone_killer();
    let wait_thread =
        spawn_conpty_wait_thread(child, Arc::clone(&cancel), event_tx.clone(), options);
    let input_thread = spawn_conpty_input_thread(
        writer,
        master,
        Arc::clone(&cancel),
        input_command_rx,
        event_tx.clone(),
        options,
    );
    let timeout_thread = spawn_conpty_startup_timeout_thread(
        options.startup_timeout,
        Arc::clone(&first_output_received),
        Arc::clone(&cancel),
        event_tx.clone(),
        timeout_child_killer,
        options,
    );
    drop(event_tx);

    let mut output_bytes = 0_u64;
    let startup_deadline = options
        .startup_timeout
        .and_then(|timeout| Instant::now().checked_add(timeout));
    let loop_result = loop {
        let Some(message) =
            recv_conpty_loop_message(&event_rx, &first_output_received, startup_deadline)
        else {
            break Err(ConptyChildFailure::failed(
                session_log::SESSION_LOG_FAILURE_PHASE_CHILD_WAIT,
                session_log::SESSION_LOG_FAILURE_REASON_CHILD_WAIT_FAILED,
                anyhow!("ConPTY event loop ended without child status"),
            ));
        };
        match message {
            ConptyLoopMessage::Event(ConptyEvent::FirstOutput { bytes }) => {
                conpty_debug(
                    options.debug,
                    format_args!("first output received: {bytes} bytes"),
                );
            }
            ConptyLoopMessage::Event(ConptyEvent::OutputChunk { bytes }) => {
                output_bytes += bytes as u64;
            }
            ConptyLoopMessage::Event(ConptyEvent::ChildExited { exit_code }) => {
                let exit_code_text = exit_code
                    .map(|code| code.to_string())
                    .unwrap_or_else(|| "unavailable".to_string());
                conpty_debug(
                    options.debug,
                    format_args!("child exited: code {exit_code_text}"),
                );
                break Ok(exit_code);
            }
            ConptyLoopMessage::Event(ConptyEvent::StartupTimeout) => {
                if first_output_received.load(Ordering::SeqCst) {
                    conpty_debug(options.debug, "startup timeout ignored after first output");
                    continue;
                }
                let timeout = options.startup_timeout.unwrap_or(Duration::from_secs(0));
                conpty_debug(
                    options.debug,
                    format_args!("startup timeout after {} seconds", timeout.as_secs()),
                );
                conpty_debug(options.debug, "first output received: no");
                eprintln!();
                eprintln!(
                    "Error: no ConPTY output received within {} seconds.",
                    timeout.as_secs()
                );
                eprintln!("Aborting ConPTY child...");
                kill_conpty_child(&mut child_killer, options);
                break Err(ConptyChildFailure::failed(
                    session_log::SESSION_LOG_FAILURE_PHASE_WAITING_INITIAL_OUTPUT,
                    session_log::SESSION_LOG_FAILURE_REASON_INITIAL_OUTPUT_TIMEOUT,
                    anyhow!(
                        "no ConPTY output received within {} seconds",
                        timeout.as_secs()
                    ),
                ));
            }
            ConptyLoopMessage::Event(ConptyEvent::UserAbort) => {
                conpty_debug(options.debug, "user abort received");
                kill_conpty_child(&mut child_killer, options);
                break Err(ConptyChildFailure::aborted(
                    session_log::SESSION_LOG_FAILURE_PHASE_USER_ABORT,
                    session_log::SESSION_LOG_FAILURE_REASON_CTRL_C,
                    anyhow!("aborted by Ctrl-C"),
                ));
            }
            ConptyLoopMessage::Event(ConptyEvent::OutputError { message }) => {
                kill_conpty_child(&mut child_killer, options);
                break Err(ConptyChildFailure::failed(
                    session_log::SESSION_LOG_FAILURE_PHASE_OUTPUT_BRIDGE,
                    session_log::SESSION_LOG_FAILURE_REASON_OUTPUT_BRIDGE_FAILED,
                    anyhow!(message),
                ));
            }
            ConptyLoopMessage::Event(ConptyEvent::InputError { message }) => {
                kill_conpty_child(&mut child_killer, options);
                break Err(ConptyChildFailure::failed(
                    session_log::SESSION_LOG_FAILURE_PHASE_INPUT_BRIDGE,
                    session_log::SESSION_LOG_FAILURE_REASON_INPUT_BRIDGE_FAILED,
                    anyhow!(message),
                ));
            }
            ConptyLoopMessage::ChildWaitError { message } => {
                kill_conpty_child(&mut child_killer, options);
                eprintln!("Warning: ConPTY child process exit could not be confirmed.");
                break Err(ConptyChildFailure::failed(
                    session_log::SESSION_LOG_FAILURE_PHASE_CHILD_WAIT,
                    session_log::SESSION_LOG_FAILURE_REASON_CHILD_WAIT_FAILED,
                    anyhow!(message),
                ));
            }
        }
    };

    cancel.store(true, Ordering::SeqCst);
    conpty_debug(options.debug, "dropping pty handles");
    conpty_debug(options.debug, "joining threads best-effort");
    let input_shutdown = join_conpty_input_thread(input_thread, Duration::from_millis(500));
    drop(raw_mode);
    conpty_debug(options.debug, "terminal restored");
    match input_shutdown {
        Ok(()) => conpty_debug(
            options.debug,
            "thread shutdown status: input bridge stopped",
        ),
        Err(err) => conpty_debug(
            options.debug,
            format_args!("thread shutdown status: input bridge incomplete: {err}"),
        ),
    }

    let wait_shutdown = join_conpty_wait_thread_best_effort(wait_thread, Duration::from_secs(3));
    match &wait_shutdown {
        Ok(()) => conpty_debug(options.debug, "thread shutdown status: child wait stopped"),
        Err(err) => {
            conpty_debug(
                options.debug,
                format_args!("thread shutdown status: child wait incomplete: {err}"),
            );
            eprintln!("Warning: ConPTY child process did not confirm exit after cleanup.");
        }
    }
    let timeout_shutdown =
        join_conpty_timer_thread_best_effort(timeout_thread, Duration::from_millis(200));
    match &timeout_shutdown {
        Ok(()) => conpty_debug(
            options.debug,
            "thread shutdown status: startup timer stopped",
        ),
        Err(err) => conpty_debug(
            options.debug,
            format_args!("thread shutdown status: startup timer incomplete: {err}"),
        ),
    }

    let exit_code = match loop_result {
        Ok(exit_code) => {
            wait_shutdown.map_err(|err| {
                ConptyChildFailure::failed(
                    session_log::SESSION_LOG_FAILURE_PHASE_CHILD_WAIT,
                    session_log::SESSION_LOG_FAILURE_REASON_CHILD_WAIT_FAILED,
                    err,
                )
            })?;
            exit_code
        }
        Err(failure) => {
            if let Err(err) =
                join_conpty_output_thread_best_effort(output_thread, Duration::from_millis(1000))
            {
                conpty_debug(
                    options.debug,
                    format_args!("thread shutdown status: output reader incomplete: {err}"),
                );
            } else {
                conpty_debug(
                    options.debug,
                    "thread shutdown status: output reader stopped",
                );
            }
            return Err(failure);
        }
    };
    let output_total = join_conpty_output_thread(output_thread, Duration::from_millis(1000))
        .map_err(|err| {
            ConptyChildFailure::failed(
                session_log::SESSION_LOG_FAILURE_PHASE_OUTPUT_BRIDGE,
                session_log::SESSION_LOG_FAILURE_REASON_OUTPUT_BRIDGE_FAILED,
                err,
            )
        })?;
    conpty_debug(
        options.debug,
        "thread shutdown status: output reader stopped",
    );
    conpty_debug(
        options.debug,
        format_args!(
            "output reader completed: {output_total} bytes; event loop observed {output_bytes} bytes"
        ),
    );
    Ok(ConptyChildReport {
        exit_code,
        first_output_received: first_output_received.load(Ordering::SeqCst),
    })
}

pub fn recv_conpty_loop_message(
    event_rx: &mpsc::Receiver<ConptyLoopMessage>,
    first_output_received: &AtomicBool,
    startup_deadline: Option<Instant>,
) -> Option<ConptyLoopMessage> {
    if first_output_received.load(Ordering::SeqCst) {
        return event_rx.recv().ok();
    }
    let Some(deadline) = startup_deadline else {
        return event_rx.recv().ok();
    };
    let now = Instant::now();
    if now >= deadline {
        return Some(ConptyLoopMessage::Event(ConptyEvent::StartupTimeout));
    }
    match event_rx.recv_timeout(deadline.duration_since(now)) {
        Ok(message) => Some(message),
        Err(mpsc::RecvTimeoutError::Timeout) => {
            Some(ConptyLoopMessage::Event(ConptyEvent::StartupTimeout))
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => None,
    }
}

fn spawn_conpty_output_thread(
    mut reader: Box<dyn Read + Send>,
    log_file: std::fs::File,
    first_output_received: Arc<AtomicBool>,
    event_tx: mpsc::Sender<ConptyLoopMessage>,
    input_command_tx: mpsc::Sender<ConptyInputCommand>,
    options: ConptyRunOptions,
) -> ConptyOutputThread {
    let (result_tx, result_rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        let result = tee_conpty_output(
            &mut reader,
            log_file,
            first_output_received,
            &event_tx,
            &input_command_tx,
            options,
        );
        if let Err(err) = &result {
            send_conpty_event(
                &event_tx,
                ConptyEvent::OutputError {
                    message: err.to_string(),
                },
            );
        }
        let _ = result_tx.send(result);
    });
    ConptyOutputThread { handle, result_rx }
}

fn spawn_conpty_input_thread(
    mut writer: Box<dyn Write + Send>,
    master: Box<dyn portable_pty::MasterPty + Send>,
    cancel: Arc<AtomicBool>,
    input_command_rx: mpsc::Receiver<ConptyInputCommand>,
    event_tx: mpsc::Sender<ConptyLoopMessage>,
    options: ConptyRunOptions,
) -> ConptyInputThread {
    let (done_tx, done_rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        conpty_debug(options.debug, "input bridge started");
        let result = run_conpty_input_bridge(
            &mut writer,
            master.as_ref(),
            &cancel,
            &input_command_rx,
            &event_tx,
            options,
        );
        if let Err(err) = result {
            send_conpty_event(
                &event_tx,
                ConptyEvent::InputError {
                    message: err.to_string(),
                },
            );
        }
        drop(writer);
        drop(master);
        let _ = done_tx.send(());
    });
    ConptyInputThread { handle, done_rx }
}

fn run_conpty_input_bridge(
    writer: &mut Box<dyn Write + Send>,
    master: &dyn portable_pty::MasterPty,
    cancel: &Arc<AtomicBool>,
    input_command_rx: &mpsc::Receiver<ConptyInputCommand>,
    event_tx: &mpsc::Sender<ConptyLoopMessage>,
    options: ConptyRunOptions,
) -> Result<()> {
    while !cancel.load(Ordering::SeqCst) {
        drain_conpty_input_commands(writer, input_command_rx, options)?;
        if !event::poll(Duration::from_millis(20))
            .context("failed to poll terminal input for ConPTY")?
        {
            continue;
        }
        match event::read().context("failed to read terminal input for ConPTY")? {
            Event::Key(key) => {
                if key_event_is_ctrl_c(key) {
                    conpty_debug(options.debug, "ctrl-c input event received");
                    cancel.store(true, Ordering::SeqCst);
                    send_conpty_event(event_tx, ConptyEvent::UserAbort);
                    return Ok(());
                }
                if let Some(bytes) = key_event_to_pty_bytes(key) {
                    writer
                        .write_all(&bytes)
                        .context("failed to write terminal input to ConPTY")?;
                    writer
                        .flush()
                        .context("failed to flush terminal input to ConPTY")?;
                }
            }
            Event::Resize(cols, rows) => {
                master
                    .resize(pty_size(cols, rows))
                    .context("failed to resize ConPTY")?;
            }
            _ => {}
        }
    }
    Ok(())
}

fn drain_conpty_input_commands(
    writer: &mut Box<dyn Write + Send>,
    input_command_rx: &mpsc::Receiver<ConptyInputCommand>,
    options: ConptyRunOptions,
) -> Result<()> {
    while let Ok(ConptyInputCommand::WriteSynthetic { response }) = input_command_rx.try_recv() {
        writer
            .write_all(response.bytes())
            .context("failed to write synthetic terminal response to ConPTY")?;
        writer
            .flush()
            .context("failed to flush synthetic terminal response to ConPTY")?;
        conpty_debug(
            options.debug,
            format_args!(
                "synthetic terminal response sent: {}",
                response.debug_label()
            ),
        );
    }
    Ok(())
}

fn spawn_conpty_wait_thread(
    mut child: Box<dyn portable_pty::Child + Send + Sync>,
    cancel: Arc<AtomicBool>,
    event_tx: mpsc::Sender<ConptyLoopMessage>,
    options: ConptyRunOptions,
) -> ConptyWaitThread {
    let (done_tx, done_rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        conpty_debug(options.debug, "child wait started");
        let mut cancel_seen_at: Option<Instant> = None;
        let message = loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    let exit_code = Some(conpty_exit_code(&status));
                    let exit_code_text = exit_code
                        .map(|code| code.to_string())
                        .unwrap_or_else(|| "unavailable".to_string());
                    conpty_debug(
                        options.debug,
                        format_args!("child exited: code {exit_code_text}"),
                    );
                    break ConptyLoopMessage::Event(ConptyEvent::ChildExited { exit_code });
                }
                Ok(None) => {}
                Err(err) => {
                    break ConptyLoopMessage::ChildWaitError {
                        message: err.to_string(),
                    }
                }
            }
            if cancel.load(Ordering::SeqCst) {
                let first_seen = cancel_seen_at.get_or_insert_with(Instant::now);
                if first_seen.elapsed() >= Duration::from_secs(2) {
                    break ConptyLoopMessage::ChildWaitError {
                        message: "ConPTY child did not exit after shutdown request".to_string(),
                    };
                }
            }
            thread::sleep(Duration::from_millis(20));
        };
        let _ = event_tx.send(message);
        let _ = done_tx.send(());
    });
    ConptyWaitThread { handle, done_rx }
}

fn spawn_conpty_startup_timeout_thread(
    timeout: Option<Duration>,
    first_output_received: Arc<AtomicBool>,
    cancel: Arc<AtomicBool>,
    event_tx: mpsc::Sender<ConptyLoopMessage>,
    mut child_killer: Box<dyn portable_pty::ChildKiller + Send + Sync>,
    options: ConptyRunOptions,
) -> Option<ConptyTimerThread> {
    let Some(timeout) = timeout else {
        conpty_debug(options.debug, "startup timeout disabled");
        return None;
    };
    conpty_debug(
        options.debug,
        format_args!("startup timeout armed: {} seconds", timeout.as_secs()),
    );
    let (done_tx, done_rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        let started = Instant::now();
        while started.elapsed() < timeout {
            if cancel.load(Ordering::SeqCst) || first_output_received.load(Ordering::SeqCst) {
                let _ = done_tx.send(());
                return;
            }
            thread::sleep(Duration::from_millis(20));
        }
        if !cancel.load(Ordering::SeqCst) && !first_output_received.load(Ordering::SeqCst) {
            conpty_debug(options.debug, "startup watchdog killing child");
            if let Err(err) = child_killer.kill() {
                conpty_debug(
                    options.debug,
                    format_args!("startup watchdog child kill failed: {err}"),
                );
            }
            send_conpty_event(&event_tx, ConptyEvent::StartupTimeout);
        }
        let _ = done_tx.send(());
    });
    Some(ConptyTimerThread { handle, done_rx })
}

fn kill_conpty_child(
    child_killer: &mut Box<dyn portable_pty::ChildKiller + Send + Sync>,
    options: ConptyRunOptions,
) {
    conpty_debug(options.debug, "killing child");
    if let Err(err) = child_killer.kill() {
        conpty_debug(options.debug, format_args!("child kill failed: {err}"));
    } else {
        conpty_debug(options.debug, "child killed");
    }
}

fn join_conpty_input_thread(thread: ConptyInputThread, timeout: Duration) -> Result<()> {
    match thread.done_rx.recv_timeout(timeout) {
        Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => match thread.handle.join() {
            Ok(()) => Ok(()),
            Err(_) => Err(anyhow!("ConPTY input bridge thread panicked")),
        },
        Err(mpsc::RecvTimeoutError::Timeout) => Err(anyhow!(
            "ConPTY input bridge did not stop within {} ms",
            timeout.as_millis()
        )),
    }
}

fn join_conpty_wait_thread_best_effort(thread: ConptyWaitThread, timeout: Duration) -> Result<()> {
    match thread.done_rx.recv_timeout(timeout) {
        Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => match thread.handle.join() {
            Ok(()) => Ok(()),
            Err(_) => Err(anyhow!("ConPTY child wait thread panicked")),
        },
        Err(mpsc::RecvTimeoutError::Timeout) => Err(anyhow!(
            "ConPTY child wait did not stop within {} ms",
            timeout.as_millis()
        )),
    }
}

fn join_conpty_timer_thread_best_effort(
    thread: Option<ConptyTimerThread>,
    timeout: Duration,
) -> Result<()> {
    let Some(thread) = thread else {
        return Ok(());
    };
    match thread.done_rx.recv_timeout(timeout) {
        Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => match thread.handle.join() {
            Ok(()) => Ok(()),
            Err(_) => Err(anyhow!("ConPTY startup timer thread panicked")),
        },
        Err(mpsc::RecvTimeoutError::Timeout) => Err(anyhow!(
            "ConPTY startup timer did not stop within {} ms",
            timeout.as_millis()
        )),
    }
}

fn join_conpty_output_thread(output_thread: ConptyOutputThread, timeout: Duration) -> Result<u64> {
    match output_thread.result_rx.recv_timeout(timeout) {
        Ok(result) => {
            match output_thread.handle.join() {
                Ok(()) => {}
                Err(_) => return Err(anyhow!("ConPTY output thread panicked")),
            }
            result.context("ConPTY output tee failed")
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => match output_thread.handle.join() {
            Ok(()) => Ok(0),
            Err(_) => Err(anyhow!("ConPTY output thread panicked")),
        },
        Err(mpsc::RecvTimeoutError::Timeout) => Err(anyhow!(
            "ConPTY output reader did not stop within {} ms",
            timeout.as_millis()
        )),
    }
}

fn join_conpty_output_thread_best_effort(
    output_thread: ConptyOutputThread,
    timeout: Duration,
) -> Result<Option<u64>> {
    match join_conpty_output_thread(output_thread, timeout) {
        Ok(total) => Ok(Some(total)),
        Err(err) => Err(err),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ConptyOutputParseState {
    Ground,
    Escape,
    Csi,
    Osc,
    OscEscape,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ConptyOutputInspection {
    pub has_visible_output: bool,
    pub synthetic_responses: Vec<ConptySyntheticResponse>,
}

pub struct ConptyOutputInspector {
    state: ConptyOutputParseState,
    csi_body: Vec<u8>,
}

impl Default for ConptyOutputInspector {
    fn default() -> Self {
        Self {
            state: ConptyOutputParseState::Ground,
            csi_body: Vec::new(),
        }
    }
}

impl ConptyOutputInspector {
    pub fn inspect(&mut self, bytes: &[u8]) -> ConptyOutputInspection {
        let mut inspection = ConptyOutputInspection::default();
        for &byte in bytes {
            match self.state {
                ConptyOutputParseState::Ground => match byte {
                    0x1b => self.state = ConptyOutputParseState::Escape,
                    0x9b => {
                        self.csi_body.clear();
                        self.state = ConptyOutputParseState::Csi;
                    }
                    0x9d => self.state = ConptyOutputParseState::Osc,
                    0x00..=0x1f | 0x7f | 0x80..=0x9f => {}
                    _ => inspection.has_visible_output = true,
                },
                ConptyOutputParseState::Escape => match byte {
                    b'[' => {
                        self.csi_body.clear();
                        self.state = ConptyOutputParseState::Csi;
                    }
                    b']' => self.state = ConptyOutputParseState::Osc,
                    0x1b => self.state = ConptyOutputParseState::Escape,
                    _ => self.state = ConptyOutputParseState::Ground,
                },
                ConptyOutputParseState::Csi => {
                    if (0x40..=0x7e).contains(&byte) {
                        if let Some(response) =
                            conpty_synthetic_response_for_csi(&self.csi_body, byte)
                        {
                            inspection.synthetic_responses.push(response);
                        }
                        self.csi_body.clear();
                        self.state = ConptyOutputParseState::Ground;
                    } else if self.csi_body.len() < 64 {
                        self.csi_body.push(byte);
                    }
                }
                ConptyOutputParseState::Osc => match byte {
                    0x07 => self.state = ConptyOutputParseState::Ground,
                    0x1b => self.state = ConptyOutputParseState::OscEscape,
                    _ => {}
                },
                ConptyOutputParseState::OscEscape => match byte {
                    b'\\' => self.state = ConptyOutputParseState::Ground,
                    0x1b => self.state = ConptyOutputParseState::OscEscape,
                    _ => self.state = ConptyOutputParseState::Osc,
                },
            }
        }
        inspection
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ConptyLogCell {
    ch: char,
    width: usize,
}

pub struct ConptyLogSanitizer {
    state: ConptyOutputParseState,
    csi_body: Vec<u8>,
    pending_text: Vec<u8>,
    line: Vec<ConptyLogCell>,
    cursor_col: usize,
    row: usize,
}

impl Default for ConptyLogSanitizer {
    fn default() -> Self {
        Self {
            state: ConptyOutputParseState::Ground,
            csi_body: Vec::new(),
            pending_text: Vec::new(),
            line: Vec::new(),
            cursor_col: 0,
            row: 1,
        }
    }
}

impl ConptyLogSanitizer {
    pub fn with_row(row: usize) -> Self {
        Self {
            row,
            ..Default::default()
        }
    }

    pub fn push(&mut self, bytes: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        for &byte in bytes {
            match self.state {
                ConptyOutputParseState::Ground => match byte {
                    0x1b => {
                        self.flush_pending_text(&mut out);
                        self.state = ConptyOutputParseState::Escape;
                    }
                    0x9b => {
                        self.flush_pending_text(&mut out);
                        self.csi_body.clear();
                        self.state = ConptyOutputParseState::Csi;
                    }
                    0x9d => {
                        self.flush_pending_text(&mut out);
                        self.state = ConptyOutputParseState::Osc;
                    }
                    b'\n' => {
                        self.flush_pending_text(&mut out);
                        self.flush_line(&mut out);
                    }
                    b'\r' => {
                        self.flush_pending_text(&mut out);
                        self.cursor_col = 0;
                    }
                    b'\t' => {
                        self.flush_pending_text(&mut out);
                        let next_tab = ((self.cursor_col / 8) + 1) * 8;
                        while self.cursor_col < next_tab {
                            self.write_char(' ');
                        }
                    }
                    0x07 | 0x00..=0x08 | 0x0b..=0x1f | 0x7f => {
                        self.flush_pending_text(&mut out);
                    }
                    _ => self.pending_text.push(byte),
                },
                ConptyOutputParseState::Escape => match byte {
                    b'[' => {
                        self.csi_body.clear();
                        self.state = ConptyOutputParseState::Csi;
                    }
                    b']' => self.state = ConptyOutputParseState::Osc,
                    0x1b => self.state = ConptyOutputParseState::Escape,
                    _ => self.state = ConptyOutputParseState::Ground,
                },
                ConptyOutputParseState::Csi => {
                    if (0x40..=0x7e).contains(&byte) {
                        self.apply_csi(byte, &mut out);
                        self.csi_body.clear();
                        self.state = ConptyOutputParseState::Ground;
                    } else if self.csi_body.len() < 64 {
                        self.csi_body.push(byte);
                    }
                }
                ConptyOutputParseState::Osc => match byte {
                    0x07 => self.state = ConptyOutputParseState::Ground,
                    0x1b => self.state = ConptyOutputParseState::OscEscape,
                    _ => {}
                },
                ConptyOutputParseState::OscEscape => match byte {
                    b'\\' => self.state = ConptyOutputParseState::Ground,
                    0x1b => self.state = ConptyOutputParseState::OscEscape,
                    _ => self.state = ConptyOutputParseState::Osc,
                },
            }
        }
        self.flush_pending_text(&mut out);
        out
    }

    pub fn finish(&mut self) -> Vec<u8> {
        let mut out = Vec::new();
        self.flush_pending_text(&mut out);
        if !self.line.is_empty() {
            self.flush_line(&mut out);
        }
        out
    }

    fn apply_csi(&mut self, final_byte: u8, out: &mut Vec<u8>) {
        match final_byte {
            b'C' => {
                let count = csi_params(&self.csi_body).first().copied().unwrap_or(1);
                self.cursor_col = self.cursor_col.saturating_add(count.max(1));
            }
            b'D' => {
                let count = csi_params(&self.csi_body).first().copied().unwrap_or(1);
                self.cursor_col = self.cursor_col.saturating_sub(count.max(1));
            }
            b'G' => {
                let col = csi_params(&self.csi_body).first().copied().unwrap_or(1);
                self.cursor_col = col.saturating_sub(1);
            }
            b'H' | b'f' => {
                let params = csi_params(&self.csi_body);
                let row = params.first().copied().unwrap_or(1).max(1);
                let col = params.get(1).copied().unwrap_or(1).max(1);
                if row > self.row && !self.line.is_empty() {
                    self.flush_line(out);
                }
                self.row = row;
                self.cursor_col = col - 1;
            }
            b'K' => {
                let mode = csi_params(&self.csi_body).first().copied().unwrap_or(0);
                match mode {
                    0 => self.truncate_line_to_cursor(),
                    1 => {
                        let suffix = self.split_line_at_cursor();
                        self.line = suffix;
                        self.cursor_col = 0;
                    }
                    2 => {
                        self.line.clear();
                        self.cursor_col = 0;
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn flush_pending_text(&mut self, _out: &mut Vec<u8>) {
        if self.pending_text.is_empty() {
            return;
        }
        let text = String::from_utf8_lossy(&self.pending_text).into_owned();
        self.pending_text.clear();
        for ch in text.chars() {
            self.write_char(ch);
        }
    }

    fn write_char(&mut self, ch: char) {
        let width = display_width(ch);
        if width == 0 {
            return;
        }
        self.pad_to_cursor();
        self.truncate_line_to_cursor();
        self.line.push(ConptyLogCell { ch, width });
        self.cursor_col = self.cursor_col.saturating_add(width);
    }

    fn pad_to_cursor(&mut self) {
        let current_width = self.line_width();
        if self.cursor_col <= current_width {
            return;
        }
        for _ in 0..(self.cursor_col - current_width) {
            self.line.push(ConptyLogCell { ch: ' ', width: 1 });
        }
    }

    fn truncate_line_to_cursor(&mut self) {
        let keep = self.index_for_col(self.cursor_col);
        self.line.truncate(keep);
    }

    fn split_line_at_cursor(&self) -> Vec<ConptyLogCell> {
        let start = self.index_for_col(self.cursor_col);
        self.line[start..].to_vec()
    }

    fn index_for_col(&self, col: usize) -> usize {
        let mut width = 0;
        for (index, cell) in self.line.iter().enumerate() {
            if width >= col || width.saturating_add(cell.width) > col {
                return index;
            }
            width += cell.width;
        }
        self.line.len()
    }

    fn line_width(&self) -> usize {
        self.line.iter().map(|cell| cell.width).sum()
    }

    fn flush_line(&mut self, out: &mut Vec<u8>) {
        for cell in &self.line {
            let mut encoded = [0_u8; 4];
            out.extend_from_slice(cell.ch.encode_utf8(&mut encoded).as_bytes());
        }
        out.push(b'\n');
        self.line.clear();
        self.cursor_col = 0;
        self.row = self.row.saturating_add(1);
    }
}

fn csi_params(body: &[u8]) -> Vec<usize> {
    let raw = String::from_utf8_lossy(body);
    raw.trim_start_matches('?')
        .split(';')
        .filter_map(|part| part.parse::<usize>().ok())
        .collect()
}

fn display_width(ch: char) -> usize {
    if ch == '\u{0}' || ch.is_control() {
        0
    } else if is_wide_char(ch) {
        2
    } else {
        1
    }
}

fn is_wide_char(ch: char) -> bool {
    matches!(
        ch as u32,
        0x1100..=0x115f
            | 0x2329..=0x232a
            | 0x2e80..=0xa4cf
            | 0xac00..=0xd7a3
            | 0xf900..=0xfaff
            | 0xfe10..=0xfe19
            | 0xfe30..=0xfe6f
            | 0xff00..=0xff60
            | 0xffe0..=0xffe6
            | 0x1f300..=0x1f64f
            | 0x1f900..=0x1f9ff
            | 0x20000..=0x3fffd
    )
}

fn conpty_synthetic_response_for_csi(
    body: &[u8],
    final_byte: u8,
) -> Option<ConptySyntheticResponse> {
    match (body, final_byte) {
        (b"6", b'n') => Some(ConptySyntheticResponse::CursorPosition),
        (b"5", b'n') => Some(ConptySyntheticResponse::DeviceStatusOk),
        _ => None,
    }
}

fn tee_conpty_output(
    reader: &mut Box<dyn Read + Send>,
    mut log_file: std::fs::File,
    first_output_received: Arc<AtomicBool>,
    event_tx: &mpsc::Sender<ConptyLoopMessage>,
    input_command_tx: &mpsc::Sender<ConptyInputCommand>,
    options: ConptyRunOptions,
) -> io::Result<u64> {
    conpty_debug(options.debug, "output reader started");
    let mut stdout = io::stdout();
    let mut buffer = [0_u8; 8192];
    let mut total_bytes = 0_u64;
    let mut inspector = ConptyOutputInspector::default();
    let mut log_sanitizer = ConptyLogSanitizer::default();
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => {
                total_bytes += n as u64;
                let inspection = inspector.inspect(&buffer[..n]);
                for response in inspection.synthetic_responses {
                    conpty_debug(
                        options.debug,
                        format_args!("terminal query detected: {}", response.debug_label()),
                    );
                    let _ = input_command_tx.send(ConptyInputCommand::WriteSynthetic { response });
                }
                if inspection.has_visible_output
                    && !first_output_received.swap(true, Ordering::SeqCst)
                {
                    send_conpty_event(event_tx, ConptyEvent::FirstOutput { bytes: n });
                } else if !inspection.has_visible_output
                    && !first_output_received.load(Ordering::SeqCst)
                {
                    conpty_debug(
                        options.debug,
                        format_args!("startup control output ignored: {n} bytes"),
                    );
                }
                send_conpty_event(event_tx, ConptyEvent::OutputChunk { bytes: n });
                let sanitized = log_sanitizer.push(&buffer[..n]);
                if !sanitized.is_empty() {
                    log_file.write_all(&sanitized)?;
                    log_file.flush()?;
                }
                stdout.write_all(&buffer[..n])?;
                stdout.flush()?;
            }
            Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
            Err(err)
                if matches!(
                    err.kind(),
                    io::ErrorKind::BrokenPipe
                        | io::ErrorKind::ConnectionReset
                        | io::ErrorKind::UnexpectedEof
                ) =>
            {
                break
            }
            Err(err) => return Err(err),
        }
    }
    let sanitized = log_sanitizer.finish();
    if !sanitized.is_empty() {
        log_file.write_all(&sanitized)?;
        log_file.flush()?;
    }
    conpty_debug(
        options.debug,
        format_args!("output reader ended: {total_bytes} bytes"),
    );
    Ok(total_bytes)
}

fn current_pty_size() -> PtySize {
    let (cols, rows) = crossterm::terminal::size().unwrap_or((80, 24));
    pty_size(cols, rows)
}

pub fn pty_size(cols: u16, rows: u16) -> PtySize {
    PtySize {
        rows: rows.max(1),
        cols: cols.max(1),
        pixel_width: 0,
        pixel_height: 0,
    }
}

pub fn conpty_exit_code(status: &portable_pty::ExitStatus) -> i32 {
    i32::try_from(status.exit_code()).unwrap_or(i32::MAX)
}

pub fn key_event_to_pty_bytes(key: KeyEvent) -> Option<Vec<u8>> {
    if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
        return None;
    }
    match key.code {
        KeyCode::Char(ch) if key.modifiers.contains(KeyModifiers::CONTROL) => {
            ctrl_char_to_byte(ch).map(|byte| vec![byte])
        }
        KeyCode::Char(ch) => {
            let mut encoded = [0_u8; 4];
            Some(ch.encode_utf8(&mut encoded).as_bytes().to_vec())
        }
        KeyCode::Enter => Some(b"\r".to_vec()),
        KeyCode::Backspace => Some(vec![0x7f]),
        KeyCode::Tab => Some(b"\t".to_vec()),
        KeyCode::Esc => Some(vec![0x1b]),
        KeyCode::Left => Some(b"\x1b[D".to_vec()),
        KeyCode::Right => Some(b"\x1b[C".to_vec()),
        KeyCode::Up => Some(b"\x1b[A".to_vec()),
        KeyCode::Down => Some(b"\x1b[B".to_vec()),
        KeyCode::Home => Some(b"\x1b[H".to_vec()),
        KeyCode::End => Some(b"\x1b[F".to_vec()),
        KeyCode::PageUp => Some(b"\x1b[5~".to_vec()),
        KeyCode::PageDown => Some(b"\x1b[6~".to_vec()),
        KeyCode::Delete => Some(b"\x1b[3~".to_vec()),
        KeyCode::Insert => Some(b"\x1b[2~".to_vec()),
        _ => None,
    }
}

fn key_event_is_ctrl_c(key: KeyEvent) -> bool {
    matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat)
        && key.code == KeyCode::Char('c')
        && key.modifiers.contains(KeyModifiers::CONTROL)
}

fn ctrl_char_to_byte(ch: char) -> Option<u8> {
    let upper = ch.to_ascii_uppercase();
    if upper.is_ascii_uppercase() {
        Some((upper as u8) - b'A' + 1)
    } else if ch == ' ' {
        Some(0)
    } else {
        None
    }
}
