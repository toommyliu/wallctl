use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LightDarkHeicSpec {
    pub light: PathBuf,
    pub dark: PathBuf,
    pub output: PathBuf,
    pub force: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HeicReport {
    pub light: PathBuf,
    pub dark: PathBuf,
    pub output: PathBuf,
}

impl LightDarkHeicSpec {
    pub fn validate(&self) -> Result<()> {
        validate_input_image("light", &self.light)?;
        validate_input_image("dark", &self.dark)?;

        if self.light == self.dark {
            bail!("light and dark images must be different paths");
        }

        if self.output.exists() && !self.force {
            bail!(
                "output already exists: {}. Pass --force to replace it",
                self.output.display()
            );
        }

        if self.output.extension().and_then(|ext| ext.to_str()) != Some("heic") {
            bail!("output path must end in .heic: {}", self.output.display());
        }

        if let Some(parent) = self.output.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("failed to create {}", parent.display()))?;
            }
        }

        Ok(())
    }
}

pub fn create_light_dark_heic(spec: LightDarkHeicSpec) -> Result<HeicReport> {
    spec.validate()?;

    let script_path = write_swift_script()?;
    let cache_dir =
        std::env::temp_dir().join(format!("wallctl-swift-cache-{}", std::process::id()));
    fs::create_dir_all(&cache_dir)
        .with_context(|| format!("failed to create {}", cache_dir.display()))?;

    let output = Command::new("xcrun")
        .arg("swift")
        .arg(&script_path)
        .arg(&spec.light)
        .arg(&spec.dark)
        .arg(&spec.output)
        .env("CLANG_MODULE_CACHE_PATH", &cache_dir)
        .env("SWIFT_MODULE_CACHE_PATH", &cache_dir)
        .output()
        .context("failed to run xcrun swift for dynamic HEIC generation")?;

    let _ = fs::remove_file(&script_path);

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "dynamic HEIC generation failed\nstdout: {}\nstderr: {}",
            stdout.trim(),
            stderr.trim()
        );
    }

    if !spec.output.is_file() {
        bail!(
            "dynamic HEIC generation completed but output was not created: {}",
            spec.output.display()
        );
    }

    Ok(HeicReport {
        light: spec.light,
        dark: spec.dark,
        output: spec.output,
    })
}

fn validate_input_image(label: &str, path: &Path) -> Result<()> {
    if !path.is_file() {
        bail!(
            "{label} image does not exist or is not a file: {}",
            path.display()
        );
    }

    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !matches!(
        extension.as_str(),
        "jpg" | "jpeg" | "png" | "heic" | "heif" | "tif" | "tiff" | "webp"
    ) {
        bail!(
            "{label} image has unsupported extension '.{}': {}",
            extension,
            path.display()
        );
    }

    Ok(())
}

fn write_swift_script() -> Result<PathBuf> {
    let path =
        std::env::temp_dir().join(format!("wallctl-dynamic-heic-{}.swift", std::process::id()));
    fs::write(&path, SWIFT_DYNAMIC_HEIC_SCRIPT)
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(path)
}

const SWIFT_DYNAMIC_HEIC_SCRIPT: &str = r#"
import AppKit
import AVFoundation
import Foundation
import ImageIO

struct Appearance: Codable {
    let d: Int
    let l: Int
}

extension NSImage {
    var wallctlCGImage: CGImage? {
        var rect = CGRect(origin: .zero, size: self.size)
        return self.cgImage(forProposedRect: &rect, context: nil, hints: nil)
    }
}

func fail(_ message: String) -> Never {
    fputs("wallctl-heic: \(message)\n", stderr)
    exit(1)
}

func loadImage(_ path: String, label: String) -> CGImage {
    let url = URL(fileURLWithPath: path)
    guard let image = NSImage(contentsOf: url) else {
        fail("could not read \(label) image: \(path)")
    }
    guard let cgImage = image.wallctlCGImage else {
        fail("could not decode \(label) image as CGImage: \(path)")
    }
    return cgImage
}

