use crate::{utils, Files, WatcherEvent};
use std::time::Duration;
use sysinfo::{ProcessExt, SystemExt};

/// A constant directory scanning service
///
/// # Arguments
/// * `dir_event` - an MSPC Sender of type WatcherEvent
/// * `dir_path` - a String representation of a directory path
pub fn dir_runner(dir_event: std::sync::mpsc::Sender<WatcherEvent>, dir_path: String) {
    let mut file_names = utils::grab_directory_and_files(dir_path.clone())
        .expect("Could not retrieve files from Directory.");

    loop {
        let file_names_reloaded = utils::grab_directory_and_files(dir_path.clone())
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
pub fn cmd_runner(
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
