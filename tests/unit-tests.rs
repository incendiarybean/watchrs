#[cfg(test)]
mod tests {
    use std::{
        fs::DirEntry,
        time::{Duration, SystemTime},
    };
    use watchrs::{
        utils::{
            self, get_executable_from_dir, get_executable_id, grab_directory_and_files, visit_dirs,
        },
        Files, WatchRs, WatcherEvent,
    };

    // Test Example
    // #[test]
    // fn watch_rs_test_name() {
    //     // Test setup
    //     let (test_path, _files) = generate_test_files(String::from("TEMPLATE_DIR"))
    //         .expect("Couldn't create test files!");
    //
    //     // Clear files before assertion, in case assertion
    //     cleanup_test_files(test_path).expect("Couldn't clean up files!");
    // }

    /// Function to generate the test folders in the given path.
    /// We use a given path so each test has its own temporary folder.
    /// This stops the tests from trying to access a directory that may have been recently deleted while running in parallel.
    ///
    /// # Arguments
    /// * `path` - String notation of the temporary folder to create
    fn generate_test_files(
        path: String,
        file_count: u8,
    ) -> Result<(String, Vec<std::fs::File>), std::io::Error> {
        let test_path = format!(
            "{}\\tests\\{}",
            std::env::current_dir().unwrap().to_string_lossy(),
            path
        );

        // Generate test folder structure
        let folders = vec![String::from("src"), String::from("target\\debug")];
        for folder in folders {
            let test_folder = std::fs::create_dir_all(format!("{}\\{}", test_path, folder));

            assert!(test_folder.is_ok());
        }

        let mut files = Vec::<std::fs::File>::new();
        for file in 0..file_count {
            let test_file: Result<std::fs::File, std::io::Error> =
                std::fs::File::create(format!("{}\\src\\test_{}.txt", test_path, file));

            assert!(test_file.is_ok());

            files.push(test_file.unwrap());
        }

        // Generate executable files
        let test_exe =
            std::fs::File::create(format!("{}\\target\\debug\\test_exe_0.exe", test_path));

        assert!(test_exe.is_ok());

        Ok((test_path, files))
    }

    /// Function to delete the test folders in the given path.
    ///
    /// # Arguments
    /// * `test_path` - String notation of the test_path returned by generate_test_files
    fn cleanup_test_files(test_path: String) -> Result<(), std::io::Error> {
        let remove_test_folders = std::fs::remove_dir_all(test_path);

        assert!(remove_test_folders.is_ok());

        Ok(())
    }

    #[test]
    fn watch_rs_setup() {
        // Check WatchRS starts with correct defaults
        let watch_rs = WatchRs::default();

        #[derive(PartialEq, Debug)]
        struct TestResult {
            status: WatcherEvent,
            dir_path: String,
            process_id: Option<sysinfo::Pid>,
        }

        let actual_startup_result = TestResult {
            status: watch_rs.status,
            dir_path: watch_rs.dir_path,
            process_id: watch_rs.process_id,
        };

        let expected_startup_result = TestResult {
            status: WatcherEvent::Stopped,
            dir_path: std::env::current_dir()
                .unwrap()
                .to_string_lossy()
                .to_string(),
            process_id: None,
        };

        assert_eq!(actual_startup_result, expected_startup_result);
    }

    #[test]
    fn watch_rs_file_discovery() {
        // Test setup
        let (test_path, _files) = generate_test_files(String::from("tmp-discovery"), 10)
            .expect("Couldn't create test files!");

        // Check WatchRS finds all files in the selected directory
        let mut actual_result = Vec::<DirEntry>::new();
        let ignored_paths = Vec::<&std::path::Path>::new();
        visit_dirs(
            ignored_paths.clone(),
            std::path::Path::new(&test_path),
            &mut |file| {
                actual_result.push(file);
            },
        )
        .unwrap();

        assert_eq!(actual_result.len(), 11);

        // Check WatchRS ignores all files in ignored_paths
        let ignore_folder_path = format!("{}\\target\\debug", test_path);
        let mut actual_result = Vec::<DirEntry>::new();
        let mut ignored_paths = Vec::<&std::path::Path>::new();
        ignored_paths.push(&std::path::Path::new(&ignore_folder_path));

        visit_dirs(
            ignored_paths.clone(),
            std::path::Path::new(&test_path),
            &mut |file| {
                actual_result.push(file);
            },
        )
        .unwrap();

        // Clear files before assertion, in case assertion
        cleanup_test_files(test_path).expect("Couldn't clean up files!");

        assert_eq!(actual_result.len(), 10);
    }

