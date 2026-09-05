//! Process supervision: spawn/stop/restart child services, track status and
//! capture logs.
//!
//! Stop semantics (unix): the child runs in its own process group, stop sends
//! SIGTERM to the whole group, then SIGKILL after a grace period. On other
//! platforms the child is killed directly.

use crate::config::{self, ConfigFile, ServiceConfig, ServiceInput};
use crate::logs::{LogStore, LogStream, now_millis};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, RwLock};
use tokio_util::sync::CancellationToken;

const LOG_BUFFER_LINES: usize = 5000;
const RESTART_DELAY: Duration = Duration::from_secs(2);
const GRACEFUL_TERM: Duration = Duration::from_secs(5);
pub const STOP_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Stopped,
    Starting,
    Running,
    Stopping,
    Backoff,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
pub struct ServiceInfo {
    pub id: String,
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub cwd: String,
    pub env: BTreeMap<String, String>,
    pub autostart: bool,
    pub auto_restart: bool,
    pub status: Status,
    pub pid: Option<u32>,
    /// Unix epoch milliseconds, set while running.
    pub started_at: Option<i64>,
    pub exit_code: Option<i32>,
    pub last_error: Option<String>,
}

/// One managed service: configuration + runtime state + log buffer.
pub struct ManagedService {
    pub cfg: RwLock<ServiceConfig>,
    status: RwLock<Status>,
    pid: RwLock<Option<u32>>,
    started_at: RwLock<Option<i64>>,
    exit_code: RwLock<Option<i32>>,
    last_error: RwLock<Option<String>>,
    pub logs: LogStore,
    /// Serializes start/stop/restart control operations.
    control: Mutex<()>,
    /// Bumped on every start/stop/delete; lets stale watcher tasks detect that
    /// the world moved on.
    generation: AtomicU64,
    run_token: Mutex<Option<(u64, CancellationToken)>>,
}

impl ManagedService {
    fn new(cfg: ServiceConfig) -> Arc<Self> {
        Arc::new(Self {
            cfg: RwLock::new(cfg),
            status: RwLock::new(Status::Stopped),
            pid: RwLock::new(None),
            started_at: RwLock::new(None),
            exit_code: RwLock::new(None),
            last_error: RwLock::new(None),
            logs: LogStore::new(LOG_BUFFER_LINES),
            control: Mutex::new(()),
            generation: AtomicU64::new(0),
            run_token: Mutex::new(None),
        })
    }

    pub async fn info(&self) -> ServiceInfo {
        let cfg = self.cfg.read().await.clone();
        ServiceInfo {
            id: cfg.id,
            name: cfg.name,
            command: cfg.command,
            args: cfg.args,
            cwd: cfg.cwd,
            env: cfg.env,
            autostart: cfg.autostart,
            auto_restart: cfg.auto_restart,
            status: *self.status.read().await,
            pid: *self.pid.read().await,
            started_at: *self.started_at.read().await,
            exit_code: *self.exit_code.read().await,
            last_error: self.last_error.read().await.clone(),
        }
    }

    pub async fn status(&self) -> Status {
        *self.status.read().await
    }

    pub async fn is_busy(&self) -> bool {
        matches!(
            self.status().await,
            Status::Running | Status::Starting | Status::Stopping | Status::Backoff
        )
    }

    async fn set_status(&self, status: Status) {
        *self.status.write().await = status;
    }

    fn current_gen(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }

    async fn clear_run_token(&self, run_gen: u64) {
        let mut token = self.run_token.lock().await;
        if token.as_ref().map(|(g, _)| *g) == Some(run_gen) {
            *token = None;
        }
    }

