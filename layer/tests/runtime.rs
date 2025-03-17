#[cfg(test)]
#[cfg(target_os = "linux")]
mod tests {

    use std::env::temp_dir;

    use ctor::ctor;
    use layer::{convert_to_script, *};
    use std::path::PathBuf;
    use tracing;
    use tracing_subscriber;

    fn dirname() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests")
    }

    #[ctor]
    fn init() {
        init_tracing();
    }

    fn init_tracing() {
        let subscriber = tracing_subscriber::fmt::Subscriber::builder()
            .with_max_level(tracing::Level::TRACE)
            .with_file(true)
            .with_line_number(true)
            .finish();
        tracing::subscriber::set_global_default(subscriber)
            .expect("setting default subscriber failed");
    }

    #[test]
    fn test_run_command_api() {
        let d = dirname().join("data").join("simple");
        let r = get_or_create_package_layer(&d, &vec![]);
        assert!(r.is_ok(), "Error: {:?}", r.unwrap_err());

        let runtime_layer = create_runtime_layer(d.to_str().unwrap(), &vec![]);
        assert!(
            runtime_layer.is_ok(),
            "Error: {:?}",
            runtime_layer.unwrap_err()
        );

        let mut runtime_layer = runtime_layer.unwrap();

        // 测试空格有没有正确的被包裹处理

        let mut work_dir = temp_dir();
        work_dir.push("example dir");
        std::fs::create_dir_all(&work_dir).expect("create dir failed");
        work_dir.push("file");
        std::fs::write(&work_dir, "hello aaa").expect("write file failed");
        let bind_dir = work_dir.parent().unwrap().to_str().unwrap().to_string();

        runtime_layer.add_bind_paths(&vec![BindPath {
            source: bind_dir.clone(),
            target: bind_dir.clone(),
        }]);

        let exec_form_cmd = vec!["ls", work_dir.parent().unwrap().to_str().unwrap()];

        let exec_string = convert_to_script(&exec_form_cmd);
        println!("exec_string: {}", exec_string);

        let mut cmd = runtime_layer.run_command(&exec_string);

        let mut log = temp_dir();
        log.push("ovmlayer.log");

        // 把 omvlayer 的日志输出到文件中，避免干扰。
        cmd.env("OVMLAYER_LOG", log.to_str().unwrap());

        let out = cmd.output().expect("run command failed");
        assert_eq!(out.status.success(), true);
        assert_eq!(String::from_utf8(out.stdout).unwrap().trim(), "file");

        std::fs::remove_dir_all(&work_dir.parent().unwrap()).expect("remove dir failed");
    }
}
