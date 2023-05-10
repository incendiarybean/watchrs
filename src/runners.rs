use crate::{
    utils::{self, get_list_differences},
    WatcherEvent,
};
use std::{sync::mpsc::Sender, time::Duration};
use sysinfo::{ProcessExt, SystemExt};

/// A constant directory scanning service
///
/// # Arguments
/// * `dir_event` - an MSPC Sender of type WatcherEvent
/// * `dir_path` - a String representation of a directory path
pub fn dir_runner(dir_path: String, event: Sender<WatcherEvent>) -> Result<(), std::io::Error> {
    let mut file_names = utils::grab_directory_and_files(dir_path.clone())
        .expect("Could not retrieve files from Directory.");

    loop {
        let file_names_reloaded = utils::grab_directory_and_files(dir_path.clone())
            .expect("Could not retrieve files from Directory.");

        let changes = get_list_differences(file_names_reloaded.clone(), file_names.clone())
            .expect("Couldn't get file differences, check permissions.");

        if changes.len() > 0 {
            event.send(WatcherEvent::FileChanged(changes)).unwrap();
            file_names = file_names_reloaded;
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
pub fn cmd_runner(dir_path: String, event: Sender<WatcherEvent>) {
    if cfg!(target_os = "windows") {
        loop {
            // Generate Cargo Run process
            let child_process = std::process::Command::new("cargo")
                .args(["run"])
                .spawn()
                .expect("Could not create child process from given command.");

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
                        event.send(WatcherEvent::Watching(pid)).unwrap();
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
                            event.send(WatcherEvent::Exit).unwrap();
                        } else {
                            // Application was terminated
                            event.send(WatcherEvent::Starting).unwrap();
                        }
                    }
                }
                Err(e) => event.send(WatcherEvent::Error(e.to_string())).unwrap(),
            }
        }
    }
}
