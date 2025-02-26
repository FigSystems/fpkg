use anyhow::{anyhow, bail, Context, Result};
use kdl::{KdlDocument, KdlError, KdlIdentifier};
use std::{
    fmt::{self, Display},
    io::BufRead,
    path::Path,
    str::FromStr,
};
use tar::Archive;

use crate::repo::OnlinePackage;

#[derive(Debug, Clone)]
pub struct Dependency {
    pub name: String,
    pub version_mask: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    pub name: String,
    pub version: String,
}

impl Display for Package {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Package: {} {}", self.name, self.version)
    }
}

impl PartialEq for Dependency {
    fn eq(&self, other: &Dependency) -> bool {
        self.name == other.name && self.version_mask == other.version_mask
    }
}

#[derive(Debug)]
pub struct PackageConfig {
    pub name: String,
    pub version: String,
    pub depends: Vec<Dependency>,
}

impl PartialEq for PackageConfig {
    fn eq(&self, other: &PackageConfig) -> bool {
        self.name == other.name
            && self.version == other.version
            && self.depends == other.depends
    }
}

/// Reads a kdl value from a kdl document, bailing if it is not a string or doesn't exist.
fn get_kdl_value_string(doc: &KdlDocument, field: &str) -> Result<String> {
    let field_value = doc.get_arg(&field);
    if let None = field_value {
        bail!("{} does not have an argument", field);
    }
    let field_value = field_value.unwrap().as_string();
    if let None = field_value {
        bail!("{}'s argument is not a string", field);
    }
    Ok(field_value.unwrap().to_string())
}

/// Returns Ok if the package config is valid, and Err if it is not.
pub fn verify_pkg_config(file: &str) -> Result<()> {
    match get_package_config(file) {
        Err(x) => Err(x),
        Ok(_) => Ok(()),
    }
}

/// Parses the package configuration and bails if not valid.
pub fn get_package_config(file: &str) -> Result<PackageConfig> {
    let doc = parse_kdl(file)?;

    let name = get_kdl_value_string(&doc, "name")?;
    let version = get_kdl_value_string(&doc, "version")?;

    let depends = parse_depends(&doc)?;
    Ok(PackageConfig {
        name,
        version,
        depends,
    })
}

/// Parse a kdl document, bailing if invalid
pub fn parse_kdl(file: &str) -> Result<KdlDocument> {
    let doc: Result<KdlDocument, KdlError> = file.parse();
    if let Err(e) = doc {
        let diagnostics = e
            .diagnostics
            .into_iter()
            .map(|x| {
                let a = x.to_string();
                let b = x.help.unwrap_or("None".to_string());
                format!("{} help: {}\n", a, b)
            })
            .collect::<Vec<String>>()
            .concat();
        bail!(
            "Failed to parse KDL document: {}\n\n diagnostics: \n{}",
            file,
            diagnostics
        );
    }
    let doc = doc;
    match doc {
        Ok(x) => Ok(x),
        Err(e) => Err(anyhow!(e)),
    }
}

/// Parse the dependencies from a package configuration kdl document
pub fn parse_depends(doc: &KdlDocument) -> Result<Vec<Dependency>> {
    let mut depends: Vec<Dependency> = Vec::new();
    for node in doc.nodes().into_iter() {
        if node.name().value() == "depends" {
            let name = node.get(0);
            if let None = name {
                bail!("Name not specified for dependency!");
            }
            let name = name.unwrap();

            let version = {
                let mut v = "";
                for ent in node.entries() {
                    if ent.name() == Some(&KdlIdentifier::parse("version")?) {
                        v = ent.value().as_string().ok_or(anyhow!("`version` specifier for dependency is not a string! (Did you forget quotes?)"))?;
                    }
                }
                v
            }.to_string();
            depends.push(Dependency {
                name: name.to_string(),
                version_mask: version,
            });
        }
    }
    Ok(depends)
}

/// Downgrade an OnlinePackage to Package
pub fn onlinepackage_to_package(pkg: &OnlinePackage) -> Package {
    Package {
        name: pkg.name.clone(),
        version: pkg.version.clone(),
    }
}

/// Parses the name and version from a string.
pub fn string_to_package(s: &str) -> Result<Package> {
    let version = s
        .split("-")
        .last()
        .ok_or(anyhow!("Failed to parse version from string {}", s))?;
    pubgrub::version::SemanticVersion::from_str(version)
        .context(anyhow!("Failed to parse version string {}", version))?;
    let name: Vec<String> = s.split("-").map(|x| x.to_string() + "-").collect(); // Collect each segment, adding a '-' after each
    let name = &name[..name.len() - 1].concat(); // Concat each segment
    let name = &name[..name.len() - 1]; // Trim off last '-'
    Ok(Package {
        name: name.to_string(),
        version: version.to_string(),
    })
}

/// Tars the directory and compresses it into a .fpkg
pub fn package_pkg(dir: &Path, out: &Path) -> Result<()> {
    let f = std::fs::File::create(&out)?;

    let mut zstrm =
        zstd::Encoder::new(f, zstd::DEFAULT_COMPRESSION_LEVEL)?.auto_finish();

    let mut tar = tar::Builder::new(&mut zstrm);
    tar.follow_symlinks(false);
    tar.append_dir_all(".", &dir)?;

    tar.finish()?;
    Ok(())
}

/// Decompresses a package from something implementing std::io::Read
pub fn decompress_pkg_read<'a>(
    pkg: impl std::io::Read,
) -> Result<Archive<zstd::Decoder<'a, impl BufRead>>> {
    let zstrm = zstd::Decoder::new(pkg)?;

    let archive = tar::Archive::new(zstrm);
    Ok(archive)
}

#[cfg(test)]
mod tests {
    use crate::pkg::string_to_package;

    use super::*;

    #[test]
    fn get_pkg_config_1() {
        let s = r###"
name "abcd"
version "145.54.12"

depends "coreutils"
depends python {
    version "^8.9.112"
    }"###;
        let expected = PackageConfig {
            name: "abcd".to_string(),
            version: "145.54.12".to_string(),
            depends: vec![
                Dependency {
                    name: "coreutils".to_string(),
                    version_mask: "".to_string(),
                },
                Dependency {
                    name: "python".to_string(),
                    version_mask: "^8.9.112".to_string(),
                },
            ],
        };
        let x = get_package_config(s).unwrap();
        assert_eq!(x, expected);
    }

    #[test]
    fn string_to_package_1() {
        assert_eq!(
            string_to_package("a-b-c-d-e-1.2.3").unwrap(),
            Package {
                name: "a-b-c-d-e".to_string(),
                version: "1.2.3".to_string()
            }
        );
        assert_eq!(
            string_to_package("testing-123-0.4.3").unwrap(),
            Package {
                name: "testing-123".to_string(),
                version: "0.4.3".to_string()
            }
        );
    }
}
