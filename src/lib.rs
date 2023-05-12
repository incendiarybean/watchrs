pub mod utils;

use crossterm::{
    cursor, queue,
    style::{Color, Print, ResetColor, SetForegroundColor},
    terminal,
};
use std::{
    io::{stdout, Write},
    sync::mpsc::{Receiver, Sender},
    time::{Duration, SystemTime},
};
use sysinfo::{ProcessExt, System, SystemExt};

#[derive(PartialEq, PartialOrd, Clone, Debug)]
pub struct Files {
    pub name: String,
    pub path: String,
    pub time: SystemTime,
}

#[derive(Clone, PartialEq, Debug)]
pub enum WatcherEvent {
    Starting,
    Watching(sysinfo::Pid, Vec<String>),
    FileChanged(Vec<Files>),
    Stopping,
    Stopped,
    Error(String),
    Exit,
}

pub struct WatchRs {
    pub status: WatcherEvent,
    pub process_id: Option<sysinfo::Pid>,
    pub dir_path: String,
    pub event: Sender<WatcherEvent>,

    watcher: Receiver<WatcherEvent>,
}

impl Default for WatchRs {
    fn default() -> Self {
        let (event_sender, event_receiver) = std::sync::mpsc::channel::<WatcherEvent>();
        let dir_path = std::env::current_dir()
            .expect("Could not get Path from current_dir().")
            .to_string_lossy()
            .into_owned();

        Self {
            status: WatcherEvent::Stopped,
            process_id: None,
            dir_path,
            event: event_sender,

            watcher: event_receiver,
        }
    }
}

impl WatchRs {
    /// Launches an instance of WatchRS
    pub fn begin_watching(self) -> Result<(), std::io::Error> {
        let mut stdout = stdout();
        queue!(
            stdout,
            terminal::Clear(terminal::ClearType::All),
            cursor::MoveTo(0, 0),
            SetForegroundColor(Color::Cyan),
            Print("Waiting for initialisation!"),
            cursor::MoveToNextLine(1),
        )
        .expect("Could not write to stdout.");

        stdout.flush().expect("Could not flush on Stdout");

        // Start processes
        self.spawn_directory_watcher();
        self.spawn_command_runner();

        // Handle events
        self.event_handler();

        Ok(())
    }

    /// Create directory watcher
    /// Watches directory and sends event on changes
    fn spawn_directory_watcher(&self) {
        let path = self.dir_path.clone();
        self.event
            .send(WatcherEvent::Starting)
            .expect("Could not send event.");

        let event = self.event.clone();
        std::thread::Builder::new()
            .name("DirWatcher".to_string())
            .spawn(move || {
                let file_changes = utils::dir_watcher(path, Duration::from_millis(1000))
                    .expect("Could not find changes.");
                event
                    .clone()
                    .send(WatcherEvent::FileChanged(file_changes))
                    .expect("Could not send event.");
            })
            .expect("Could not spawn thread!");
    }

    /// Create command runner
    /// Creates and waits for process to end
    fn spawn_command_runner(&self) {
        let path = self.dir_path.clone();
        let event = self.event.clone();
        std::thread::Builder::new()
            .name("CommandRunner".to_string())
            .spawn(move || loop {
                let (child_process, pid, exe_names) =
                    utils::cmd_runner(path.clone()).expect("Could not run command successfully.");

                event
                    .send(WatcherEvent::Watching(pid, exe_names))
                    .expect("Could not send event.");

                match child_process.wait_with_output() {
                    Ok(output) => {
                        if let Some(status_code) = output.status.code() {
                            if status_code == 0 {
                                // Application was closed
                                event
                                    .send(WatcherEvent::Exit)
                                    .expect("Could not send event.");
                                break;
                            } else {
                                // Application was terminated
                                event
                                    .send(WatcherEvent::Starting)
                                    .expect("Could not send event.");
                            }
                        }
                    }
                    Err(e) => event
                        .send(WatcherEvent::Error(e.to_string()))
                        .expect("Could not send event."),
                }
            })
            .expect("Could not spawn thread!");
    }

