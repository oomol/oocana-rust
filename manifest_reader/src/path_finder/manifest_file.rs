use std::path::{Path, PathBuf};

/// find `<basename>.oo.yaml` or `<basename>.oo.yml` file in <dir_path>, return the existing path or None.
pub fn find_oo_yaml_in_dir<P: AsRef<Path>>(dir_path: P, basename: &str) -> Option<PathBuf> {
    if dir_path.as_ref().is_dir() {
        let mut guess_file = dir_path.as_ref().join(format!("{}.oo.yaml", basename));
        if guess_file.is_file() {
            return Some(guess_file);
        }

        guess_file.set_extension("yml");
        if guess_file.is_file() {
            return Some(guess_file);
        }
    }

    None
}

//  find `<basename_path>.oo.yaml` or `<basename_path>.oo.yml` file, return the existing path or None.
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

pub fn find_oo_yaml<P: AsRef<Path>>(file_or_dir_path: P, basename: &str) -> Option<PathBuf> {
    if file_or_dir_path.as_ref().is_file() {
        Some(file_or_dir_path.as_ref().to_path_buf())
    } else {
        find_oo_yaml_in_dir(file_or_dir_path.as_ref(), basename)
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
