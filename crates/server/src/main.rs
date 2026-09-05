use anyhow::{Context, bail};
use services_manager_server::{build_app, config};
use std::path::PathBuf;
use std::sync::Arc;

const USAGE: &str = "\
services-manager — web UI for managing background services

USAGE:
  services-manager [--host HOST] [--port PORT] [--config PATH]

OPTIONS:
  --host HOST     Listen address (default 127.0.0.1; env SM_HOST)
  --port PORT     Listen port (default 8730; env SM_PORT)
  --config PATH   Config file path (env SM_CONFIG; default
                  ~/.config/services-manager/config.toml)
  -h, --help      Show this help
";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let mut host = std::env::var("SM_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let mut port: u16 = std::env::var("SM_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8730);
    let mut config_path: Option<PathBuf> = std::env::var("SM_CONFIG").ok().map(PathBuf::from);

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--host" if i + 1 < args.len() => {
                host = args[i + 1].clone();
                i += 2;
            }
            "--port" if i + 1 < args.len() => {
                port = args[i + 1].parse().context("invalid --port value")?;
                i += 2;
            }
            "--config" if i + 1 < args.len() => {
                config_path = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "-h" | "--help" => {
                print!("{USAGE}");
                return Ok(());
            }
            other => bail!("unknown argument: {other}\n\n{USAGE}"),
        }
    }

    let Some(config_path) = config_path.or_else(config::default_config_path) else {
        bail!("cannot determine config directory; pass --config PATH");
    };
    let listen = format_listen(&host, port)?;

    let (app, supervisor) = build_app(&config_path).await?;

    // On ctrl-c / SIGTERM: stop all managed services, then exit hard (SSE
    // connections keep the graceful path alive forever).
    let sup = Arc::clone(&supervisor);
    tokio::spawn(async move {
        shutdown_signal().await;
        tracing::info!("shutting down: stopping all services");
        sup.shutdown_all().await;
        std::process::exit(0);
    });

    let listener = tokio::net::TcpListener::bind(listen).await?;
    tracing::info!(
        "services manager listening on http://{}",
        listener.local_addr()?
    );
    tracing::info!("config file: {}", config_path.display());
    axum::serve(listener, app).await?;
    Ok(())
}

fn format_listen(host: &str, port: u16) -> anyhow::Result<std::net::SocketAddr> {
    let s = if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    };
    s.parse()
        .with_context(|| format!("invalid listen address: {s}"))
}

async fn shutdown_signal() {
    let ctrl_c = tokio::signal::ctrl_c();
    #[cfg(unix)]
    {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler");
        tokio::select! {
            _ = ctrl_c => {}
            _ = term.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = ctrl_c.await;
    }
}
