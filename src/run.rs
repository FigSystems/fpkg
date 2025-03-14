use log::{debug, error};
use nix::mount::MsFlags;
use std::{
    fs::hard_link,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, bail, Result};
use rand::prelude::*;
use sys_mount::{unmount, UnmountFlags};

use crate::{
    env::package_to_env_location,
    pkg::Package,
    store::{
        get_installed_packages, get_store_location, package_to_store_location,
    },
};

pub fn get_run_location() -> PathBuf {
    match crate::config::get_config_option(&"run".to_string()) {
        Some(x) => PathBuf::from(x),
        None => PathBuf::from("/fpkg/run/"),
    }
}

pub fn get_tmp_location() -> PathBuf {
    match crate::config::get_config_option(&"tmp".to_string()) {
        Some(x) => PathBuf::from(x),
        None => PathBuf::from("/fpkg/tmp"),
    }
}

pub fn get_random_string(length: usize) -> String {
    let mut rng = rand::rng();
    let mut ret = String::new();
    for _ in 0..length {
        ret.push(rng.sample(rand::distr::Alphanumeric) as char)
    }
    ret
}

pub fn join_proper(a: &Path, b: &Path) -> Result<PathBuf> {
    Ok(a.join(make_path_relative(b)))
}

pub fn make_path_relative(a: &Path) -> PathBuf {
    match a.strip_prefix("/") {
        Ok(x) => x.to_path_buf(),
        Err(_) => a.to_path_buf(), // a is already a relative path
    }
}

/// Makes src's contents show up at target
pub fn mount(src: &Path, target: &Path) -> Result<(), std::io::Error> {
    let mut flags = MsFlags::MS_BIND;
    flags.insert(MsFlags::MS_REC);
    nix::mount::mount(
        Option::<&Path>::Some(src),
        target,
        Option::<&Path>::None,
        flags,
        Option::<&Path>::None,
    )?;
    nix::mount::mount(
        Option::<&Path>::None,
        target,
        Option::<&Path>::None,
        MsFlags::MS_SLAVE.union(MsFlags::MS_REC),
        Option::<&Path>::None,
    )?;
    // Mount::builder().flags(MountFlags::BIND).mount(src, target)
    Ok(())
}

pub fn bind_mount(src: &Path, target: &Path) -> Result<()> {
    if src.is_dir() {
        std::fs::DirBuilder::new().recursive(true).create(&target)?;
    } else {
        std::fs::DirBuilder::new()
            .recursive(true)
            .create(&target.parent().unwrap_or(Path::new("/")))?;
        std::fs::File::create(&target)?;
    }
    match mount(&src, &target) {
        Err(x) => bail!(x.to_string()),
        Ok(_) => Ok(()),
    }
}

pub fn run_pkg(
    pkg: &Package,
    uid: u32,
    args: Vec<String>,
    cmd: Option<&str>,
) -> Result<i32> {
    let env_dir = package_to_env_location(pkg)?;
    if !env_dir.is_dir() {
        bail!(
            "Package {}-{} does not have an environment!",
            pkg.name,
            pkg.version
        );
    }
    let pkg_store_dir = package_to_store_location(pkg);
    if !pkg_store_dir.is_dir() {
        bail!("Package {}-{} not found!", pkg.name, pkg.version);
    }
    let cmd = match cmd {
        Some(x) => x,
        None => &pkg.name.clone(),
    };

    run_pkg_(&env_dir, uid, args, cmd)
}

