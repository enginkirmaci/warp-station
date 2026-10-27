//! The warp-agent door client. DevTools' AI surface (wands, summary, model
//! catalog) runs in warp-agent over its token-authenticated loopback door —
//! DevTools never spawns an opencode server itself anymore. A missing
//! warp-agent turns the AI features off; a merely closed one is launched on
//! demand and the run continues.

use anyhow::{anyhow, Context as _, Result};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

/// Names warp-agent's built-in General workspace on the wire.
pub const GENERAL: &str = "general";

/// How long a door command may take to answer (run-prompt waits for the
/// turn itself and is exempt on the server side).
const ANSWER_TIMEOUT: Duration = Duration::from_secs(60);
/// How long a launch-and-connect attempt keeps polling the door.
const LAUNCH_DEADLINE: Duration = Duration::from_secs(15);

pub fn port_file() -> PathBuf {
    config_dir().join("ipc.port")
}

pub fn token_file() -> PathBuf {
    config_dir().join("ipc.token")
}

fn config_dir() -> PathBuf {
    crate::paths::home().join(".warp-agent")
}

/// The door endpoint as published by a running warp-agent.
pub fn read_endpoint() -> Option<(u16, String)> {
    let port = std::fs::read_to_string(port_file()).ok()?.trim().parse().ok()?;
    let token = std::fs::read_to_string(token_file()).ok()?.trim().to_string();
    if token.is_empty() {
        return None;
    }
    Some((port, token))
}

/// One authenticated door connection. Line frames: requests out, responses
/// and server pushes in, matched by id.
pub struct Client {
    reader: BufReader<TcpStream>,
    next_id: u64,
}

impl Client {
    pub async fn connect() -> Result<Self> {
        Self::connect_in(&config_dir()).await
    }

    /// Test seam for [`connect`]: the endpoint files live in `dir`.
    pub async fn connect_in(dir: &Path) -> Result<Self> {
        let read = |file: &str| std::fs::read_to_string(dir.join(file));
        let port: u16 = read("ipc.port")
            .ok()
            .and_then(|text| text.trim().parse().ok())
            .ok_or_else(|| anyhow!("warp-agent is not running (no door endpoint)"))?;
        let token = read("ipc.token").ok().unwrap_or_default();
        let token = token.trim().to_string();
        if token.is_empty() {
            return Err(anyhow!("warp-agent is not running (no door token)"));
        }
        let stream = TcpStream::connect(("127.0.0.1", port))
            .await
            .context("warp-agent door connect failed")?;
        stream.set_nodelay(true).ok();
        let mut client = Self { reader: BufReader::new(stream), next_id: 1 };
        client.write(&json!({"auth": token})).await?;
        let welcome = client.read_frame().await?;
        if !welcome.get("ok").and_then(Value::as_bool).unwrap_or(false) {
            return Err(anyhow!("warp-agent door rejected the token"));
        }
        Ok(client)
    }

    /// True when a running warp-agent answered a ping.
    pub async fn probe() -> bool {
        match Self::connect().await {
            Ok(mut client) => client.request("ping", json!({})).await.is_ok(),
            Err(_) => false,
        }
    }

    async fn write(&mut self, frame: &Value) -> Result<()> {
        let raw = self.reader.get_mut();
        raw.write_all(format!("{frame}\n").as_bytes()).await?;
        raw.flush().await?;
        Ok(())
    }

    /// Reads one NDJSON frame; an oversized line drops the connection (the
    /// door promises the same cap on its side).
    async fn read_frame(&mut self) -> Result<Value> {
        let mut limited = (&mut self.reader).take(8 * 1024 * 1024 + 1);
        let mut line = String::new();
        if limited.read_line(&mut line).await? == 0 {
            return Err(anyhow!("warp-agent closed the door"));
        }
        if line.len() > 8 * 1024 * 1024 {
            return Err(anyhow!("oversized frame from warp-agent"));
        }
        Ok(serde_json::from_str(line.trim())?)
    }

