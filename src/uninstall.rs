use crate::{
    info::get_manually_installed,
    pkg::{onlinepackage_to_package, Package},
    repo::{resolve_dependencies_for_package, OnlinePackage},
    store::{get_installed_packages, package_to_store_location},
};
use anyhow::{anyhow, bail, Result};
use log::info;

#[derive(Debug)]
pub struct OnlinePackageWithDependCount {
    pkg: OnlinePackage,
    depends_count: u32,
    manually_installed: bool,
}

pub fn uninstall_package(pkg: &Package) -> Result<()> {
    info!("Uninstalling {}", pkg);
    let store_loc = package_to_store_location(pkg);

    if !store_loc.exists() || !store_loc.is_dir() {
        bail!("{} is not installed!", &pkg);
    }

    std::fs::remove_dir_all(&store_loc)?;
    Ok(())
}

/// Returns the dependency count of each package
pub fn get_dependency_count_for_packages(
    packages: &Vec<OnlinePackage>,
) -> Result<Vec<OnlinePackageWithDependCount>> {
    let packages = packages.clone();
    let mut ret = Vec::<OnlinePackageWithDependCount>::new();

    for package in &packages {
        ret.push(OnlinePackageWithDependCount {
            pkg: package.clone(),
            depends_count: 0,
            manually_installed: get_manually_installed(
                &onlinepackage_to_package(package),
            )?,
        })
    }

    for package in &packages {
        let mut dependencies = resolve_dependencies_for_package(
            &packages,
            &onlinepackage_to_package(&package),
        )?;

        let index =
            dependencies
                .iter()
                .position(|x| x == package)
                .ok_or(anyhow!(
                "Failed to find package in dependencies returned by resolve"
            ))?;
        dependencies.swap_remove(index);

        for depend in dependencies {
            let index =
                packages.iter().position(|x| x == &depend).ok_or(anyhow!(
                "Failed to find package in dependencies returned by resolve"
            ))?;

            ret.get_mut(index)
                .ok_or(anyhow!(
                    "Failed to get index {} from array {:#?}",
                    index,
                    &packages
                ))?
                .depends_count += 1;
        }
    }

    Ok(ret)
}

pub fn uninstall_package_and_deps(package: &Package) -> Result<()> {
    let packages = get_installed_packages()?;
    let mut dep_count = get_dependency_count_for_packages(&packages)?;
    let this_packages_dependencies =
        resolve_dependencies_for_package(&packages, package)?;
    for dep in dep_count.iter_mut() {
        if this_packages_dependencies.contains(&dep.pkg) {
            if dep.depends_count > 0 {
                dep.depends_count -= 1;
            }
        }
    }

    let cloned = package.clone();
    for pkg in dep_count {
        if onlinepackage_to_package(&pkg.pkg) == cloned {
            if pkg.depends_count > 0 {
                bail!("Package is depended upon! Failed to uninstall");
            }
            uninstall_package(&onlinepackage_to_package(&pkg.pkg))?;
            continue;
        }

        let user_depends = if pkg.manually_installed { 1 } else { 0 };
        if pkg.depends_count + user_depends == 0 {
            uninstall_package(&onlinepackage_to_package(&pkg.pkg))?;
        }
    }
    Ok(())
}
