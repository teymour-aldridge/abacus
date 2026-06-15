use std::env;
use std::ffi::OsStr;
use std::path::Path;
use std::process::Command;

fn main() {
    println!("cargo:rustc-check-cfg=cfg(fuzzing)");

    // Check for bootstrap
    let bootstrap_dir = Path::new("assets/bootstrap");
    let bootstrap_load_path = env::var("ABACUS_BOOTSTRAP_LOAD_PATH").ok();
    if !bootstrap_dir.exists() && bootstrap_load_path.is_none() {
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
    let mut sass = Command::new("sass");
    sass.arg("--load-path=assets");
    if let Some(load_path) = &bootstrap_load_path {
        sass.arg(format!("--load-path={load_path}"));
    }
    let status = sass
        .arg("assets/scss/main.scss")
        .arg(&dest_path)
        .status()
        .unwrap();

    if !status.success() {
        panic!("Failed to compile sass");
    }

    println!("cargo:rerun-if-changed=assets/scss");

    // Copy frontend build outputs into OUT_DIR so they can be embedded with include_str!.
    // Nix provides these as a separate derivation so frontend and Rust builds
    // can be cached independently; local development keeps the existing npm path.
    let frontend_dist_env = env::var("ABACUS_FRONTEND_DIST").ok();
    let frontend_dist = frontend_dist_env
        .as_deref()
        .map(Path::new)
        .unwrap_or_else(|| Path::new("static/dist"));
    let out_dir_path = Path::new(&out_dir);

    let files_to_copy = [
        ("draw-editor.js", "draw_editor.js"),
        ("store.css", "draw_editor.css"),
        ("draw-room-allocator.js", "draw_room_allocator.js"),
        ("store.css", "draw_room_allocator.css"),
        ("store.js", "store.js"),
        ("store.css", "store.css"),
    ];

    if frontend_dist_env.is_none() {
        let status = Command::new("npm")
            .arg("run")
            .arg("build")
            .arg("--prefix")
            .arg("frontend")
            .status()
            .unwrap();

        if !status.success() {
            panic!("Failed to compile frontend");
        }
    }

    println!("cargo:rerun-if-changed=frontend/src");
    println!("cargo:rerun-if-changed=frontend/vite.config.ts");

    for (src_name, dst_name) in &files_to_copy {
        let src = frontend_dist.join(src_name);
        let dst = out_dir_path.join(dst_name);
        std::fs::copy(&src, &dst).unwrap_or_else(|e| {
            panic!("Failed to copy {:?} to {:?}: {}", src, dst, e)
        });
    }

    generate_tournamentsim_regression_tests(out_dir_path);

    lalrpop::process_root().unwrap();
}

fn generate_tournamentsim_regression_tests(out_dir_path: &Path) {
    let regressions_dir = Path::new("src/tournamentsim/regressions");
    println!("cargo:rerun-if-changed={}", regressions_dir.display());

    let mut fixtures = std::fs::read_dir(regressions_dir)
        .unwrap_or_else(|e| {
            panic!(
                "Failed to read regression fixtures from {:?}: {}",
                regressions_dir, e
            )
        })
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension() == Some(OsStr::new("json")))
        .collect::<Vec<_>>();
    fixtures.sort();

    let generated = fixtures
        .iter()
        .map(|path| {
            let stem = path
                .file_stem()
                .and_then(OsStr::to_str)
                .expect("fixture filename should be valid UTF-8");
            let test_name = sanitize_test_name(stem);
            let fixture_path = path
                .canonicalize()
                .unwrap_or_else(|e| {
                    panic!(
                        "Failed to canonicalize regression fixture {:?}: {}",
                        path, e
                    )
                })
                .display()
                .to_string();

            format!(
                "#[test]\nfn {test_name}() {{\n    super::run_regression_fixture(\"{stem}\", include_str!(r#\"{fixture_path}\"#));\n}}\n"
            )
        })
        .collect::<String>();

    std::fs::write(
        out_dir_path.join("tournamentsim_regression_tests.rs"),
        generated,
    )
    .unwrap();
}

fn sanitize_test_name(name: &str) -> String {
    let mut out = String::from("regression_");
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push('_');
        }
    }
    out
}
