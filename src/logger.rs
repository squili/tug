// basic hierarchical logger

use std::{fmt::Display, io::Write, sync::Arc};

use parking_lot::Mutex;

#[derive(Clone)]
pub struct Logger {
    inner: Arc<Mutex<LoggerInner>>,
}

struct LoggerInner {
    trace: bool,
}

impl Logger {
    pub fn new() -> Logger {
        Logger {
            inner: Arc::new(Mutex::new(LoggerInner {
                trace: std::env::var("TUG_TRACE").is_ok(),
            })),
        }
    }

    pub fn log(&self, d: impl Display) {
        let _ = std::io::stdout().lock().write_all(format!("{d}\n").as_bytes());
    }

    pub fn trace(&self, d: impl Display) {
        let inner = self.inner.lock();
        if !inner.trace {
            return;
        }
        let _ = std::io::stdout().lock().write_all(format!("[TRACE] {d}\n").as_bytes());
    }
}
