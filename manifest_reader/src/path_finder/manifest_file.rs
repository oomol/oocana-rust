use std::path::{Path, PathBuf};

/// find `<basename>.oo.yaml` or `<basename>.oo.yml` file in <dir_path>, return the existing path or None.
pub fn find_oo_yaml_in_dir<P: AsRef<Path>>(dir_path: P, basename: &str) -> Option<PathBuf> {
    if dir_path.as_ref().is_dir() {
        find_oo_yaml_without_oo_suffix(dir_path.as_ref().join(basename))
    } else {
        None
    }
}

/// find `</a/b/c/basename_path>.oo.yaml` or `</a/b/c/basename_path>.oo.yml` file, return the existing path or None.
pub fn find_oo_yaml_without_oo_suffix<P: AsRef<Path>>(
    path_without_oo_suffix: P,
) -> Option<PathBuf> {
    let mut guess_file_path = path_without_oo_suffix.as_ref().with_extension("oo.yaml");
    if guess_file_path.is_file() {
        return Some(guess_file_path);
    }

    guess_file_path.set_extension("yml");
    if guess_file_path.is_file() {
        return Some(guess_file_path);
    }

    None
}

/// if path is a dir, find `<basename>.oo.yaml` or `<basename>.oo.yml` file in the dir.
/// if path is a file, check if it is `<basename>.oo.yaml` or `<basename>.oo.yml`.
/// if path is neither, return None.
pub fn find_oo_yaml<P: AsRef<Path>>(path: P, basename: &str) -> Option<PathBuf> {
    let path = path.as_ref();
    if path.is_dir() {
        return find_oo_yaml_in_dir(path, basename);
    } else if path.is_file() {
        let filename = path.file_name();
        if let Some(name) = filename.and_then(|n| n.to_str()) {
            if name == format!("{basename}.oo.yaml") || name == format!("{basename}.oo.yml") {
                return Some(path.to_path_buf());
            }
        }
        return None;
    } else {
        return None;
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_path_extension() {
        let basename = "test";
        let basename_path = std::path::Path::new(basename);

        let mut guess_file = basename_path.with_extension("oo.yaml");
        assert_eq!(guess_file.to_str().unwrap(), "test.oo.yaml");

        guess_file.set_extension("yml");
        assert_eq!(guess_file.to_str().unwrap(), "test.oo.yml");
    }
}
