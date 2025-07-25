use serde::Deserialize;

use crate::{utils, Files, WatcherEvent};
use std::time::Duration;

#[derive(Deserialize)]
struct CargoPackage {
    name: String,
}

#[derive(Deserialize)]
struct CargoTOML {
    package: CargoPackage,
}

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
    loop {
        // Generate Cargo Run process
        let child_process = std::process::Command::new("cargo").args(["run"]).spawn()?;

        match std::fs::read_to_string(dir_path.clone() + "/Cargo.toml") {
            Ok(cargo_file) => {
                let config: CargoTOML = toml::from_str(&cargo_file).unwrap();
                let mut sys = sysinfo::System::new();

                loop {
                    std::thread::sleep(Duration::from_millis(100));
                    let processes: Vec<(&sysinfo::Pid, &sysinfo::Process)> = sys
                        .processes()
                        .iter()
                        .filter(|(_pid, process)| {
                            *config.package.name == *process.name().to_owned()
                        })
                        .collect();

                    if processes.len() == 1 {
                        dir_event
                            .send(WatcherEvent::Watching(child_process.id()))
                            .unwrap();
                        break;
                    }

                    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
                }
            }
            Err(e) => panic!("TOML File is invalid! {}", e),
        }

        match child_process.wait_with_output() {
            Ok(output) => {
                if let Some(status_code) = output.status.code() {
                    if status_code == 0 {
                        // Application was closed
                        dir_event.send(WatcherEvent::Exit).unwrap();
                        break;
                    } else {
                        // Application was terminated
                        dir_event.send(WatcherEvent::Starting).unwrap();
                        break;
                    }
                }
            }
            Err(e) => {
                dir_event.send(WatcherEvent::Error(e.to_string())).unwrap();
                break;
            }
        }
    }

    Ok(())
}
