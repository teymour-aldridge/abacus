use std::env;
use std::path::Path;
use std::process::Command;

fn main() {
    // Check for bootstrap
    let bootstrap_dir = Path::new("assets/bootstrap");
    if !bootstrap_dir.exists() {
        println!("cargo:warning=Bootstrap source not found, downloading...");
        // Create assets dir
        let assets_dir = Path::new("assets");
        if !assets_dir.exists() {
            std::fs::create_dir(assets_dir).unwrap();
        }

        let url =
            "https://github.com/twbs/bootstrap/archive/refs/tags/v5.3.3.tar.gz";
        let archive_path = "assets/bootstrap.tar.gz";

        // Download
        let status = Command::new("curl")
            .arg("-L")
            .arg(url)
            .arg("-o")
            .arg(archive_path)
            .status()
            .unwrap();

        if !status.success() {
            panic!("Failed to download bootstrap");
        }

        // Extract
        let status = Command::new("tar")
            .arg("-xzf")
            .arg(archive_path)
            .arg("-C")
            .arg("assets")
            .status()
            .unwrap();

        if !status.success() {
            panic!("Failed to extract bootstrap");
        }

        // Rename extracted folder
        let extracted_folder = Path::new("assets/bootstrap");
        let downloaded_folder = Path::new("assets/bootstrap-v5.3.3");
        if !downloaded_folder.exists() {
            // try another name
            let downloaded_folder = Path::new("assets/bootstrap-5.3.3");
            if downloaded_folder.exists() {
                std::fs::rename(downloaded_folder, extracted_folder).unwrap();
            }
        } else {
            std::fs::rename(downloaded_folder, extracted_folder).unwrap();
        }

        // Clean up
        std::fs::remove_file(archive_path).unwrap();

        println!("cargo:warning=Bootstrap downloaded and extracted.");
    }

    // Compile sass
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("style.css");
    let status = Command::new("sass")
        .arg("--load-path=assets")
        .arg("assets/scss/custom.scss")
        .arg(&dest_path)
        .status()
        .unwrap();

    if !status.success() {
        panic!("Failed to compile sass");
    }

    println!("cargo:rerun-if-changed=assets/scss/custom.scss");

    lalrpop::process_root().unwrap();
}
