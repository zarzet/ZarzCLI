use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use tokio::sync::{broadcast, mpsc, Mutex};
use tokio::task::JoinHandle;
use tokio::time::{sleep, Instant};

#[derive(Clone, Debug)]
pub struct ExecCommandRequest {
    pub command: String,
    pub shell: String,
    pub login: bool,
    pub yield_time_ms: Option<u64>,
}

#[derive(Clone, Debug)]
pub struct WriteStdinRequest {
    pub session_id: i32,
    pub input: String,
    pub yield_time_ms: Option<u64>,
}

#[derive(Debug)]
pub struct UnifiedExecResponse {
    pub output: String,
    pub session_id: Option<i32>,
    pub exit_code: Option<i32>,
    pub wall_time: Duration,
}

impl UnifiedExecResponse {
    pub fn format_for_display(&self) -> String {
        let mut sections = Vec::new();
        sections.push(format!("Wall time: {:.3} seconds", self.wall_time.as_secs_f64()));
        if let Some(code) = self.exit_code {
            sections.push(format!("Exit code: {}", code));
        }
        if let Some(id) = self.session_id {
            sections.push(format!("Session ID: {} (still running)", id));
        }
        sections.push("Output:".to_string());
        sections.push(if self.output.is_empty() {
            "(no output)".to_string()
        } else {
            self.output.clone()
        });
        sections.join("\n")
    }
}

struct ExecSession {
    stdin_tx: mpsc::Sender<Vec<u8>>,
    output_rx: broadcast::Receiver<Vec<u8>>,
    exit_code: Arc<Mutex<Option<i32>>>,
}

pub struct UnifiedExecManager {
    sessions: Arc<Mutex<HashMap<i32, Arc<ExecSession>>>>,
    next_id: AtomicI32,
}

impl UnifiedExecManager {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            next_id: AtomicI32::new(1),
        })
    }

    pub async fn exec_command(
        self: &Arc<Self>,
        request: ExecCommandRequest,
    ) -> Result<UnifiedExecResponse> {
        let start = Instant::now();
        let session_id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (session, reader_handle, writer_handle, wait_handle) =
            spawn_pty_process(session_id, &request, self.sessions.clone()).await?;

        let output = collect_initial_output(&session, request.yield_time_ms.unwrap_or(250)).await;
        let exit_code = session.exit_code.lock().await.clone();
        let wall_time = start.elapsed();
        let session_alive = exit_code.is_none();

        let session_arc = Arc::new(session);
        self.sessions
            .lock()
            .await
            .insert(session_id, session_arc);

        tokio::spawn(reader_handle);
        tokio::spawn(writer_handle);
        tokio::spawn(wait_handle);

        Ok(UnifiedExecResponse {
            output,
            session_id: if session_alive { Some(session_id) } else { None },
            exit_code,
            wall_time,
        })
    }

    pub async fn write_stdin(
        self: &Arc<Self>,
        request: WriteStdinRequest,
    ) -> Result<UnifiedExecResponse> {
        let start = Instant::now();
        let session = {
            let map = self.sessions.lock().await;
            map.get(&request.session_id).cloned()
        }
        .ok_or_else(|| anyhow!("Session {} not found", request.session_id))?;

        session
            .stdin_tx
            .send(request.input.into_bytes())
            .await
            .context("Failed to write to session stdin")?;

        let output = collect_initial_output(&session, request.yield_time_ms.unwrap_or(250)).await;
        let exit_code = session.exit_code.lock().await.clone();
        let wall_time = start.elapsed();

        if exit_code.is_some() {
            self.sessions.lock().await.remove(&request.session_id);
        }

        Ok(UnifiedExecResponse {
            output,
            session_id: if exit_code.is_none() {
                Some(request.session_id)
            } else {
                None
            },
            exit_code,
            wall_time,
        })
    }
}

async fn spawn_pty_process(
    session_id: i32,
    request: &ExecCommandRequest,
    registry: Arc<Mutex<HashMap<i32, Arc<ExecSession>>>>,
) -> Result<(ExecSession, JoinHandle<()>, JoinHandle<()>, JoinHandle<()>)> {
    let command = build_command(request);
    let pty_system = native_pty_system();
    let pair = pty_system.openpty(PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    })?;

    let mut builder = command;
    builder.cwd(&PathBuf::from("."));

    let mut child = pair.slave.spawn_command(builder)?;
    let mut killer = child.clone_killer();

    let (stdin_tx, mut stdin_rx) = mpsc::channel::<Vec<u8>>(128);
    let (output_tx, _) = broadcast::channel::<Vec<u8>>(256);

    let mut reader = pair.master.try_clone_reader()?;
    let output_tx_clone = output_tx.clone();
    let reader_handle = tokio::task::spawn_blocking(move || {
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let chunk = buf[..n].to_vec();
                    let _ = output_tx_clone.send(chunk);
                }
                Err(_) => break,
            }
        }
    });

    let writer = pair.master.take_writer()?;
    let writer = Arc::new(Mutex::new(writer));
    let writer_handle = tokio::spawn(async move {
        while let Some(bytes) = stdin_rx.recv().await {
            let mut guard = writer.lock().await;
            let _ = guard.write_all(&bytes);
            let _ = guard.flush();
        }
    });

    let exit_code = Arc::new(Mutex::new(None));
    let exit_code_wait = exit_code.clone();
    let registry = registry.clone();
    let wait_handle = tokio::task::spawn_blocking(move || {
        let code = match child.wait() {
            Ok(status) => status.exit_code() as i32,
            Err(_) => -1_i32,
        };
        let _ = killer.kill();
        futures::executor::block_on(async {
            let mut guard = exit_code_wait.lock().await;
            *guard = Some(code);
            let mut map = registry.lock().await;
            map.remove(&session_id);
        });
    });

    let session = ExecSession {
        stdin_tx,
        output_rx: output_tx.subscribe(),
        exit_code,
    };

    Ok((session, reader_handle, writer_handle, wait_handle))
}

fn build_command(request: &ExecCommandRequest) -> CommandBuilder {
    let mut builder = CommandBuilder::new(&request.shell);
    if request.login {
        if cfg!(windows) {
            builder.arg("/C");
            builder.arg(&request.command);
        } else {
            builder.arg("-lc");
            builder.arg(&request.command);
        }
    } else {
        builder.arg(&request.command);
    }
    builder
}

async fn collect_initial_output(session: &ExecSession, yield_time_ms: u64) -> String {
    let timeout = Duration::from_millis(yield_time_ms);
    let mut receiver = session.output_rx.resubscribe();

    let mut chunk = String::new();
    let start = Instant::now();
    loop {
        let remaining = timeout
            .checked_sub(start.elapsed())
            .unwrap_or_else(|| Duration::from_millis(0));
        let recv = receiver.recv();
        tokio::select! {
            res = recv => {
                match res {
                    Ok(bytes) => {
                        chunk.push_str(&String::from_utf8_lossy(&bytes));
                        break;
                    }
                    Err(_) => break,
                }
            }
            _ = sleep(remaining) => break,
        }
    }
    chunk
}
