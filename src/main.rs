use std::{
    io::{stdout, Write},
    sync::{mpsc::Sender, Arc, Mutex},
    time::{Duration, SystemTime},
};

use crossterm::{
    cursor, queue,
    style::{Color, Print, ResetColor, SetForegroundColor},
    terminal,
};
use sysinfo::{ProcessExt, System, SystemExt};

#[derive(PartialEq, PartialOrd, Debug, Clone)]
struct Files {
    name: String,
    path: String,
    time: SystemTime,
}

/// A function to scan directories recursively
///
/// # Arguments
/// * `ignored_paths` - a Vec of Paths to ignore
/// * `file` - a Path of the file/folder to check currently
/// * `cb` - a callback function to run when the scan finds a file
fn visit_dirs(
    ignored_paths: Vec<&std::path::Path>,
    file: &std::path::Path,
    cb: &mut dyn FnMut(std::fs::DirEntry),
) -> std::io::Result<()> {
    if file.is_dir() && !ignored_paths.contains(&file) {
        for entry in std::fs::read_dir(file)? {
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

/// A function to get the files from the selected directory
///
/// # Arguments
/// * `dir_path` - a String representation of a directory path
fn grab_directory_and_files(dir_path: String) -> Result<Vec<Files>, std::io::Error> {
    let path = std::path::Path::new(&dir_path);

    // TODO: Make this dynamic
    let mut ignored_paths = Vec::<&std::path::Path>::new();
    let target_dir = format!("{}/target", dir_path);
    ignored_paths.push(std::path::Path::new(&target_dir));

    // Generate a list of all files in the selected directory
    let mut dir_contents = Vec::<std::fs::DirEntry>::new();
    visit_dirs(ignored_paths, &path, &mut |file| {
        dir_contents.push(file);
    })?;

    // Collect file metadata
    let file_metadata: Vec<Files> = dir_contents
        .into_iter()
        .map(|item| Files {
            name: item.file_name().to_string_lossy().to_string(),
            path: item.path().to_string_lossy().to_string(),
            time: item.metadata().unwrap().modified().unwrap(),
        })
        .collect();

    Ok(file_metadata)
}

/// A constant directory scanning service
///
/// # Arguments
/// * `dir_event` - an MSPC Sender of type WatcherEvent
/// * `dir_path` - a String representation of a directory path
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

/// A constant command running service
///
/// # Arguments
/// * `dir_event` - an MSPC Sender of type WatcherEvent
/// * `dir_cmd` - the command to run, which will respawn on executable termination
/// * `dir_path` - a String representation of a directory path
fn cmd_runner(
    dir_event: std::sync::mpsc::Sender<WatcherEvent>,
    _dir_cmd: String,
    dir_path: String,
) -> Result<(), std::io::Error> {
    if cfg!(target_os = "windows") {
        loop {
            // Generate Cargo Run process
            let child_process = std::process::Command::new("cargo").args(["run"]).spawn()?;

            // scan and find executable
            let mut exe_name = String::new();
            for entry in std::fs::read_dir(dir_path.clone() + "/target/debug")
                .expect("Couldn't search directory for executables")
            {
                if let Some(found_file) = entry.unwrap().file_name().to_str() {
                    if found_file.contains(".exe") {
                        exe_name = found_file.to_string();
                    }
                }
            }

            let mut sys = sysinfo::System::new();
            let mut exec_running = false;
            loop {
                for (pid, process) in sys.processes() {
                    if exe_name == process.name().to_owned() {
                        let pid = pid.to_owned();
                        dir_event.send(WatcherEvent::Watching(pid)).unwrap();
                        exec_running = true;
                        break;
                    }
                }

                if exec_running {
                    break;
                }

                sys.refresh_processes();
            }

            match child_process.wait_with_output() {
                Ok(output) => {
                    if let Some(status_code) = output.status.code() {
                        if status_code == 0 {
                            // Application was closed
                            dir_event.send(WatcherEvent::Exit).unwrap();
                        } else {
                            // Application was terminated
                            dir_event.send(WatcherEvent::Starting).unwrap();
                        }
                    }
                }
                Err(e) => dir_event.send(WatcherEvent::Error(e.to_string())).unwrap(),
            }
        }
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq)]
enum WatcherEvent {
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