    /// Start the service. Fails if it is already running or being controlled.
    pub async fn start(self: &Arc<Self>) -> Result<(), String> {
        let _guard = self.control.lock().await;
        match self.status().await {
            Status::Running | Status::Starting => return Err("服务已在运行".into()),
            Status::Stopping => return Err("服务正在停止，请稍候".into()),
            Status::Backoff => return Err("服务正在等待自动重启，请先停止".into()),
            Status::Stopped | Status::Failed => {}
        }
        let cfg = self.cfg.read().await.clone();

        let run_gen = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        self.set_status(Status::Starting).await;
        let mut child = match spawn_child(&cfg) {
            Ok(c) => c,
            Err(e) => {
                *self.last_error.write().await = Some(e.to_string());
                self.set_status(Status::Failed).await;
                self.logs
                    .push(LogStream::System, format!("启动失败: {e}"))
                    .await;
                return Err(format!("启动失败: {e}"));
            }
        };

        let pid = child.id();
        attach_readers(self.clone(), &mut child);
        let token = CancellationToken::new();
        *self.run_token.lock().await = Some((run_gen, token.clone()));
        *self.pid.write().await = pid;
        *self.started_at.write().await = Some(now_millis());
        *self.exit_code.write().await = None;
        *self.last_error.write().await = None;
        self.set_status(Status::Running).await;
        self.logs
            .push(
                LogStream::System,
                format!(
                    "已启动 (pid {}): {}",
                    pid.unwrap_or(0),
                    display_command(&cfg)
                ),
            )
            .await;
        tracing::info!(service = %cfg.name, pid = pid.unwrap_or(0), "service started");

        tokio::spawn(watch_run(self.clone(), child, token, run_gen));
        Ok(())
    }

