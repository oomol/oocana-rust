#[cfg(test)]
mod tests {
    use utils::config::load_config;

    #[test]
    fn test_default_config() {
        let config = load_config(None::<String>);
        assert!(config.is_ok(), "Error: {:?}", config.unwrap_err());

        let config = config.unwrap();
        let home_dir = dirs::home_dir().unwrap().to_string_lossy().to_string();
        let global = config.global.clone();

        assert_eq!(
            global.store_dir.clone(),
            format!("{}/.oomol-studio/oocana", home_dir)
        );
        assert_eq!(global.oocana_dir.clone(), format!("{}/.oocana", home_dir));
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
            global.store_dir.clone(),
            format!("{}/.oomol-studio/oocana", home_dir)
        );
        assert_eq!(global.oocana_dir.clone(), format!("{}/.oocana", home_dir));
        assert_eq!(
            global.env_file.clone(),
            Some(format!("{}/.oocana/.env", home_dir))
        );
        assert_eq!(
            global.bind_path_file.clone(),
            Some(format!("{}/bind_path.env", home_dir))
        );

        assert_eq!(global.search_paths.is_some(), true);
        assert_eq!(global.search_paths.unwrap().len(), 0);

        let run = config.run.clone();

        assert_eq!(run.exclude_packages.is_some(), true);
        assert_eq!(run.exclude_packages.unwrap().len(), 0);

        let extra = run.extra.clone().unwrap();
        assert_eq!(extra.search_paths.is_some(), true);
        assert_eq!(extra.search_paths.unwrap().len(), 0);
    }
}
