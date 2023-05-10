#[cfg(test)]
mod tests {

    use std::{fs::DirEntry, time::SystemTime};
    use watchrs::{
        utils::{self, visit_dirs},
        Files, WatchRs, WatcherEvent,
    };

    #[test]
    fn watch_rs_setup() {
        // Expect default startup params to be correct
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
        let current_path = std::env::current_dir()
            .unwrap()
            .to_string_lossy()
            .to_string();

        let test_path = format!("{}/tests/temp", current_path);
        std::fs::create_dir_all(format!("{}/ignore_me", test_path)).unwrap();

        std::fs::File::create(format!("{}/test.txt", test_path)).unwrap();
        std::fs::File::create(format!("{}/ignore_me/test2.txt", test_path)).unwrap();

        let mut dir_contents = Vec::<DirEntry>::new();
        visit_dirs(
            Vec::<&std::path::Path>::new(),
            std::path::Path::new(&test_path),
            &mut |file| {
                dir_contents.push(file);
            },
        )
        .unwrap();

        std::fs::remove_dir_all(test_path).unwrap();

        assert_eq!(dir_contents.len(), 2)
    }

    #[test]
    fn watch_rs_file_changes() {
        // Detect new files
        let files = vec![
            Files {
                name: String::from("test.txt"),
                path: String::from("./test.txt"),
                time: SystemTime::now(),
            },
            Files {
                name: String::from("test2.txt"),
                path: String::from("./test2.txt"),
                time: SystemTime::now(),
            },
        ];

        let updated_files = vec![
            files[0].clone(),
            files[1].clone(),
            Files {
                name: String::from("test3.txt"),
                path: String::from("./test3.txt"),
                time: SystemTime::now(),
            },
        ];

        let expected_result = vec![updated_files[2].clone()];
        let actual_result = utils::get_list_differences(updated_files, files.clone()).unwrap();
        assert_eq!(actual_result, expected_result);

        // Detect timestamp changes
        let mut file_date_changed = files[1].clone();
        file_date_changed.time = SystemTime::now();
        let updated_files = vec![files[0].clone(), file_date_changed];

        let expected_result = vec![updated_files[1].clone()];
        let actual_result = utils::get_list_differences(updated_files, files.clone()).unwrap();
        assert_eq!(actual_result, expected_result);
    }
}
