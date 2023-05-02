use crate::Files;

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
            time: item.metadata().unwrap().modified().unwrap(),
        })
        .collect();

    Ok(file_metadata)
}
