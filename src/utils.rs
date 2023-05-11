use crate::{Files, WatcherEvent};
use std::{sync::mpsc::Sender, time::Duration};
use sysinfo::{ProcessExt, SystemExt};

/// A function to scan directories recursively
///
/// # Arguments
/// * `ignored_paths` - a Vec of Paths to ignore
/// * `file` - a Path of the file/folder to check currently
/// * `cb` - a callback function to run when the scan finds a file
pub fn visit_dirs(
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
pub fn grab_directory_and_files(dir_path: String) -> Result<Vec<Files>, std::io::Error> {
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
            time: item
                .metadata()
                .expect("Could not get file metadata.")
                .modified()
                .expect("Could not get file metadata for time modified."),
        })
        .collect();

    Ok(file_metadata)
}

/// A function to compare two Vecs of a specific type
///
/// # Arguments
/// * `list` - A vec of desired type
/// * `comparison_list` - A vec of desired type to compare against
pub fn get_list_differences<Item: PartialEq>(
    list: Vec<Item>,
    comparison_list: Vec<Item>,
) -> Result<Vec<Item>, std::io::Error> {
    let changes = list
        .into_iter()
        .filter(|item| {
            if comparison_list.contains(item) {
                false
            } else {
                true
            }
        })
        .collect();

    Ok(changes)
}

/// A directory scanning service that waits for changes
///
/// # Arguments
/// * `dir_event` - an MSPC Sender of type WatcherEvent
/// * `dir_path` - a String representation of a directory path
pub fn dir_watcher(dir_path: String, event: Sender<WatcherEvent>) -> Result<(), std::io::Error> {
    let file_names = grab_directory_and_files(dir_path.clone())
        .expect("Could not retrieve files from Directory.");

    loop {
        let file_names_reloaded = grab_directory_and_files(dir_path.clone())
            .expect("Could not retrieve files from Directory.");

        let changes = get_list_differences(file_names_reloaded.clone(), file_names.clone())
            .expect("Couldn't get file differences, check permissions.");

        if changes.len() > 0 {
            event
                .send(WatcherEvent::FileChanged(changes))
                .expect("Could not send event.");
            break;
        }

        std::thread::sleep(Duration::from_millis(1000));
    }

    Ok(())
}

/// A constant command running service
///
/// # Arguments
/// * `dir_event` - an MSPC Sender of type WatcherEvent
/// * `dir_cmd` - the command to run, which will respawn on executable termination
/// * `dir_path` - a String representation of a directory path
pub fn cmd_runner(dir_path: String, event: Sender<WatcherEvent>) -> Result<(), std::io::Error> {
    if cfg!(target_os = "windows") {
        loop {
            // Generate Cargo Run process
            let child_process = std::process::Command::new("cargo")
                // .current_dir()
                .env("PATH", dir_path.clone())
                .args(["run"])
                .spawn()
                .expect("Could not create child process from given command.");

            // scan and find executable
            let mut exe_name = String::new();
            for entry in std::fs::read_dir(dir_path.clone() + "/target/debug")
                .expect("Couldn't search directory for executables")
            {
                if let Some(found_file) = entry.expect("Could not find file.").file_name().to_str()
                {
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
                        event
                            .send(WatcherEvent::Watching(pid))
                            .expect("Could not send event.");
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
                            event
                                .send(WatcherEvent::Exit)
                                .expect("Could not send event.");
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
        }
    }
    Ok(())
}
