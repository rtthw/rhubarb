//! # Runner

use {
    anyhow::{Result, bail},
    std::{
        env::{current_dir, set_current_dir},
        fs::{create_dir_all, read_dir},
        path::{Path, PathBuf},
        process::{Command, ExitStatus, Stdio},
    },
};

const OBJECT_SOURCES: &[(&str, &str)] = &[
    ("crates", "boot-info"),
    ("crates", "framebuffer"),
    ("crates", "heap"),
    ("crates", "input"),
    ("crates", "log"),
    ("crates", "math"),
    ("crates", "panic"),
    ("crates", "process"),
    ("crates", "time"),
    ("drivers", "pci"),
    ("drivers", "pit"),
    ("drivers", "uart-16550"),
    ("drivers", "virtio"),
    ("", "shell"),
    ("shell/apps", "input-driver"),
];
const EXTERNAL_DEPS: &[&str] = &["core", "alloc", "compiler_builtins", "hashbrown"];

fn main() -> Result<()> {
    let workspace_dir = find_workspace_dir()?;
    let _ = set_current_dir(&workspace_dir);

    println!("Creating build directories...");

    let kernel_dir = workspace_dir.join("kernel");
    if !kernel_dir.exists() {
        bail!("`./kernel` not found");
    }
    let extraction_dir = kernel_dir.join("tmp/extracted");
    if !extraction_dir.exists() {
        create_dir_all(&extraction_dir)?;
    }
    let boot_dir = kernel_dir.join("esp/efi/boot");
    if !boot_dir.exists() {
        create_dir_all(&boot_dir)?;
    }
    let uefi_dir = kernel_dir.join("firmware/uefi");
    get_ovmf(&uefi_dir)?;

    if !is_program_installed("rustc")? {
        bail!("Rust is not installed");
    }

    println!("Building bootloader...");

    set_current_dir(workspace_dir.join("bootloader"))?;
    let cargo_exit_status = Command::new("cargo")
        .arg("build")
        .arg("--release")
        .status()?;
    if !cargo_exit_status.success() {
        bail!(
            "Failed to build bootloader, cargo exited with code: {}",
            cargo_exit_status.code().unwrap(),
        );
    }
    set_current_dir(&workspace_dir)?;
    std::fs::copy(
        workspace_dir.join("target/x86_64-unknown-uefi/release/bootloader.efi"),
        boot_dir.join("bootx64.efi"),
    )?;

    for (crate_dir, crate_name) in OBJECT_SOURCES {
        println!("\tBuilding `{crate_name}`...");
        let cargo_exit_status = build_crate_object(&workspace_dir, crate_dir, crate_name)?;
        if !cargo_exit_status.success() {
            bail!(
                "Failed to build `{crate_name}`, cargo exited with code: {}",
                cargo_exit_status.code().unwrap(),
            );
        }
    }

    // Create object files for external dependencies.
    let external_deps_dir = workspace_dir.join("target/x86_64-app/release/deps");
    for name in EXTERNAL_DEPS {
        let extraction_path = extraction_dir.join(name);
        if extraction_path.exists() {
            continue;
        }
        create_dir_all(&extraction_path)?;
        let rlib_path = find_rlib(&external_deps_dir, name)?;

        println!(
            "Extracting `{}` to `{}`...",
            rlib_path.display(),
            extraction_path.display(),
        );

        if !Command::new("ar")
            .arg("-xo")
            .arg("--output")
            .arg(&extraction_path)
            .arg(&rlib_path)
            .status()?
            .success()
        {
            bail!("Failed to extract `{}`", rlib_path.display());
        }

        let mut ld_command = Command::new("ld");
        ld_command
            .arg("-r")
            .arg("--output")
            .arg(&format!("kernel/esp/{name}.o"));
        for entry in read_dir(&extraction_path)? {
            let entry = entry?;
            if entry.file_name().to_string_lossy().ends_with(".o") {
                ld_command.arg(entry.path());
            }
        }
        if !ld_command.status()?.success() {
            bail!("Failed to extract `{}`", rlib_path.display());
        }
    }

    println!("Linking core language object...");

    let mut ld_command = Command::new("ld");
    ld_command
        .arg("-r")
        .arg("--output")
        .arg(&format!("kernel/esp/lang.o"))
        .arg(&format!("kernel/esp/core.o"))
        .arg(&format!("kernel/esp/compiler_builtins.o"));
    if !ld_command.status()?.success() {
        bail!("Failed to link Rust language object");
    }

    println!("Building kernel...");

    let cargo_exit_status = Command::new("cargo")
        .arg("rustc")
        .arg("--release")
        .arg("--manifest-path=kernel/Cargo.toml")
        .arg("--target=kernel/x86_64-kernel.json")
        .arg("-Zbuild-std=core,alloc")
        .arg("-Zbuild-std-features=compiler-builtins-mem")
        .arg("--")
        .arg("-Clink-arg=-T")
        .arg("-Clink-arg=kernel/kernel_x86_64.ld")
        .arg("-Clink-arg=-z")
        .arg("-Clink-arg=max-page-size=0x1000")
        .arg("--emit=link=kernel/esp/kernel")
        .status()?;
    if !cargo_exit_status.success() {
        bail!(
            "Failed to build kernel, cargo exited with code: {}",
            cargo_exit_status.code().unwrap(),
        );
    }

    let qemu_exit_status = run_qemu()?;
    if !qemu_exit_status.success() {
        bail!(
            "QEMU failed with exit code {}",
            qemu_exit_status.code().unwrap(),
        );
    }

    Ok(())
}

