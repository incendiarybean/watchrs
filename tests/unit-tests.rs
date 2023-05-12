#[cfg(test)]
mod tests {

    use futures::executor::block_on;
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

    /// Function to generate the test folders in the given path.
    /// We use a given path so each test has its own temporary folder.
    /// This stops the tests from trying to access a directory that may have been recently deleted while running in parallel.
    ///
    /// # Arguments
    /// * `path` - String notation of the temporary folder to create
    fn generate_test_files(path: String) -> Result<(String, Vec<std::fs::File>), std::io::Error> {
        let test_path = format!(
            "{}\\tests\\{}",
            std::env::current_dir().unwrap().to_string_lossy(),
            path
        );
        let test_folder_0 = std::fs::create_dir_all(format!("{}\\src", test_path));
        let test_folder_1 = std::fs::create_dir_all(format!("{}\\target\\debug", test_path));
        let test_file_0 = std::fs::File::create(format!("{}\\src\\test_0.txt", test_path));
        let test_file_1 = std::fs::File::create(format!("{}\\src\\test_1.txt", test_path));
        let test_exe_0 =
            std::fs::File::create(format!("{}\\target\\debug\\test_exe_0.exe", test_path));

        assert!(test_folder_0.is_ok());
        assert!(test_folder_1.is_ok());
        assert!(test_file_0.is_ok());
        assert!(test_file_1.is_ok());
        assert!(test_exe_0.is_ok());

        Ok((test_path, vec![test_file_0.unwrap(), test_file_1.unwrap()]))
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
        let (test_path, _files) = generate_test_files(String::from("tmp-discovery"))
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

        assert_eq!(actual_result.len(), 3);

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

        assert_eq!(actual_result.len(), 2);
    }

    #[test]
    fn watch_rs_file_formatter() {
        // Test setup
        let (test_path, files) = generate_test_files(String::from("tmp-formatter"))
            .expect("Couldn't create test files!");

        let mut actual_result = grab_directory_and_files(test_path.clone()).unwrap();

        let mut expected_result = vec![
            Files {
                name: String::from("test_0.txt"),
                path: format!("{}\\src\\test_0.txt", test_path),
                time: files[0].metadata().unwrap().modified().unwrap(),
            },
            Files {
                name: String::from("test_1.txt"),
                path: format!("{}\\src\\test_1.txt", test_path),
                time: files[1].metadata().unwrap().modified().unwrap(),
            },
        ];

        // Clear files before assertion, in case assertion
        cleanup_test_files(test_path).expect("Couldn't clean up files!");

        assert_eq!(actual_result.len(), 2);
        assert_eq!(
            actual_result.sort_by_key(|item| item.name.clone()),
            expected_result.sort_by_key(|item| item.name.clone())
        );
    }

    #[test]
    fn watch_rs_list_comparison() {
        // Check new files are detected
        let mut files = Vec::<Files>::new();
        for n in 1..12000 {
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
        let (test_path, _files) = generate_test_files(String::from("tmp-dir-runner"))
            .expect("Couldn't create test files!");

        let (sender, receiver) = std::sync::mpsc::channel::<WatcherEvent>();

        let thread_path_clone = test_path.clone();
        let worker = std::thread::spawn(move || utils::dir_watcher(thread_path_clone, sender));

        // Wait a moment, to ensure that files have been collected first
        std::thread::sleep(Duration::from_millis(500));

        let new_test_file_path = format!("{}\\test_2.txt", test_path);
        let test_file_2 =
            std::fs::File::create(new_test_file_path).expect("Couldn't create test file 2.");
        let expected_result = vec![Files {
            name: String::from("test_2.txt"),
            path: format!("{}\\test_2.txt", test_path),
            time: test_file_2.metadata().unwrap().modified().unwrap(),
        }];

        match receiver.recv() {
            Ok(event) => match event {
                WatcherEvent::FileChanged(actual_result) => {
                    // Clear files before assertion, in case assertion
                    cleanup_test_files(test_path).expect("Couldn't clean up files!");

                    assert_eq!(actual_result.len(), 1);
                    assert_eq!(actual_result, expected_result);
                }
                _ => panic!(),
            },
            _ => panic!(),
        }

        assert!(worker.join().is_ok());
    }

    #[test]
    fn watch_rs_get_exe_from_dir() {
        // Test setup
        let (test_path, _files) = generate_test_files(String::from("tmp-exe-dir-finder"))
            .expect("Couldn't create test files!");

        // Check that an executable name is returned from a valid build directory
        block_on(async {
            let exe_name = get_executable_from_dir(test_path.clone()).await.unwrap();

            assert_eq!(exe_name, String::from("test_exe_0.exe"));
        });

        // Clear files before assertion, in case assertion
        cleanup_test_files(test_path).expect("Couldn't clean up files!");
    }

    #[test]
    fn watch_rs_test_name() {
        // Test setup
        let (test_path, _files) = generate_test_files(String::from("tmp-pid-finder"))
            .expect("Couldn't create test files!");

        // Check that a PID is returned when supplied a valid running process name
        block_on(async {
            let pid = get_executable_id(String::from("cargo.exe")).await.unwrap();

            assert_ne!(pid, sysinfo::Pid::from(0));
        });

        // Clear files before assertion, in case assertion
        cleanup_test_files(test_path).expect("Couldn't clean up files!");
    }

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
}