    #[test]
    fn watch_rs_file_formatter() {
        // Test setup
        let file_count = 10;
        let (test_path, files) = generate_test_files(String::from("tmp-formatter"), file_count)
            .expect("Couldn't create test files!");

        let mut actual_result = grab_directory_and_files(test_path.clone()).unwrap();

        let mut expected_result = Vec::<Files>::new();
        for file in 0..file_count {
            expected_result.push(Files {
                name: format!("test_{}.txt", file),
                path: format!("{}\\src\\test_{}.txt", test_path, file),
                time: files[0].metadata().unwrap().modified().unwrap(),
            })
        }

        // Clear files before assertion, in case assertion
        cleanup_test_files(test_path).expect("Couldn't clean up files!");

        assert_eq!(actual_result.len(), 10);
        assert_eq!(
            actual_result.sort_by_key(|item| item.name.clone()),
            expected_result.sort_by_key(|item| item.name.clone())
        );
    }

    #[test]
    fn watch_rs_list_comparison() {
        // Check new files are detected
        let mut files = Vec::<Files>::new();
        for n in 1..100 {
            files.push(Files {
                name: String::from(format!("test_{}.txt", n)),
                path: String::from(format!(".\\test_{}.txt", n)),
                time: SystemTime::now(),
            })
        }

        // Check that an executable name is returned from a valid build directory
        let mut updated_files = files.clone();
        updated_files.push(Files {
            name: String::from(format!("test_{}.txt", files.len() + 1)),
            path: String::from(format!(".\\test_{}.txt", files.len() + 1)),
            time: SystemTime::now(),
        });
        let mut expected_result = Vec::<Files>::new();
        if let Some(last_file) = updated_files.last() {
            expected_result.push(last_file.clone());
        }
        let actual_result = utils::get_list_differences(updated_files, files.clone()).unwrap();

        assert_eq!(actual_result, expected_result);

        // Check timestamp changes are detected
        let mut file_date_changed = files[1].clone();
        file_date_changed.time = SystemTime::now();
        let updated_files = vec![files[0].clone(), file_date_changed];
        let expected_result = vec![updated_files[1].clone()];
        let actual_result = utils::get_list_differences(updated_files, files.clone()).unwrap();

        assert_eq!(actual_result, expected_result);
    }

    #[test]
    fn watch_rs_directory_watcher() {
        // Test setup
        let file_count = 10;
        let (test_path, _files) = generate_test_files(String::from("tmp-dir-runner"), file_count)
            .expect("Couldn't create test files!");

        let thread_path_clone = test_path.clone();
        let worker = std::thread::spawn(move || {
            let actual_result =
                utils::dir_watcher(thread_path_clone, Duration::from_millis(1000)).unwrap();

            return actual_result;
        });

        // Wait a moment, to ensure that files have been collected first
        std::thread::sleep(Duration::from_millis(500));

        let new_test_file_path = format!("{}\\test_{}.txt", test_path, file_count + 1);
        let test_file =
            std::fs::File::create(new_test_file_path).expect("Couldn't create test file 2.");
        let expected_result = vec![Files {
            name: format!("test_{}.txt", file_count + 1),
            path: format!("{}\\test_{}.txt", test_path, file_count + 1),
            time: test_file.metadata().unwrap().modified().unwrap(),
        }];
        let actual_result = worker.join().unwrap();

        // Clear files before assertion, in case assertion
        cleanup_test_files(test_path).expect("Couldn't clean up files!");

        assert_eq!(actual_result.len(), 1);
        assert_eq!(actual_result, expected_result);
    }

    #[test]
    fn watch_rs_get_exe_from_dir() {
        // Test setup
        let file_count = 5;
        let (test_path, _files) =
            generate_test_files(String::from("tmp-exe-dir-finder"), file_count)
                .expect("Couldn't create test files!");

        // Check that an executable name is returned from a valid build directory
        let exe_names = get_executable_from_dir(test_path.clone()).unwrap();

        // Clear files before assertion, in case assertion
        cleanup_test_files(test_path).expect("Couldn't clean up files!");

        assert_eq!(exe_names, vec![String::from("test_exe_0.exe")]);
    }

    #[test]
    fn watch_rs_get_pid_from_exe_name() {
        // Test setup
        let file_count = 5;
        let (test_path, _files) = generate_test_files(String::from("tmp-pid-finder"), file_count)
            .expect("Couldn't create test files!");

        // Check that a PID is returned when supplied a valid running process name
        let pid = get_executable_id(vec![String::from("cargo.exe")]).unwrap();

        // Clear files before assertion, in case assertion
        cleanup_test_files(test_path).expect("Couldn't clean up files!");

        assert_ne!(pid, sysinfo::Pid::from(0));
    }
}