    /// Reads the next raw frame (a push event or an answer); the status
    /// watcher rides this until the door drops.
    pub async fn next_frame(&mut self) -> Result<Value> {
        self.read_frame().await
    }

    /// Sends one command and waits for its answer, ignoring pushes.
    pub async fn request(&mut self, cmd: &str, params: Value) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        let mut frame = json!({"id": id, "cmd": cmd});
        if let (Some(target), Some(extra)) = (frame.as_object_mut(), params.as_object()) {
            for (key, value) in extra {
                target.insert(key.clone(), value.clone());
            }
        }
        self.write(&frame).await?;
        let deadline = tokio::time::sleep(ANSWER_TIMEOUT);
        tokio::pin!(deadline);
        loop {
            let answer = tokio::select! {
                _ = &mut deadline => return Err(anyhow!("warp-agent did not answer {cmd} in time")),
                frame = self.read_frame() => frame?,
            };
            if answer.get("id").and_then(Value::as_u64) != Some(id) {
                continue;
            }
            if answer.get("ok").and_then(Value::as_bool).unwrap_or(false) {
                return Ok(answer);
            }
            let error = answer.get("error").and_then(Value::as_str).unwrap_or("unknown error");
            return Err(anyhow!("{error}"));
        }
    }

    pub async fn open_workspace(&mut self, path: &Path) -> Result<()> {
        self.request(
            "open-workspace",
            json!({"path": path.to_string_lossy()}),
        )
        .await
        .map(|_| ())
    }

    pub async fn new_chat(
        &mut self,
        workspace: &str,
        prefill: &str,
        auto_send: bool,
        model: Option<&str>,
    ) -> Result<()> {
        let mut params = json!({"workspace": workspace, "prefill": prefill, "auto_send": auto_send});
        if let Some(model) = model.filter(|m| !m.is_empty()) {
            params["model"] = json!(model);
        }
        self.request("new-chat", params).await.map(|_| ())
    }

    pub async fn models(&mut self) -> Result<Vec<String>> {
        let answer = self.request("models", json!({})).await?;
        Ok(answer
            .get("models")
            .and_then(Value::as_array)
            .map(|list| list.iter().filter_map(|m| m.as_str().map(str::to_string)).collect())
            .unwrap_or_default())
    }

    pub async fn status(&mut self) -> Result<Value> {
        self.request("status", json!({})).await.map(|answer| {
            answer.get("workspaces").cloned().unwrap_or(Value::Array(Vec::new()))
        })
    }

    /// Runs one prompt in a fresh warp-agent chat and returns the final
    /// assistant text. `on_log` receives the run's progress lines; a signal
    /// on `cancel` aborts the run server-side and settles as an error.
    pub async fn run_prompt(
        &mut self,
        workspace: &str,
        prompt: &str,
        model: Option<&str>,
        mut on_log: impl FnMut(String),
        cancel: async_channel::Receiver<()>,
    ) -> Result<String> {
        let id = self.next_id;
        self.next_id += 1;
        let mut params = json!({"id": id, "cmd": "run-prompt", "workspace": workspace, "prompt": prompt});
        if let Some(model) = model.filter(|m| !m.is_empty()) {
            params["model"] = json!(model);
        }
        self.write(&params).await?;

        let deadline = tokio::time::sleep(crate::opencode::RUN_CAP);
        tokio::pin!(deadline);
        let mut cancelled = false;
        loop {
            let answer = tokio::select! {
                biased;
                _ = &mut deadline, if !cancelled => {
                    let _ = self.request("cancel", json!({"run": id})).await;
                    return Err(anyhow!("the wand run exceeded its time cap"));
                }
                stopped = cancel.recv(), if !cancelled => {
                    if stopped.is_ok() {
                        cancelled = true;
                        let cancel_frame = json!({"id": self.next_id, "cmd": "cancel", "run": id});
                        self.next_id += 1;
                        self.write(&cancel_frame).await?;
                    }
                    continue;
                }
                frame = self.read_frame() => frame?,
            };
            if answer.get("event").and_then(Value::as_str) == Some("run-log")
                && answer.get("run").and_then(Value::as_u64) == Some(id)
                && let Some(line) = answer.get("line").and_then(Value::as_str) {
                    on_log(line.to_string());
                    continue;
            }
            if answer.get("id").and_then(Value::as_u64) != Some(id) {
                continue;
            }
            if answer.get("ok").and_then(Value::as_bool).unwrap_or(false) {
                let text = answer.get("text").and_then(Value::as_str).unwrap_or_default();
                let text = text.trim().to_string();
                if text.is_empty() {
                    return Err(anyhow!("the model returned no text"));
                }
                return Ok(text);
            }
            let error = answer.get("error").and_then(Value::as_str).unwrap_or("unknown error");
            return Err(anyhow!("{error}"));
        }
    }
}

