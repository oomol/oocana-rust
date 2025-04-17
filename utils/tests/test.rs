#[cfg(test)]
mod tests {
    use utils::{config::load_config, env::load_env_from_file};

    #[test]
    fn test_default_config() {
        let config = load_config(None::<String>);
        assert!(config.is_ok(), "Error: {:?}", config.unwrap_err());

        let config = config.unwrap();
        let home_dir = dirs::home_dir().unwrap().to_string_lossy().to_string();
        let global = config.global;

        assert_eq!(
            global.store_dir,
            format!("{}/.oomol-studio/oocana", home_dir)
        );
        assert_eq!(global.oocana_dir, format!("{}/.oocana", home_dir));
    }

    #[test]
    fn test_load_config() {
        let config = load_config(Some("tests/sample"));
        assert!(config.is_ok(), "Error: {:?}", config.unwrap_err());

        let config = load_config(Some("tests/sample.toml"));
        assert!(config.is_ok(), "Error: {:?}", config.unwrap_err());

        let config = config.unwrap();
        let home_dir = dirs::home_dir().unwrap().to_string_lossy().to_string();
        let global = config.global.clone();
        assert_eq!(
            global.store_dir,
            format!("{}/.oomol-studio/oocana", home_dir)
        );
        assert_eq!(global.oocana_dir, format!("{}/.oocana", home_dir));
        assert_eq!(
            global.env_file,
            Some(format!("{}/.oocana/.env", home_dir))
        );
        assert_eq!(
            global.bind_path_file,
            Some(format!("{}/bind_path.env", home_dir))
        );

        assert!(global.search_paths.is_some());
        assert_eq!(global.search_paths.unwrap().len(), 0);

        let run = config.run;

        assert!(run.exclude_packages.is_some());
        assert_eq!(run.exclude_packages.unwrap().len(), 0);

        let extra = run.extra.unwrap();
        assert!(extra.search_paths.is_some());
        assert_eq!(extra.search_paths.unwrap().len(), 0);
    }

    #[test]
    fn test_load_env_from_file() {
        let file_path = Some("tests/test.env");
        let env_vars = load_env_from_file(&file_path);
        assert!(!env_vars.is_empty());
        assert_eq!(env_vars.get("TEST_KEY"), Some(&"TEST_VALUE".to_string()));
    }
}
