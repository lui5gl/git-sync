use chrono::Local;
use std::fs::OpenOptions;
use std::io::Write;

pub struct Logger {
    log_file: String,
}

impl Logger {
    pub fn new(log_file: String) -> Self {
        Logger { log_file }
    }

    pub fn log(&self, message: &str) {
        let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S");
        let log_entry = format!("[{}] {}\n", timestamp, message);

        // Mostrar en consola
        print!("{}", message);

        // Escribir en el archivo de registro
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_file)
        {
            let _ = file.write_all(log_entry.as_bytes());
        }
    }

    pub fn log_line(&self, message: &str) {
        self.log(&format!("{}\n", message));
    }

    pub fn log_error(&self, message: &str) {
        let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S");
        let log_entry = format!("[{}] ERROR: {}\n", timestamp, message);

        // Mostrar en consola
        eprint!("ERROR: {}\n", message);

        // Escribir en el archivo de registro
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_file)
        {
            let _ = file.write_all(log_entry.as_bytes());
        }
    }
}