/// Connects to the door, launching warp-agent when it is merely closed. The
/// launch opens the binary with `folder` so the workspace is right there.
/// Err means: not installed, or it never came up.
pub async fn connect_or_launch(exe: &Path, folder: Option<&Path>) -> Result<Client> {
    if let Ok(client) = Client::connect().await {
        return Ok(client);
    }
    let mut command = std::process::Command::new(exe);
    if let Some(folder) = folder {
        command.arg(folder);
    }
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    hide_console(&mut command);
    command.spawn().with_context(|| format!("'{}' failed to start", exe.display()))?;

    let deadline = std::time::Instant::now() + LAUNCH_DEADLINE;
    loop {
        if std::time::Instant::now() >= deadline {
            return Err(anyhow!("warp-agent did not open its door within 15 s"));
        }
        if let Ok(client) = Client::connect().await {
            return Ok(client);
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
}

fn hide_console(command: &mut std::process::Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    {
        let _ = command;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    /// A door-speaking mock: auth, then canned command answers.
    async fn mock_door(answers: Vec<(&'static str, Value)>) -> (u16, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (read_half, mut write_half) = stream.into_split();
            let mut reader = BufReader::new(read_half);
            let mut line = String::new();
            reader.read_line(&mut line).await.unwrap();
            write_half.write_all(format!("{}\n", json!({"ok": true})).as_bytes()).await.unwrap();
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).await.unwrap() == 0 {
                    return;
                }
                let request: Value = serde_json::from_str(line.trim()).unwrap();
                let cmd = request.get("cmd").and_then(Value::as_str).unwrap_or_default();
                let id = request.get("id").and_then(Value::as_u64).unwrap_or_default();
                let Some((_, answer)) = answers.iter().find(|(name, _)| *name == cmd) else {
                    continue;
                };
                let mut frame = answer.clone();
                frame["id"] = json!(id);
                frame["ok"] = json!(true);
                write_half
                    .write_all(format!("{frame}\n").as_bytes())
                    .await
                    .unwrap();
            }
        });
        (port, handle)
    }

    #[tokio::test]
    async fn client_roundtrips_commands() {
        let (port, door) = mock_door(vec![
            ("ping", json!({})),
            ("models", json!({"models": ["a/x", "b/y"]})),
        ])
        .await;
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("ipc.token"), "test-token\n").unwrap();
        std::fs::write(dir.path().join("ipc.port"), format!("{port}\n")).unwrap();
        let mut client = Client::connect_in(dir.path()).await.unwrap();
        assert!(client.request("ping", json!({})).await.is_ok());
        let models = client.models().await.unwrap();
        assert_eq!(models, vec!["a/x".to_string(), "b/y".to_string()]);
        let _ = client.reader.get_mut().shutdown().await;
        let _ = door.await;
    }
}