pub fn run_pkg_(
    env_dir: &Path,
    uid: u32,
    args: Vec<String>,
    cmd: &str,
) -> Result<i32> {
    let mut out_dir = PathBuf::from("/");
    while out_dir.exists() || out_dir == PathBuf::from("/") {
        out_dir = get_run_location().join(get_random_string(10));
    }
    std::fs::DirBuilder::new()
        .recursive(true)
        .create(&out_dir)?;

    let store_dir = get_store_location();

    // Bind mount fpkg store inside the out_dir
    let store_target = join_proper(&out_dir, &store_dir)?;
    bind_mount(&store_dir, &store_target)?;

    let mut binds = Vec::<PathBuf>::new();

    for ent in std::fs::read_dir(&env_dir)? {
        let ent = ent?;
        let ent_full_path = env_dir.join(ent.path());
        let ent_path = ent.path();
        let ent_relative_path = ent_path.strip_prefix(&env_dir)?;
        if !ent_full_path.exists() {
            bail!(
                "Path {} does not exist even though it showed up in the directory listing!",
                ent_full_path.display()
            );
        }

        let target = join_proper(&out_dir, ent_relative_path)?;
        if ent_full_path.is_file() {
            bind_mount(&ent_full_path, &target)?;
        } else if ent_full_path.is_symlink() {
            hard_link(&ent_full_path, &target)?;
        } else if ent_full_path.is_dir() {
            bind_mount(&ent_full_path, &target)?;
        } else {
            bind_mount(&ent_full_path, &target)?;
        }
        binds.push(target);
    }

    for bind in vec![
        "dev", "mnt", "media", "run", "var", "home", "tmp", "proc", "tmp",
    ] {
        let dir = Path::new("/").join(bind);
        let dir_target = out_dir.join(bind);
        if dir_target.exists() {
            continue;
        }
        if !dir.exists() {
            continue;
        }
        bind_mount(&dir, &dir_target)?;
        binds.push(dir_target);
    }

    let mut cleanup = false;

    let mut prefix = "/";
    if out_dir.join("bin").join(&cmd).is_file()
        || out_dir.join("bin").join(&cmd).is_symlink()
    {
        prefix = "/bin";
    } else if out_dir.join("usr/bin").join(&cmd).is_file()
        || out_dir.join("usr/bin").join(&cmd).is_symlink()
    {
        prefix = "/usr/bin";
    } else {
        error!("Warning! No executable found!");
        cleanup = true;
    }
    let mut code: i32 = 0;
    if !cleanup {
        code = std::process::Command::new(std::env::current_exe()?)
            .arg("chroot-not-intended-for-interactive-use")
            .arg(&out_dir.to_str().ok_or(anyhow::anyhow!(
                "Failed to parse directory {} into string!",
                &out_dir.display()
            ))?)
            .arg(uid.to_string())
            .arg(Path::new(prefix).join(&cmd))
            .args(args)
            .spawn()?
            .wait()?
            .code()
            .unwrap_or(89);
    }

    let mut binds2: Vec<PathBuf> = Vec::new();
    let mut binds = binds;
    binds.push(store_target);

    for _ in 0..10 {
        for bind in &binds {
            let e = unmount(&bind, UnmountFlags::DETACH);
            if e.is_err() {
                binds2.push(bind.clone());
            } else {
                if bind.is_dir() {
                    if bind.read_dir()?.next().is_some() {
                        for p in walkdir::WalkDir::new(&bind) {
                            if let Ok(p) = p {
                                let _ =
                                    unmount(&p.path(), UnmountFlags::empty());
                            }
                        }

                        binds2.push(bind.clone());
                    }
                }
            }
        }
        binds = binds2.clone();
        binds2 = Vec::new();
    }

    assert!(binds2.is_empty(), "Terminated to prevent data loss");

    std::fs::remove_dir_all(&out_dir)?;

    Ok(code)
}

pub fn run_multiple_packages(
    pkgs: &Vec<Package>,
    uid: u32,
    args: Vec<String>,
    cmd: Option<&str>,
) -> Result<i32> {
    if pkgs.is_empty() {
        bail!("No packages specified!");
    }

    if pkgs.len() == 1 {
        return run_pkg(&pkgs[0], uid, args, cmd);
    }

    let mut pkg_path = format!("tmp-env-{}", get_random_string(10));
    while get_tmp_location()
        .join(pkg_path.clone() + "-1.0.0")
        .exists()
    {
        pkg_path = format!("tmp-env-{}", get_random_string(10));
    }
    let pkg_path_name = PathBuf::from(pkg_path.clone());
    let pkg_path = get_tmp_location().join(pkg_path + "-1.0.0"); // Make immutable
    std::fs::DirBuilder::new()
        .recursive(true)
        .create(&pkg_path)?;
    std::fs::DirBuilder::new()
        .recursive(true)
        .create(pkg_path.join("data").join("fpkg"))?;

    let mut pkg_depends_str = String::new();
    for pkg in pkgs {
        pkg_depends_str.push_str(&format!(
            "depends \"{}\" {{\n    version \"{}\"\n}}\n",
            pkg.name, pkg.version
        ));
    }

    let mut pkg_info_str = String::new();
    pkg_info_str.push_str(&format!(
        "name \"{}\"\n",
        pkg_path_name
            .to_str()
            .ok_or(anyhow!("Failed to convert path into string!"))?
    ));

    pkg_info_str.push_str("version \"1.0.0\"\n");
    pkg_info_str.push_str(&pkg_depends_str);
    std::fs::write(
        pkg_path.join("data").join("fpkg").join("pkg.kdl"),
        &pkg_info_str,
    )?;

    let installed_packages = get_installed_packages()?;

    let mut done_list: Vec<Package> = Vec::new();

    for pkg in pkgs {
        crate::env::generate_environment_for_package(
            pkg,
            &installed_packages,
            &pkg_path,
            &mut done_list,
        )?;
    }

    let cmd = cmd.unwrap_or(&pkgs[0].name);

    debug!(
        "run_pkg_({}, {}, {:?}, {})",
        pkg_path.display(),
        uid,
        args,
        cmd
    );
    let code = run_pkg_(&pkg_path, uid, args, cmd)?;

    std::fs::remove_dir_all(pkg_path)?;

    Ok(code)
}
