use anyhow::{Context, Result};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

pub struct CommandExecutor;

#[allow(dead_code)]
#[derive(Debug)]
pub struct CommandResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub success: bool,
}

impl CommandExecutor {
    #[allow(dead_code)]
    pub async fn execute(command: &str) -> Result<CommandResult> {
        let (shell, flag) = if cfg!(target_os = "windows") {
            ("cmd", "/C")
        } else {
            ("sh", "-c")
        };

        let mut child = Command::new(shell)
            .arg(flag)
            .arg(command)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("Failed to execute command: {}", command))?;

        let stdout = child
            .stdout
            .take()
            .context("Failed to capture stdout")?;

        let stderr = child
            .stderr
            .take()
            .context("Failed to capture stderr")?;

        let mut stdout_lines = BufReader::new(stdout).lines();
        let mut stderr_lines = BufReader::new(stderr).lines();

        let stdout_handle = tokio::spawn(async move {
            let mut output = String::new();
            while let Ok(Some(line)) = stdout_lines.next_line().await {
                output.push_str(&line);
                output.push('\n');
            }
            output
        });

        let stderr_handle = tokio::spawn(async move {
            let mut output = String::new();
            while let Ok(Some(line)) = stderr_lines.next_line().await {
                output.push_str(&line);
                output.push('\n');
            }
            output
        });

        let stdout_output = stdout_handle
            .await
            .context("Failed to join stdout task")?;

        let stderr_output = stderr_handle
            .await
            .context("Failed to join stderr task")?;

        let status = child
            .wait()
            .await
            .context("Failed to wait for command")?;

        let exit_code = status.code().unwrap_or(-1);
        let success = status.success();

        Ok(CommandResult {
            stdout: stdout_output,
            stderr: stderr_output,
            exit_code,
            success,
        })
    }

    #[allow(dead_code)]
    pub async fn execute_streaming<F>(command: &str, mut on_output: F) -> Result<CommandResult>
    where
        F: FnMut(&str) + Send,
    {
        let (shell, flag) = if cfg!(target_os = "windows") {
            ("cmd", "/C")
        } else {
            ("sh", "-c")
        };

        let mut child = Command::new(shell)
            .arg(flag)
            .arg(command)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("Failed to execute command: {}", command))?;

        let stdout = child
            .stdout
            .take()
            .context("Failed to capture stdout")?;

        let stderr = child
            .stderr
            .take()
            .context("Failed to capture stderr")?;

        let mut stdout_lines = BufReader::new(stdout).lines();
        let mut stderr_lines = BufReader::new(stderr).lines();

        let mut stdout_output = String::new();
        let mut stderr_output = String::new();

        loop {
            tokio::select! {
                result = stdout_lines.next_line() => {
                    match result {
                        Ok(Some(line)) => {
                            on_output(&line);
                            stdout_output.push_str(&line);
                            stdout_output.push('\n');
                        }
                        Ok(None) => break,
                        Err(_) => break,
                    }
                }
                result = stderr_lines.next_line() => {
                    match result {
                        Ok(Some(line)) => {
                            on_output(&line);
                            stderr_output.push_str(&line);
                            stderr_output.push('\n');
                        }
                        Ok(None) => {},
                        Err(_) => {},
                    }
                }
            }
        }

        let status = child
            .wait()
            .await
            .context("Failed to wait for command")?;

        let exit_code = status.code().unwrap_or(-1);
        let success = status.success();

        Ok(CommandResult {
            stdout: stdout_output,
            stderr: stderr_output,
            exit_code,
            success,
        })
    }
}