    /// Stop the service if running and wait for it to settle.
    pub async fn stop(self: &Arc<Self>) -> Result<(), String> {
        let _guard = self.control.lock().await;
        match self.status().await {
            Status::Stopped | Status::Failed => return Ok(()),
            _ => {}
        }
        // Invalidate any pending auto-restart.
        self.generation.fetch_add(1, Ordering::SeqCst);

        let token = self.run_token.lock().await.clone();
        match token {
            Some((_, t)) => {
                t.cancel();
            }
            None => {
                self.set_status(Status::Stopped).await;
                return Ok(());
            }
        }

        let deadline = tokio::time::Instant::now() + STOP_TIMEOUT;
        loop {
            match self.status().await {
                Status::Stopped | Status::Failed => return Ok(()),
                _ => {}
            }
            if tokio::time::Instant::now() >= deadline {
                return Err("停止超时，请重试".into());
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    pub async fn restart(self: &Arc<Self>) -> Result<(), String> {
        self.stop().await?;
        self.start().await
    }
}

fn spawn_child(cfg: &ServiceConfig) -> std::io::Result<Child> {
    let mut cmd = std::process::Command::new(&cfg.command);
    cmd.args(&cfg.args)
        .envs(&cfg.env)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    if !cfg.cwd.is_empty() {
        cmd.current_dir(&cfg.cwd);
    }
    // Run the service in its own process group so we can signal the whole
    // tree (shell wrappers, worker children, ...) on stop.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }

    let mut cmd = Command::from(cmd);
    // Safety net: never leak a process if the watcher task is dropped.
    cmd.kill_on_drop(true);
    cmd.spawn()
}

fn attach_readers(svc: Arc<ManagedService>, child: &mut Child) {
    if let Some(out) = child.stdout.take() {
        spawn_reader(svc.clone(), LogStream::Stdout, out);
    }
    if let Some(err) = child.stderr.take() {
        spawn_reader(svc, LogStream::Stderr, err);
    }
}

fn spawn_reader(
    svc: Arc<ManagedService>,
    stream: LogStream,
    reader: impl tokio::io::AsyncRead + Unpin + Send + 'static,
) {
    tokio::spawn(async move {
        use tokio::io::AsyncBufReadExt;
        let mut lines = tokio::io::BufReader::new(reader).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            svc.logs.push(stream, line).await;
        }
    });
}

/// Watch a run to completion: handles natural exits, user stops and
/// auto-restart policy. Respawns in-place (without the control lock) so a stop
/// request always wins races against the restart loop.
async fn watch_run(
    svc: Arc<ManagedService>,
    mut child: Child,
    token: CancellationToken,
    run_gen: u64,
) {
    loop {
        let exit_status = tokio::select! {
            res = child.wait() => res,
            _ = token.cancelled() => {
                svc.set_status(Status::Stopping).await;
                svc.logs.push(LogStream::System, "正在停止…").await;
                graceful_stop(&mut child).await
            }
        };
        let code = exit_status.ok().and_then(|s| s.code());
        *svc.pid.write().await = None;
        *svc.exit_code.write().await = code;

        if token.is_cancelled() {
            svc.logs
                .push(LogStream::System, format!("已停止{}", fmt_exit(code)))
                .await;
            svc.set_status(Status::Stopped).await;
            svc.clear_run_token(run_gen).await;
            let name = svc.cfg.read().await.name.clone();
            tracing::info!(service = %name, "service stopped");
            return;
        }

        let success = code == Some(0);
        svc.logs
            .push(LogStream::System, format!("进程退出{}", fmt_exit(code)))
            .await;

        let cfg = svc.cfg.read().await.clone();
        let should_restart = !success && cfg.auto_restart && svc.current_gen() == run_gen;
        if !should_restart {
            svc.set_status(if success {
                Status::Stopped
            } else {
                Status::Failed
            })
            .await;
            svc.clear_run_token(run_gen).await;
            return;
        }

        svc.set_status(Status::Backoff).await;
        svc.logs
            .push(
                LogStream::System,
                format!("非零退出，{} 秒后自动重启", RESTART_DELAY.as_secs()),
            )
            .await;
        tokio::select! {
            _ = tokio::time::sleep(RESTART_DELAY) => {}
            _ = token.cancelled() => {
                svc.set_status(Status::Stopped).await;
                svc.clear_run_token(run_gen).await;
                return;
            }
        }
        if svc.current_gen() != run_gen || token.is_cancelled() {
            svc.set_status(Status::Stopped).await;
            svc.clear_run_token(run_gen).await;
            return;
        }

        match spawn_child(&cfg) {
            Ok(mut next) => {
                let pid = next.id();
                attach_readers(svc.clone(), &mut next);
                *svc.pid.write().await = pid;
                *svc.started_at.write().await = Some(now_millis());
                *svc.exit_code.write().await = None;
                svc.set_status(Status::Running).await;
                svc.logs
                    .push(
                        LogStream::System,
                        format!("已自动重启 (pid {})", pid.unwrap_or(0)),
                    )
                    .await;
                child = next;
            }
            Err(e) => {
                *svc.last_error.write().await = Some(e.to_string());
                svc.logs
                    .push(LogStream::System, format!("自动重启失败: {e}"))
                    .await;
                svc.set_status(Status::Failed).await;
                svc.clear_run_token(run_gen).await;
                return;
            }
        }
    }
}

/// Gracefully terminate: SIGTERM the process group, wait, then SIGKILL.
/// Returns the final exit status exactly once.
async fn graceful_stop(child: &mut Child) -> std::io::Result<std::process::ExitStatus> {
    #[cfg(unix)]
    {
        if let Some(pid) = child.id() {
            let pgid = -(pid as libc::pid_t);
            unsafe {
                libc::kill(pgid, libc::SIGTERM);
            }
        }
        match tokio::time::timeout(GRACEFUL_TERM, child.wait()).await {
            // wait() is cancel-safe: on timeout nothing was reaped yet.
            Ok(res) => res,
            Err(_elapsed) => {
                if let Some(pid) = child.id() {
                    let pgid = -(pid as libc::pid_t);
                    unsafe {
                        libc::kill(pgid, libc::SIGKILL);
                    }
                }
                child.wait().await
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = child.start_kill();
        child.wait().await
    }
}

fn fmt_exit(code: Option<i32>) -> String {
    match code {
        Some(c) => format!(" (exit={c})"),
        None => " (被信号终止)".to_string(),
    }
}

fn normalize_input(input: ServiceInput) -> ServiceConfig {
    ServiceConfig {
        id: String::new(),
        name: input.name.trim().to_string(),
        command: input.command.trim().to_string(),
        args: input.args,
        cwd: input.cwd.trim().to_string(),
        env: input
            .env
            .into_iter()
            .filter(|(k, _)| !k.trim().is_empty())
            .map(|(k, v)| (k.trim().to_string(), v))
            .collect(),
        autostart: input.autostart,
        auto_restart: input.auto_restart,
    }
}

fn display_command(cfg: &ServiceConfig) -> String {
    if cfg.args.is_empty() {
        cfg.command.clone()
    } else {
        format!("{} {}", cfg.command, cfg.args.join(" "))
    }
}

/// Registry of managed services, persisted to the config file.
pub struct Supervisor {
    services: Mutex<HashMap<String, Arc<ManagedService>>>,
    config_path: PathBuf,
}

impl Supervisor {
    pub async fn load(path: &std::path::Path) -> anyhow::Result<Arc<Self>> {
        let file = config::load(path)?;
        let mut services = HashMap::new();
        for cfg in file.services {
            services.insert(cfg.id.clone(), ManagedService::new(cfg));
        }
        let supervisor = Arc::new(Self {
            services: Mutex::new(services),
            config_path: path.to_path_buf(),
        });

        for svc in supervisor.all().await {
            if svc.cfg.read().await.autostart {
                let name = svc.cfg.read().await.name.clone();
                tracing::info!(service = %name, "autostart");
                if let Err(e) = svc.start().await {
                    tracing::warn!(service = %name, "autostart failed: {e}");
                }
            }
        }
        Ok(supervisor)
    }

    pub async fn all(&self) -> Vec<Arc<ManagedService>> {
        let list: Vec<Arc<ManagedService>> = {
            let services = self.services.lock().await;
            services.values().cloned().collect()
        };
        let mut keyed: Vec<(String, Arc<ManagedService>)> = Vec::new();
        for svc in list {
            let name = svc.cfg.read().await.name.clone();
            keyed.push((name, svc));
        }
        keyed.sort_by(|a, b| a.0.cmp(&b.0));
        keyed.into_iter().map(|(_, svc)| svc).collect()
    }

    pub async fn list_infos(&self) -> Vec<ServiceInfo> {
        let mut infos = Vec::new();
        for svc in self.all().await {
            infos.push(svc.info().await);
        }
        infos
    }

    pub async fn get(&self, id: &str) -> Option<Arc<ManagedService>> {
        self.services.lock().await.get(id).cloned()
    }

    async fn persist(&self) -> Result<(), String> {
        let mut file = ConfigFile::default();
        for svc in self.all().await {
            file.services.push(svc.cfg.read().await.clone());
        }
        config::save(&self.config_path, &file).map_err(|e| e.to_string())
    }

    pub async fn create(&self, input: ServiceInput) -> Result<Arc<ManagedService>, String> {
        input.validate()?;
        let cfg = normalize_input(input);
        let cfg = ServiceConfig {
            id: uuid::Uuid::new_v4().to_string(),
            ..cfg
        };
        let svc = ManagedService::new(cfg);
        self.services
            .lock()
            .await
            .insert(svc.cfg.read().await.id.clone(), svc.clone());
        self.persist().await?;
        Ok(svc)
    }

    pub async fn update(
        &self,
        id: &str,
        input: ServiceInput,
    ) -> Result<Arc<ManagedService>, String> {
        input.validate()?;
        let svc = self.get(id).await.ok_or("服务不存在")?;
        {
            let mut cfg = svc.cfg.write().await;
            let next = normalize_input(input);
            let process_changed = cfg.command != next.command
                || cfg.args != next.args
                || cfg.cwd != next.cwd
                || cfg.env != next.env;
            if process_changed && svc.is_busy().await {
                return Err("服务正在运行，请先停止后再修改启动配置".into());
            }
            cfg.name = next.name;
            cfg.command = next.command;
            cfg.args = next.args;
            cfg.cwd = next.cwd;
            cfg.env = next.env;
            cfg.autostart = next.autostart;
            cfg.auto_restart = next.auto_restart;
        }
        self.persist().await?;
        Ok(svc)
    }

    pub async fn set_autostart(&self, id: &str, enabled: bool) -> Result<ServiceInfo, String> {
        let svc = self.get(id).await.ok_or("服务不存在")?;
        svc.cfg.write().await.autostart = enabled;
        self.persist().await?;
        Ok(svc.info().await)
    }

    pub async fn delete(&self, id: &str) -> Result<(), String> {
        let svc = self.get(id).await.ok_or("服务不存在")?;
        svc.stop().await?;
        self.services.lock().await.remove(id);
        self.persist().await?;
        Ok(())
    }

    /// Stop every running service (used on manager shutdown).
    pub async fn shutdown_all(&self) {
        for svc in self.all().await {
            let _ = svc.stop().await;
        }
    }
}
