use crate::Files;
use std::{process::Child, time::Duration};
use sysinfo::{PidExt, ProcessExt, SystemExt};

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
/// Checks on a variable Duration
///
/// # Arguments
/// * `dir_path` - a String representation of a directory path
/// * `interval` - a custom duration to check directory on
pub fn dir_watcher(dir_path: String, interval: Duration) -> Result<Vec<Files>, std::io::Error> {
    let file_names = grab_directory_and_files(dir_path.clone())
        .expect("Could not retrieve files from Directory.");
    let changes = loop {
        let file_names_reloaded = grab_directory_and_files(dir_path.clone())
            .expect("Could not retrieve files from Directory.");

        let changes = get_list_differences(file_names_reloaded.clone(), file_names.clone())
            .expect("Couldn't get file differences, check permissions.");

        if changes.len() > 0 {
            break changes;
        }

        std::thread::sleep(interval);
    };

    Ok(changes)
}

/// A function to retreive the executable name using the given output directory
///
/// TODO: Allow dynamic target directory
///
/// # Arguments
/// * `dir_path` - The directory to search for executables
pub fn get_executable_from_dir(dir_path: String) -> Result<Vec<String>, std::io::Error> {
    let mut exe_names = Vec::<String>::new();
    for entry in std::fs::read_dir(dir_path.clone() + "/target/debug")
        .expect("Couldn't search directory for executables")
    {
        if let Some(found_file) = entry.expect("Could not find file.").file_name().to_str() {
            if cfg!(target_os = "windows") {
                if found_file.contains(".exe") {
                    exe_names.push(found_file.to_string());
                }
            }
        }
    }

    Ok(exe_names)
}

/// A function to retreive the process ID by the name of the executable
///
/// # Arguments
/// * `exe_names` - String notation of the executable name e.g. watchrs.exe
pub fn get_executable_id(exe_names: Vec<String>) -> Result<sysinfo::Pid, std::io::Error> {
    let mut sys = sysinfo::System::new();
    let mut exec_running = false;
    let pid = loop {
        let mut process_id = sysinfo::Pid::from_u32(0);
        for (pid, process) in sys.processes() {
            for exe_name in exe_names.clone() {
                if exe_name == process.name().to_owned() {
                    exec_running = true;
                    process_id = pid.to_owned();
                    break;
                }
            }
        }

        if exec_running {
            break process_id;
        }

        sys.refresh_processes();
        std::thread::sleep(Duration::from_millis(200));
    };

    Ok(pid)
}

/// A command running service that runs `cargo run` and returns the process ID & name
///
/// # Arguments
/// * `dir_path` - a String representation of a directory path
pub fn cmd_runner(dir_path: String) -> Result<(Child, sysinfo::Pid, Vec<String>), std::io::Error> {
    // Generate Cargo Run process
    let child_process = std::process::Command::new("cargo")
        .args(["run"])
        .spawn()
        .expect("Could not create child process from given command.");

    // Scan and find Executable name
    let exe_names =
        get_executable_from_dir(dir_path.clone()).expect("Couldn't get executable name.");

    // Scan and find Process ID
    let pid = get_executable_id(exe_names.clone())
        .expect("Couldn't retrieve process ID from executable name.");

    Ok((child_process, pid, exe_names))
}