func appearanceMetadata() -> CGMutableImageMetadata {
    let metadata = CGImageMetadataCreateMutable()
    let namespace = "http://ns.apple.com/namespace/1.0/" as CFString
    let prefix = "apple_desktop" as CFString

    guard CGImageMetadataRegisterNamespaceForPrefix(metadata, namespace, prefix, nil) else {
        fail("could not register Apple desktop metadata namespace")
    }

    let encoder = PropertyListEncoder()
    encoder.outputFormat = .binary

    guard let plist = try? encoder.encode(Appearance(d: 1, l: 0)) else {
        fail("could not encode appearance metadata")
    }

    let base64 = plist.base64EncodedString() as CFString
    guard let tag = CGImageMetadataTagCreate(
        namespace,
        prefix,
        "apr" as CFString,
        CGImageMetadataType.string,
        base64
    ) else {
        fail("could not create appearance metadata tag")
    }

    guard CGImageMetadataSetTagWithPath(metadata, nil, "apple_desktop:apr" as CFString, tag) else {
        fail("could not attach appearance metadata tag")
    }

    return metadata
}

let args = CommandLine.arguments
guard args.count == 4 else {
    fail("usage: swift wallctl-dynamic-heic.swift <light> <dark> <output>")
}

let light = loadImage(args[1], label: "light")
let dark = loadImage(args[2], label: "dark")
let outputURL = URL(fileURLWithPath: args[3])

guard light.width == dark.width && light.height == dark.height else {
    fail("light and dark images must have the same dimensions; got \(light.width)x\(light.height) and \(dark.width)x\(dark.height)")
}

guard let destination = CGImageDestinationCreateWithURL(
    outputURL as CFURL,
    AVFileType.heic as CFString,
    2,
    nil
) else {
    fail("could not create HEIC destination: \(args[3])")
}

let options = [kCGImageDestinationLossyCompressionQuality as String: 1.0] as CFDictionary
CGImageDestinationAddImageAndMetadata(destination, light, appearanceMetadata(), options)
CGImageDestinationAddImage(destination, dark, options)

guard CGImageDestinationFinalize(destination) else {
    fail("could not finalize HEIC output: \(args[3])")
}

guard let source = CGImageSourceCreateWithURL(outputURL as CFURL, nil) else {
    fail("could not reopen generated HEIC: \(args[3])")
}

guard CGImageSourceGetCount(source) == 2 else {
    fail("generated HEIC should contain 2 images but contains \(CGImageSourceGetCount(source))")
}
"#;

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::LightDarkHeicSpec;

    #[test]
    fn rejects_missing_inputs() {
        let tmp = TempDir::new().unwrap();
        let spec = LightDarkHeicSpec {
            light: tmp.path().join("light.png"),
            dark: tmp.path().join("dark.png"),
            output: tmp.path().join("wallpaper.heic"),
            force: false,
        };

        assert!(spec.validate().is_err());
    }

    #[test]
    fn rejects_existing_output_without_force() {
        let tmp = TempDir::new().unwrap();
        let light = tmp.path().join("light.png");
        let dark = tmp.path().join("dark.png");
        let output = tmp.path().join("wallpaper.heic");
        fs::write(&light, b"not decoded in validation").unwrap();
        fs::write(&dark, b"not decoded in validation").unwrap();
        fs::write(&output, b"existing").unwrap();

        let spec = LightDarkHeicSpec {
            light,
            dark,
            output,
            force: false,
        };

        assert!(spec.validate().is_err());
    }

    #[test]
    fn accepts_supported_inputs_with_force() {
        let tmp = TempDir::new().unwrap();
        let light = tmp.path().join("light.jpg");
        let dark = tmp.path().join("dark.png");
        let output = tmp.path().join("wallpaper.heic");
        fs::write(&light, b"not decoded in validation").unwrap();
        fs::write(&dark, b"not decoded in validation").unwrap();
        fs::write(&output, b"existing").unwrap();

        let spec = LightDarkHeicSpec {
            light,
            dark,
            output,
            force: true,
        };

        spec.validate().unwrap();
    }
}
