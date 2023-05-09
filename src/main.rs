mod runners;
mod utils;

use crossterm::{
    cursor, queue,
    style::{Color, Print, ResetColor, SetForegroundColor},
    terminal,
};
use std::{
    io::{stdout, Write},
    sync::{mpsc::Sender, Arc, Mutex},
    time::{Duration, SystemTime},
};
use sysinfo::{ProcessExt, System, SystemExt};

#[derive(PartialEq, PartialOrd, Debug, Clone)]
pub struct Files {
    name: String,
    path: String,
    time: SystemTime,
}

#[derive(Clone, Debug, PartialEq)]
pub enum WatcherEvent {
    Starting,
    Watching(sysinfo::Pid),
    FileChanged(Vec<Files>),
    Stopping,
    Stopped,
    Error(String),
    Exit,
}

impl std::fmt::Display for WatcherEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            WatcherEvent::Starting => write!(f, "STARTING"),
            WatcherEvent::Watching(_) => write!(f, "WATCHING"),
            WatcherEvent::FileChanged(_) => write!(f, "FILE_CHANGED"),
            WatcherEvent::Stopping => write!(f, "STOPPING"),
            WatcherEvent::Stopped => write!(f, "STOPPED"),
            WatcherEvent::Error(_) => write!(f, "ERROR"),
            WatcherEvent::Exit => write!(f, "EXIT"),
        }
    }
}

struct WatchDog {
    dir_cmd: String,
    dir_path: String,
    status: Arc<Mutex<WatcherEvent>>,
    event: Sender<WatcherEvent>,
}

impl Default for WatchDog {
    fn default() -> Self {
        let mut stdout = stdout();

        let (event_sender, event_receiver) = std::sync::mpsc::channel::<WatcherEvent>();

        let dir_path = std::env::current_dir()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let dir_path_clone = dir_path.clone();

        let dir_cmd = "cargo run".to_string();

        let watch_dog_status = Arc::new(Mutex::new(WatcherEvent::Stopped));
        let watch_dog_status_clone = Arc::clone(&watch_dog_status);

        let cargo_exe_pid = Arc::new(Mutex::new(sysinfo::Pid::from(usize::default())));

        std::thread::spawn(move || loop {
            // Sleep loop to loosen CPU stress
            std::thread::sleep(Duration::from_millis(100));

            // Check incoming Directory events
            match event_receiver.recv() {
                Ok(event) => match event {
                    WatcherEvent::Starting => {
                        queue!(
                            stdout,
                            terminal::Clear(terminal::ClearType::All),
                            cursor::MoveTo(0, 0),
                            SetForegroundColor(Color::Cyan),
                            Print("Waiting for initialisation!"),
                            cursor::MoveToNextLine(1),
                        )
                        .unwrap();
                    }
                    WatcherEvent::Watching(process_id) => {
                        let mut pname = cargo_exe_pid.lock().unwrap();
                        *pname = process_id.clone();

                        queue!(
                            stdout,
                            terminal::Clear(terminal::ClearType::All),
                            cursor::MoveTo(0, 0),
                            SetForegroundColor(Color::Cyan),
                            Print("Watching directory for changes:"),
                            cursor::MoveToNextLine(1),
                            SetForegroundColor(Color::DarkYellow),
                            cursor::MoveRight(2),
                            Print(dir_path_clone.clone()),
                            cursor::MoveToNextLine(2),
                            SetForegroundColor(Color::Cyan),
                            Print("Application is ready to reload."),
                            ResetColor
                        )
                        .unwrap();
                    }
                    WatcherEvent::FileChanged(files) => {
                        queue!(
                            stdout,
                            terminal::Clear(terminal::ClearType::All),
                            cursor::MoveTo(0, 0),
                            SetForegroundColor(Color::Cyan),
                            Print("File(s) were changed:"),
                            SetForegroundColor(Color::DarkYellow),
                            cursor::MoveToNextLine(1),
                        )
                        .unwrap();

                        for file in files {
                            queue!(
                                stdout,
                                cursor::MoveRight(2),
                                Print(file.name.clone()),
                                cursor::MoveToNextLine(1),
                            )
                            .unwrap();
                        }

                        queue!(
                            stdout,
                            cursor::MoveToNextLine(1),
                            SetForegroundColor(Color::Cyan),
                            terminal::Clear(terminal::ClearType::CurrentLine),
                            Print("Reloading application..."),
                            cursor::MoveToNextLine(1),
                        )
                        .unwrap();

                        // Find and kill the process
                        let process_id = cargo_exe_pid.lock().unwrap();
                        let mut sys = System::new();
                        sys.refresh_processes();
                        if let Some(process) = sys.process(sysinfo::Pid::from(*process_id)) {
                            process.kill();
                        }

                        stdout.flush().unwrap();

                        let mut status = watch_dog_status_clone.lock().unwrap();
                        *status = WatcherEvent::Starting;
                    }
                    WatcherEvent::Error(err) => {
                        queue!(stdout, SetForegroundColor(Color::Red), Print(err)).unwrap();
                    }
                    WatcherEvent::Exit => {
                        queue!(
                            stdout,
                            terminal::Clear(terminal::ClearType::All),
                            cursor::MoveTo(0, 0),
                            SetForegroundColor(Color::Cyan),
                            Print("Exiting program!"),
                            ResetColor
                        )
                        .unwrap();

                        let mut status = watch_dog_status_clone.lock().unwrap();
                        *status = event;

                        std::process::exit(0);
                    }
                    _ => todo!(),
                },
                Err(_) => todo!(),
            }

            stdout.flush().unwrap();
        });

        Self {
            dir_cmd,
            dir_path,
            status: watch_dog_status,
            event: event_sender,
        }
    }
}

impl WatchDog {
    fn begin_watching(&self) {
        self.event.send(WatcherEvent::Starting).unwrap();

        let dir_event = self.event.clone();
        let dir_path = self.dir_path.clone();

        std::thread::Builder::new()
            .name("DirWatcher".to_string())
            .spawn(|| runners::dir_runner(dir_event, dir_path))
            .expect("Could not spawn thread!");

        let dir_event = self.event.clone();
        let dir_path = self.dir_path.clone();
        let dir_cmd = self.dir_cmd.clone();
        std::thread::Builder::new()
            .name("CommandRunner".to_string())
            .spawn(|| runners::cmd_runner(dir_event, dir_cmd, dir_path))
            .expect("Could not spawn thread!");
    }

    fn get_status(&self) -> WatcherEvent {
        match self.status.lock() {
            Ok(status) => status.clone(),
            Err(_) => return WatcherEvent::Stopping,
        }
    }
}

fn main() {
    // let mut stdout = stdout();
    let watch_dog = WatchDog::default();

    watch_dog.begin_watching();

    loop {
        std::thread::sleep(Duration::from_millis(1000));
        let _status = watch_dog.get_status();
    }
}
