//! In-memory ring buffer of service output with broadcast fan-out for SSE.

use serde::Serialize;
use std::collections::VecDeque;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::{Mutex, broadcast};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LogStream {
    Stdout,
    Stderr,
    System,
}

#[derive(Debug, Clone, Serialize)]
pub struct LogLine {
    pub seq: u64,
    /// Unix epoch milliseconds.
    pub ts: i64,
    pub stream: LogStream,
    pub line: String,
}

struct Inner {
    seq: u64,
    lines: VecDeque<LogLine>,
}

/// Bounded log buffer per service plus a broadcast channel for live tailing.
pub struct LogStore {
    cap: usize,
    inner: Mutex<Inner>,
    tx: broadcast::Sender<LogLine>,
}

impl LogStore {
    pub fn new(cap: usize) -> Self {
        let (tx, _) = broadcast::channel(1024);
        Self {
            cap,
            inner: Mutex::new(Inner {
                seq: 0,
                lines: VecDeque::new(),
            }),
            tx,
        }
    }

    pub async fn push(&self, stream: LogStream, line: impl Into<String>) {
        let entry = {
            let mut inner = self.inner.lock().await;
            inner.seq += 1;
            let entry = LogLine {
                seq: inner.seq,
                ts: now_millis(),
                stream,
                line: line.into(),
            };
            inner.lines.push_back(entry.clone());
            while inner.lines.len() > self.cap {
                inner.lines.pop_front();
            }
            entry
        };
        // Ignore send errors: no live subscribers right now.
        let _ = self.tx.send(entry);
    }

    /// If `after` is set, return every line with `seq > after`; otherwise the
    /// last `tail` lines.
    pub async fn snapshot(&self, after: Option<u64>, tail: usize) -> Vec<LogLine> {
        let inner = self.inner.lock().await;
        match after {
            Some(a) => inner.lines.iter().filter(|l| l.seq > a).cloned().collect(),
            None => {
                let skip = inner.lines.len().saturating_sub(tail);
                inner.lines.iter().skip(skip).cloned().collect()
            }
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<LogLine> {
        self.tx.subscribe()
    }
}

pub fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