fn find_workspace_dir() -> Result<PathBuf> {
    let current_dir = current_dir()?.canonicalize()?;
    for path in current_dir.ancestors() {
        if path.is_dir() && path.file_name().is_some_and(|name| name == "rhubarb") {
            return Ok(path.to_path_buf());
        }
    }

    bail!("failed to find the workspace directory")
}

fn find_rlib(dir: &Path, name: &str) -> Result<PathBuf> {
    for entry in read_dir(dir)? {
        let entry = entry?;
        let file_name = entry.file_name();
        let name_str = file_name.to_string_lossy();
        if name_str.starts_with(&format!("lib{name}-")) && name_str.ends_with(".rlib") {
            return Ok(entry.path());
        }
    }

    bail!("no rlib found for `{name}`")
}

fn build_crate_object(workspace_dir: &Path, dir: &str, name: &str) -> Result<ExitStatus> {
    // let parent_dir = if dir.is_empty() {}
    let manifest_path = workspace_dir.join(dir).join(name).join("Cargo.toml");
    let object_path = workspace_dir
        .join("kernel/esp")
        .join(format!("{}.o", crate_name_to_object_name(name)));

    Ok(Command::new("cargo")
        .arg("rustc")
        .arg("--release")
        .arg(format!("--manifest-path={}", manifest_path.display()))
        .arg("--target=kernel/x86_64-app.json")
        .arg("-Zbuild-std=core,alloc")
        .arg("-Zbuild-std-features=compiler-builtins-mem")
        .arg("--")
        .arg("--crate-type=lib")
        .arg(format!("--emit=obj={}", object_path.display()))
        .arg("-Clink-dead-code=yes")
        .arg("-Clink-arg=-m")
        .arg("-Clink-arg=64")
        .arg("-Zshare-generics=no")
        .status()?)
}

fn crate_name_to_object_name(crate_name: &str) -> String {
    crate_name.replace('-', "_")
}

fn is_program_installed(name: &str) -> Result<bool> {
    Ok(Command::new("which")
        .arg(name)
        .stdout(Stdio::null())
        .status()
        .map(|status| status.success())?)
}

fn run_qemu() -> Result<ExitStatus> {
    let qemu_program = "qemu-system-x86_64";
    if !is_program_installed(qemu_program)? {
        bail!("QEMU is not installed");
    }

    Ok(Command::new(qemu_program)
        .args([
            "-accel",
            "kvm",
            "-m",
            "256M",
            "-smp",
            "4",
            "-rtc",
            "base=localtime",
            "-display",
            "gtk,show-tabs=on",
            "-device",
            "virtio-keyboard",
            "-device",
            "virtio-mouse",
            "-vga",
            "virtio",
            "-serial",
            "stdio",
            "-serial",
            "vc",
            "-drive",
            "format=raw,file=fat:rw:kernel/esp",
            "-drive",
            "if=pflash,format=raw,readonly=on,file=kernel/firmware/uefi/OVMF_CODE.fd",
            "-drive",
            "if=pflash,format=raw,readonly=on,file=kernel/firmware/uefi/OVMF_VARS.fd",
        ])
        .status()?)
}

// TODO: Get OVMF from systems that may not have it installed at the standard
//       location.
fn get_ovmf(uefi_dir: &Path) -> Result<()> {
    if !uefi_dir.exists() {
        create_dir_all(&uefi_dir)?;
    }
    let code_output_path = uefi_dir.join("OVMF_CODE.fd");
    let vars_output_path = uefi_dir.join("OVMF_VARS.fd");

    #[cfg(target_os = "linux")]
    {
        let debian_based_code_path = PathBuf::from("/usr/share/OVMF/OVMF_CODE.fd");
        let debian_based_vars_path = PathBuf::from("/usr/share/OVMF/OVMF_VARS.fd");

        if !debian_based_code_path.exists() || !debian_based_vars_path.exists() {
            panic!(
                "Support for systems that aren't Debian-based is not yet ready. If this is a \
                Debian-based system, make sure you have that OVMF package installed",
            );
        }

        if !code_output_path.exists() {
            std::fs::copy(debian_based_code_path, code_output_path)?;
        }
        if !vars_output_path.exists() {
            std::fs::copy(debian_based_vars_path, vars_output_path)?;
        }

        return Ok(());
    }
    #[cfg(not(target_os = "linux"))]
    {
        bail!("runner support for non-Linux systems is not yet available")
    }
}
