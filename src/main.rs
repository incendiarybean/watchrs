use std::{
    io::{stdout, Cursor, Write},
    process::Stdio,
    sync::{mpsc::Sender, Arc, Mutex},
    time::{Duration, SystemTime},
};

// use colored::{Color, Colorize};
use crossterm::{
    cursor, execute, queue,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
    terminal::{self, Clear},
    Command, ExecutableCommand,
};
use sysinfo::{Pid, ProcessExt, ProcessRefreshKind, RefreshKind, System, SystemExt};

// Check directory
// Ask for command || defualt
// Ask for entry || defualt
// should watch all files excluding target by default

#[derive(PartialEq, PartialOrd, Debug, Clone)]
struct Files {
    name: String,
    path: String,
    time: SystemTime,
}

fn visit_dirs(
    ignored_paths: Vec<&std::path::Path>,
    dir: &std::path::Path,
    cb: &mut dyn FnMut(std::fs::DirEntry),
) -> std::io::Result<()> {
    if dir.is_dir() && !ignored_paths.contains(&dir) {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                visit_dirs(ignored_paths.clone(), &path, cb)?;
            } else {
                cb(entry);
            }
        }
    }
    Ok(())
}

fn grab_directory_and_files(path_name: String) -> Result<Vec<Files>, std::io::Error> {
    let path = std::path::Path::new(&path_name);
    let mut dir_contents = Vec::<std::fs::DirEntry>::new();

    // TODO: Make this dynamic
    let mut ignored_paths = Vec::<&std::path::Path>::new();
    let target_dir = format!("{}/target", path_name);
    ignored_paths.push(std::path::Path::new(&target_dir));

    // Callback to update list of directory contents
    let mut update_vec = |val| {
        dir_contents.push(val);
    };

    match visit_dirs(ignored_paths, &path, &mut update_vec) {
        Ok(_) => {}
        Err(e) => println!("There was an error! {}", e),
    }

    let file_names: Vec<Files> = dir_contents
        .into_iter()
        .map(|item| Files {
            name: item.file_name().to_string_lossy().to_string(),
            path: item.path().to_string_lossy().to_string(),
            time: item.metadata().unwrap().modified().unwrap(),
        })
        .collect();

    Ok(file_names)
}

fn dir_runner(dir_event: std::sync::mpsc::Sender<WatcherEvent>, dir_path: String) {
    let mut file_names = grab_directory_and_files(dir_path.clone())
        .expect("Could not retrieve files from Directory.");

    loop {
        let file_names_reloaded = grab_directory_and_files(dir_path.clone())
            .expect("Could not retrieve files from Directory.");

        let file_names_reloaded_clone = file_names_reloaded.clone();

        let file_changes: Vec<Files> = file_names_reloaded
            .into_iter()
            .filter(|file| {
                // iterate between files and check if they match their counterpart in file_names_reloaded
                if !file_names.contains(file) {
                    true
                } else {
                    false
                }
            })
            .collect();

        // If different file was saved previously
        if file_names != file_names_reloaded_clone {
            // Reset current files
            file_names = file_names_reloaded_clone;

            // This should only print the once
            dir_event
                .send(WatcherEvent::FileChanged(file_changes))
                .unwrap();
        }

        std::thread::sleep(Duration::from_millis(1000));
    }
}

fn cmd_runner(dir_event: std::sync::mpsc::Sender<WatcherEvent>, dir_cmd: String, dir_path: String) {
    if cfg!(target_os = "windows") {
        let dir_event_copy = dir_event.clone();
        std::thread::spawn(move || {
            let process = std::process::Command::new("cargo")
                .args(["run"])
                .stdout(Stdio::piped())
                .stdin(Stdio::piped())
                .output();
            if let Err(e) = process {
                dir_event_copy
                    .send(WatcherEvent::Error(e.to_string()))
                    .unwrap();
            } else {
                dir_event_copy.send(WatcherEvent::Exit).unwrap();
            }
        });

        let mut exe_name = String::new();
        for entry in std::fs::read_dir(dir_path + "/target/debug").unwrap() {
            if let Some(found_file) = entry.unwrap().file_name().to_str() {
                if found_file.contains(".exe") {
                    exe_name = found_file.to_string();
                }
            }
        }

        let mut sys = System::new_with_specifics(
            RefreshKind::default().with_processes(ProcessRefreshKind::everything()),
        );

        let process_name = loop {
            // Refresh Process list
            sys.refresh_processes();

            let mut process_name = String::new();
            for key in sys.processes() {
                if let Some(file_name) = key.1.exe().file_name() {
                    if file_name
                        .to_os_string()
                        .into_string()
                        .unwrap()
                        .contains(&exe_name)
                    {
                        process_name = key.1.name().to_string();
                        break;
                    }
                }
            }

            if !process_name.is_empty() {
                break process_name;
            };
            std::thread::sleep(Duration::from_millis(2000));
        };

        dir_event
            .send(WatcherEvent::Watching(process_name))
            .unwrap();
    }
}

#[derive(Clone, Debug, PartialEq)]
enum WatcherEvent {
    Starting,
    InvalidDirectory,
    Investigating,
    Watching(String),
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
            WatcherEvent::InvalidDirectory => write!(f, "INVALID_DIR"),
            WatcherEvent::Investigating => write!(f, "INVESTIGATING"),
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

        let app_process_name = Arc::new(Mutex::new(String::new()));
        let app_process_name_clone = Arc::clone(&app_process_name);

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
                            Print("Waiting for initialisation!")
                        )
                        .unwrap();
                    }
                    WatcherEvent::InvalidDirectory => todo!(),
                    WatcherEvent::Investigating => todo!(),
                    WatcherEvent::Watching(process_name) => {
                        let mut pname = app_process_name_clone.lock().unwrap();
                        *pname = process_name.clone();

                        queue!(
                            stdout,
                            terminal::Clear(terminal::ClearType::All),
                            cursor::MoveTo(0, 0),
                            SetForegroundColor(Color::Cyan),
                            Print("Watching directory for changes:"),
                            Print(process_name),
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
                        )
                        .unwrap();

                        let process_name = app_process_name_clone.lock().unwrap();
                        let sys = System::new_with_specifics(
                            RefreshKind::default().with_processes(ProcessRefreshKind::everything()),
                        );

                        queue!(
                            stdout,
                            terminal::Clear(terminal::ClearType::CurrentLine),
                            Print(&process_name)
                        )
                        .unwrap();

                        // Find all processes with selected name and kill them
                        for process in sys.processes_by_exact_name(&process_name) {
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
            .spawn(|| dir_runner(dir_event, dir_path))
            .expect("Could not spawn thread!");

        let dir_event = self.event.clone();
        let dir_path = self.dir_path.clone();
        let dir_cmd = self.dir_cmd.clone();
        std::thread::Builder::new()
            .name("CommandRunner".to_string())
            .spawn(|| cmd_runner(dir_event, dir_cmd, dir_path))
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
