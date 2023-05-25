pub mod utils;

use crossterm::style::Stylize;
use std::{
    sync::mpsc::{Receiver, Sender},
    time::{Duration, SystemTime},
};
use sysinfo::{ProcessExt, System, SystemExt};
use utils::logger_utils::logger;

#[derive(PartialEq, PartialOrd, Clone, Debug)]
pub struct Files {
    pub name: String,
    pub path: String,
    pub time: SystemTime,
    pub extension: String,
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
    pub ignore_paths: Vec<std::path::PathBuf>,
    pub file_types: Vec<String>,
    pub event: Sender<WatcherEvent>,

    watcher: Receiver<WatcherEvent>,

    // Debug
    debug: bool,
    reload: bool,
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
            dir_path: dir_path.clone(),
            ignore_paths: vec![format!("{}/target", dir_path).into()],
            file_types: Vec::<String>::new(),
            event: event_sender,

            watcher: event_receiver,

            // Debug
            debug: false,
            reload: true,
        }
    }
}

impl WatchRs {
    /// Launches an instance of WatchRS
    pub fn begin_watching(mut self) -> Result<(), std::io::Error> {
        // Validate passed in arguments
        self.process_args();

        logger::clear();
        logger::debug(format!(
            "{}: {},\n{}: {:?}\n{}: {}\n{}: {}\n\n",
            "dir_path".magenta(),
            self.dir_path.clone().green(),
            "ignore_paths".magenta(),
            self.ignore_paths,
            "file_types".magenta(),
            self.file_types.join(", ").green(),
            "reload".magenta(),
            self.reload.to_string().green()
        ));

        logger::info(format!("{}", "Waiting for initialisation!".cyan()));

        // Start watching directories
        self.spawn_directory_watcher();

        // Start reload process if allowed
        if self.reload {
            self.spawn_command_runner();
        }

        // Handle events
        self.event_handler();

        Ok(())
    }

    /// Create directory watcher
    /// Watches directory and sends event on changes
    fn spawn_directory_watcher(&self) {
        let path = self.dir_path.clone();
        let ignore_paths = self.ignore_paths.clone();
        let watch_types = self.file_types.clone();
        self.event
            .send(WatcherEvent::Starting)
            .expect("Could not send event.");

        let event = self.event.clone();
        std::thread::Builder::new()
            .name("DirWatcher".to_string())
            .spawn(move || {
                let file_changes = utils::watcher_utils::dir_watcher(
                    path,
                    ignore_paths,
                    watch_types,
                    Duration::from_millis(1000),
                )
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
                    utils::watcher_utils::cmd_runner(path.clone())
                        .expect("Could not run command successfully.");

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
        loop {
            match self.watcher.recv() {
                Ok(event) => match event {
                    WatcherEvent::Watching(process_id, exe_names) => {
                        self.process_id = Some(process_id);

                        logger::clear();
                        logger::info(format!(
                            "{}: {}\n{}: {}",
                            "Process ID".cyan(),
                            process_id.to_string().green(),
                            "Executable".cyan(),
                            exe_names.join(", ").green()
                        ));

                        if exe_names.len() > 1 {
                            logger::warning(
                                format!(
                                    "{}\n{}\n{}\n", 
                                    "WARNING: Expected 1 platform associated executable but found multiple.",
                                    "Has this project been renamed?",
                                    "If you encounter issues, remove the excess executables in the ./target/debug folder."
                                )
                            );
                        }

                        logger::info(format!(
                            "{}:\n  {}\n\n{}",
                            "Watching directory for changes".cyan(),
                            self.dir_path.clone().dark_yellow(),
                            "Application is ready to reload.".cyan()
                        ));
                    }
                    WatcherEvent::FileChanged(files) => {
                        let file_list: Vec<String> =
                            files.iter().map(|file| file.name.clone()).collect();

                        logger::clear();
                        logger::info(format!(
                            "{}:\n   {}\n\n{}",
                            "File(s) were changed:".cyan(),
                            file_list.join("\n    ").yellow(),
                            "Reloading application...".cyan()
                        ));

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
                        logger::error(err);
                        std::process::exit(1);
                    }
                    WatcherEvent::Exit => {
                        logger::clear();
                        logger::info(format!("{}", "Exiting program!".cyan()));
                        break;
                    }
                    _ => (),
                },
                Err(_) => (),
            }
        }
    }

    fn process_args(&mut self) -> &Self {
        let mut argument_index = 0;
        let arguments: Vec<String> = std::env::args().collect();
        let arguments_requiring_list = vec![String::from("--ignore"), String::from("--extensions")];

        while argument_index < arguments.len() {
            let argument = &arguments[argument_index];
            let end_of_arguments = argument_index + 1 >= arguments.len();
            let next_argument = if !end_of_arguments {
                arguments[argument_index + 1].clone()
            } else {
                String::new()
            };
            let next_argument_is_flag = next_argument.contains("--");
            let argument_requires_input = arguments_requiring_list.contains(argument);

            // Check if argument has required input
            if argument_requires_input && (next_argument_is_flag || next_argument.is_empty()) {
                logger::error(format!(
                    "Expected comma delimited list for flag: {}",
                    argument,
                ));
                std::process::exit(1);
            }

            match argument.as_str() {
                "--no-reload" => {
                    self.reload = false;
                }
                "--debug" => {
                    self.debug = true;
                }
                "--ignore" => {
                    for path in next_argument.split(",") {
                        self.ignore_paths.push(std::path::PathBuf::from(format!(
                            "{}/{}",
                            self.dir_path, path
                        )));
                    }
                }
                "--extensions" => {
                    for mut extension in next_argument.split(",") {
                        if extension.contains(".") {
                            extension = extension.split(".").collect::<Vec<_>>()[1];
                        }
                        self.file_types.push(extension.to_string());
                    }
                }
                _ => {
                    // Ignore the passed executable argument
                    if !argument.contains("watchrs") {
                        logger::error(format!("Unexpected commandline argument: {}", argument));
                        std::process::exit(1);
                    }
                }
            }

            // Move to next index after matching flag
            argument_index = argument_index + if !argument_requires_input { 1 } else { 2 };
        }
        self
    }
}
