#[cfg(test)]
mod tests {
    use utils::config::load_config;

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
            global.clone().map(|g| g.store_dir.clone()).flatten(),
            Some(format!("{}/.oomol-studio/oocana", home_dir))
        );
        assert_eq!(
            global.clone().map(|g| g.oocana_dir.clone()).flatten(),
            Some(format!("{}/.oocana", home_dir))
        );
        assert_eq!(
            global.clone().map(|g| g.env_file.clone()).flatten(),
            Some(format!("{}/.env", home_dir))
        );
        assert_eq!(
            global.clone().map(|g| g.bind_path_file.clone()).flatten(),
            Some(format!("{}/bind_path.env", home_dir))
        );

        let run = config.run.clone().unwrap();
        assert_eq!(run.search_paths.is_some(), true);
        assert_eq!(run.search_paths.unwrap().len(), 0);

        assert_eq!(run.exclude_packages.is_some(), true);
        assert_eq!(run.exclude_packages.unwrap().len(), 0);

        let extra = run.extra.clone().unwrap();
        assert_eq!(extra.search_paths.is_some(), true);
        assert_eq!(extra.search_paths.unwrap().len(), 0);
    }
}
