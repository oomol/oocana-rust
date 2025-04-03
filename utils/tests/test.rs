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

        assert_eq!(
            config.global.clone().map(|g| g.store_dir.clone()).flatten(),
            Some("~/.oomol-studio/oocana".to_string())
        );
        assert_eq!(
            config
                .global
                .clone()
                .map(|g| g.oocana_dir.clone())
                .flatten(),
            Some("~/.oocana".to_string())
        );
    }
}
