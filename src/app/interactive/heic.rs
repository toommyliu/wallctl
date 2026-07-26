use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use inquire::validator::Validation;
use inquire::{Confirm, Text};

use super::App;
use crate::cli::HeicArgs;
use crate::clock::Clock;
use crate::runner::CommandRunner;

impl<R, C> App<R, C>
where
    R: CommandRunner,
    C: Clock,
{
    pub(super) fn prompt_create_heic(&self) -> Result<()> {
        println!("Choose two images with the same dimensions.");
        let light = self.prompt_existing_image("Light appearance image", None)?;
        let dark = self.prompt_existing_image("Dark appearance image", Some(&light))?;
        let output = self.prompt_heic_output()?;
        let force = output.exists()
            && Confirm::new("That file already exists. Replace it?")
                .with_default(false)
                .prompt()
                .context("overwrite confirmation was cancelled")?;
        self.create_heic(&HeicArgs {
            light,
            dark,
            output,
            force,
        })
    }

    fn prompt_existing_image(&self, label: &str, different_from: Option<&Path>) -> Result<PathBuf> {
        let home = self.paths.home.clone();
        let different_from = different_from.map(Path::to_path_buf);
        let value = Text::new(label)
            .with_placeholder("~/Pictures/wallpaper.png")
            .with_validator(move |input: &str| {
                let path = expand_home_path(input.trim(), &home);
                if !path.is_file() {
                    return Ok(Validation::Invalid(
                        "Enter the path to an existing image file".into(),
                    ));
                }
                if !is_supported_image(&path) {
                    return Ok(Validation::Invalid(
                        "Use a JPEG, PNG, HEIC, HEIF, TIFF, or WebP image".into(),
                    ));
                }
                if different_from.as_ref() == Some(&path) {
                    return Ok(Validation::Invalid(
                        "Choose a different image for light and dark appearance".into(),
                    ));
                }
                Ok(Validation::Valid)
            })
            .prompt()
            .with_context(|| format!("{label} prompt was cancelled"))?;
        Ok(expand_home_path(value.trim(), &self.paths.home))
    }

    fn prompt_heic_output(&self) -> Result<PathBuf> {
        let home = self.paths.home.clone();
        let value = Text::new("Save dynamic wallpaper as")
            .with_placeholder("~/Pictures/wallpaper.heic")
            .with_validator(move |input: &str| {
                let path = expand_home_path(input.trim(), &home);
                if path.extension().and_then(|extension| extension.to_str()) == Some("heic") {
                    Ok(Validation::Valid)
                } else {
                    Ok(Validation::Invalid(
                        "The output path must end in .heic".into(),
                    ))
                }
            })
            .prompt()
            .context("output path prompt was cancelled")?;
        Ok(expand_home_path(value.trim(), &self.paths.home))
    }
}

fn is_supported_image(path: &Path) -> bool {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    matches!(
        extension.as_str(),
        "jpg" | "jpeg" | "png" | "heic" | "heif" | "tif" | "tiff" | "webp"
    )
}

fn expand_home_path(value: &str, home: &Path) -> PathBuf {
    if value == "~" {
        return home.to_path_buf();
    }
    if let Some(rest) = value.strip_prefix("~/") {
        return home.join(rest);
    }
    PathBuf::from(value)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{expand_home_path, is_supported_image};

    #[test]
    fn tilde_paths_expand_against_wallctl_home() {
        assert_eq!(
            expand_home_path("~/Pictures/wallpaper.png", Path::new("/Users/test")),
            Path::new("/Users/test/Pictures/wallpaper.png")
        );
    }

    #[test]
    fn supported_image_extensions_are_case_insensitive() {
        assert!(is_supported_image(Path::new("wallpaper.PNG")));
        assert!(is_supported_image(Path::new("wallpaper.heic")));
        assert!(!is_supported_image(Path::new("wallpaper.txt")));
    }
}
