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

        assert_eq!(
            config.global.clone().map(|g| g.store_dir.clone()).flatten(),
            Some(format!("{}/.oomol-studio/oocana", home_dir))
        );
        assert_eq!(
            config
                .global
                .clone()
                .map(|g| g.oocana_dir.clone())
                .flatten(),
            Some(format!("{}/.oocana", home_dir))
        );
    }
}