    /// Handles incoming events from watchers & runners
    fn event_handler(mut self) {
        let mut stdout = stdout();

        loop {
            match self.watcher.recv() {
                Ok(event) => match event {
                    WatcherEvent::Watching(process_id, exe_names) => {
                        self.process_id = Some(process_id);

                        queue!(
                            stdout,
                            terminal::Clear(terminal::ClearType::All),
                            cursor::MoveTo(0, 0)
                        )
                        .unwrap();

                        queue!(
                            stdout,
                            SetForegroundColor(Color::Cyan),
                            Print(format!("Process ID:")),
                            cursor::MoveRight(2),
                            SetForegroundColor(Color::Green),
                            Print(process_id),
                            cursor::MoveToNextLine(1),
                            SetForegroundColor(Color::Cyan),
                            Print(format!("Executable:")),
                            SetForegroundColor(Color::Green),
                        )
                        .unwrap();

                        if exe_names.len() > 1 {
                            for exe in exe_names.clone() {
                                queue!(stdout, cursor::MoveRight(2), Print(format!("{exe}")))
                                    .unwrap();
                            }
                            queue!(
                                stdout,
                                SetForegroundColor(Color::Red),
                                cursor::MoveToNextLine(1),
                                Print("WARNING: Expected 1 platform associated executable but found multiple."),
                                cursor::MoveToNextLine(1),
                                Print("Has this project been renamed?"),
                                cursor::MoveToNextLine(1),
                                Print("If you encounter issues, remove the excess executables in the ./target/debug folder."),
                                cursor::MoveToNextLine(2)
                            )
                            .unwrap();
                        } else {
                            queue!(
                                stdout,
                                cursor::MoveRight(2),
                                Print(format!("{}", exe_names[0]))
                            )
                            .unwrap();
                        }

                        queue!(
                            stdout,
                            cursor::MoveToNextLine(1),
                            SetForegroundColor(Color::Cyan),
                            Print("Watching directory for changes:"),
                            cursor::MoveToNextLine(1),
                            SetForegroundColor(Color::DarkYellow),
                            cursor::MoveRight(2),
                            Print(self.dir_path.clone()),
                            cursor::MoveToNextLine(2),
                            SetForegroundColor(Color::Cyan),
                            Print("Application is ready to reload."),
                            ResetColor
                        )
                        .expect("Could not write to stdout.");
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
                        .expect("Could not write to stdout.");

                        for file in files {
                            queue!(
                                stdout,
                                cursor::MoveRight(2),
                                Print(file.name.clone()),
                                cursor::MoveToNextLine(1),
                            )
                            .expect("Could not write to stdout.");
                        }

                        queue!(
                            stdout,
                            cursor::MoveToNextLine(1),
                            SetForegroundColor(Color::Cyan),
                            terminal::Clear(terminal::ClearType::CurrentLine),
                            Print("Reloading application..."),
                            cursor::MoveToNextLine(1),
                        )
                        .expect("Could not write to stdout.");

                        // Find and kill the process
                        if let Some(process_id) = self.process_id {
                            let mut sys = System::new();
                            sys.refresh_processes();
                            if let Some(process) = sys.process(sysinfo::Pid::from(process_id)) {
                                process.kill();
                            }
                        }

                        // Restart Directory Service
                        self.spawn_directory_watcher();
                    }
                    WatcherEvent::Error(err) => {
                        queue!(stdout, SetForegroundColor(Color::Red), Print(err))
                            .expect("Could not write to stdout.");
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
                        .expect("Could not write to stdout.");
                        break;
                    }
                    _ => (),
                },
                Err(_) => (),
            }
            stdout.flush().expect("Could not flush on stdout.");
        }
    }
}
