use std::time::SystemTime;
use std::sync::mpsc;
use std::thread;
use std::fs::OpenOptions;
use std::io::Write;
use std::process::{Command, Stdio, Child};
use std::collections::HashMap;
use std::net::TcpStream;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Info = 1,
    Warn = 2,
    Error = 3,
}

impl LogLevel {
    fn as_str(&self) -> &'static str {
        match self {
            LogLevel::Info => "INFO",
            LogLevel::Warn => "WARN",
            LogLevel::Error => "ERROR",
        }
    }
}

#[derive(Clone)]
pub enum LogFormat {
    Text,
    Json,
    Xml,
}

#[derive(Clone)]
pub enum LogTarget {
    Console,
    LocalFile { dir_path: String, prefix: String, retention_days: u64 },
    Http { host_port: String, path: String },
    Ssh { user_host: String, path: String },
}

#[derive(Clone)]
pub struct LogDispatcher {
    pub min_level: LogLevel,
    pub format: LogFormat,
    pub target: LogTarget,
}

pub struct LoggerConfig {
    pub dispatchers: Vec<LogDispatcher>,
    pub heart_beat_ms: u64,
    pub cleanup_interval_secs: Option<u64>,
}

impl LoggerConfig {
    pub fn new(dispatchers: Vec<LogDispatcher>) -> Self {
        LoggerConfig {
            dispatchers,
            heart_beat_ms: 1000,
            cleanup_interval_secs: None,
        }
    }

    pub fn set_heart_beat(mut self, ms: u64) -> Self {
        self.heart_beat_ms = ms;
        self
    }

    pub fn set_cleanup_interval(mut self, secs: u64) -> Self {
        self.cleanup_interval_secs = Some(secs);
        self
    }
}

struct LogMessage {
    level: LogLevel,
    msg: String,
    time: String,
}

#[derive(Clone)]
pub struct Logger {
    tx: mpsc::Sender<LogMessage>,
}

impl Logger {
    pub fn new(config: LoggerConfig) -> Self {
        let (tx, rx) = mpsc::channel();
        
        thread::spawn(move || {
            logger_worker(rx, config);
        });

        Logger { tx }
    }

    pub fn info(&self, msg: &str) {
        let _ = self.tx.send(LogMessage { level: LogLevel::Info, msg: msg.to_string(), time: timestamp() });
    }

    pub fn warn(&self, msg: &str) {
        let _ = self.tx.send(LogMessage { level: LogLevel::Warn, msg: msg.to_string(), time: timestamp() });
    }

    pub fn error(&self, msg: &str) {
        let _ = self.tx.send(LogMessage { level: LogLevel::Error, msg: msg.to_string(), time: timestamp() });
    }
}

/// Worker em background que processa a fila de logs e limpa arquivos velhos
fn logger_worker(rx: mpsc::Receiver<LogMessage>, config: LoggerConfig) {
    let mut last_cleanup = SystemTime::now();
    let mut ssh_pipes: HashMap<String, Child> = HashMap::new();

    loop {
        match rx.recv_timeout(std::time::Duration::from_millis(config.heart_beat_ms)) {
            Ok(msg) => {
                for dispatcher in &config.dispatchers {
                    if msg.level >= dispatcher.min_level {
                        let formatted = format_log(&msg, &dispatcher.format);
                        dispatch_log(&formatted, &dispatcher.target, &mut ssh_pipes);
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // Apenas segue para checar limpeza
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                break;
            }
        }

        if let Some(interval) = config.cleanup_interval_secs {
            if last_cleanup.elapsed().unwrap().as_secs() > interval {
                for dispatcher in &config.dispatchers {
                    do_cleanup(&dispatcher.target);
                }
                last_cleanup = SystemTime::now();
            }
        }
    }
}

fn format_log(msg: &LogMessage, format: &LogFormat) -> String {
    match format {
        LogFormat::Text => format!("[{}] [{}] {}", msg.time, msg.level.as_str(), msg.msg),
        LogFormat::Json => format!(
            r#"{{"timestamp": "{}", "level": "{}", "msg": "{}"}}"#,
            msg.time, msg.level.as_str(), msg.msg.replace("\"", "\\\"")
        ),
        LogFormat::Xml => format!(
            "<log><time>{}</time><level>{}</level><msg>{}</msg></log>",
            msg.time, msg.level.as_str(), msg.msg
        ),
    }
}

fn dispatch_log(payload: &str, target: &LogTarget, ssh_pipes: &mut HashMap<String, Child>) {
    match target {
        LogTarget::Console => {
            println!("{}", payload);
        }
        LogTarget::LocalFile { dir_path, prefix, .. } => {
            let now_date = &timestamp()[0..10];
            let filepath = format!("{}/{}_{}.log", dir_path, prefix, now_date);
            let _ = std::fs::create_dir_all(dir_path);
            if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(filepath) {
                let _ = writeln!(file, "{}", payload);
            }
        }
        LogTarget::Http { host_port, path } => {
            if let Ok(mut stream) = TcpStream::connect(host_port) {
                let request = format!(
                    "POST {} HTTP/1.1\r\nHost: {}\r\nContent-Length: {}\r\nContent-Type: text/plain\r\n\r\n{}",
                    path, host_port, payload.len(), payload
                );
                let _ = stream.write_all(request.as_bytes());
            }
        }
        LogTarget::Ssh { user_host, path } => {
            // Usa pipe bidirecional persistente (Impede injeção de comandos)
            let key = format!("{}@{}", user_host, path);
            let child = ssh_pipes.entry(key).or_insert_with(|| {
                Command::new("ssh")
                    .arg(user_host)
                    .arg(format!("cat >> {}", path))
                    .stdin(Stdio::piped())
                    .spawn()
                    .expect("Failed to start SSH process")
            });
            if let Some(stdin) = child.stdin.as_mut() {
                let _ = writeln!(stdin, "{}", payload);
            }
        }
    }
}

/// Evicção de logs antigos deletando arquivos inteiros (Log Rotation)
fn do_cleanup(target: &LogTarget) {
    if let LogTarget::LocalFile { dir_path, prefix, retention_days } = target {
        let limit_date = &timestamp_ago(*retention_days)[0..10];
        
        if let Ok(entries) = std::fs::read_dir(dir_path) {
            for entry in entries.flatten() {
                if let Ok(file_name) = entry.file_name().into_string() {
                    let prefix_with_underscore = format!("{}_", prefix);
                    if file_name.starts_with(&prefix_with_underscore) && file_name.ends_with(".log") {
                        let date_part = &file_name[prefix_with_underscore.len()..file_name.len() - 4];
                        if date_part.len() == 10 && date_part < limit_date {
                            let _ = std::fs::remove_file(entry.path());
                        }
                    }
                }
            }
        }
    }
}

// ==========================================
// MOTOR DE TIMESTAMP (SEM CHRONO)
// ==========================================

fn timestamp_from_secs(secs: u64) -> String {
    let days = secs / 86400;
    let time_of_day = secs % 86400;

    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    let (year, month, day) = days_to_date(days);

    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        year, month, day, hours, minutes, seconds
    )
}

fn timestamp() -> String {
    let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs();
    timestamp_from_secs(now)
}

fn timestamp_ago(days: u64) -> String {
    let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs();
    let past = now.saturating_sub(days * 86400);
    timestamp_from_secs(past)
}

fn days_to_date(days_since_epoch: u64) -> (u64, u64, u64) {
    let z = days_since_epoch + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y_final = if m <= 2 { y + 1 } else { y };
    (y_final, m, d)
}
