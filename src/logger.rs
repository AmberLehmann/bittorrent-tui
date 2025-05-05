use log::{LevelFilter, Log, Metadata, Record, SetLoggerError};

pub struct Logger {
    pub client: std::sync::mpsc::Sender<String>,
}

impl Logger {
    pub fn new(sender: std::sync::mpsc::Sender<String>) -> Self {
        Self { client: sender }
    }
    pub fn init(self) -> Result<(), SetLoggerError> {
        log::set_max_level(LevelFilter::Trace);
        log::set_boxed_logger(Box::new(self))
    }
}

impl Log for Logger {
    fn enabled(&self, _metadata: &Metadata<'_>) -> bool {
        true
    }
    fn log(&self, record: &Record<'_>) {
        self.client
            .send(format!(
                "[{}] {}: {}\n",
                record.level(),
                record.target(),
                record.args()
            ))
            .unwrap()
    }
    fn flush(&self) {}
}
