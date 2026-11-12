// SPDX-License-Identifier: MIT OR Apache-2.0
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use zip::write::SimpleFileOptions;
use zip::CompressionMethod;

const TEMPLATES: &[&str] = &["minimal-2d", "gem-collector"];

fn main() {
    for name in TEMPLATES {
        let repo_example = Path::new("../../examples").join(name);
        println!("cargo:rerun-if-changed=../../examples/{name}");

        if repo_example.is_dir() {
            let zip_path = Path::new("templates").join(format!("{name}.zip"));
            let mut buffer = Vec::new();
            let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut buffer));
            let options =
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

            let mut files = Vec::new();
            collect_files(&repo_example, &repo_example, &mut files);
            files.sort();

            for (rel, full) in files {
                zip.start_file(&rel, options).expect("start zip file");
                let mut f = File::open(&full).expect("open template source file");
                let mut contents = Vec::new();
                f.read_to_end(&mut contents).expect("read file");
                zip.write_all(&contents).expect("write file to zip");
            }
            zip.finish().expect("finish zip");

            let existing = std::fs::read(&zip_path).unwrap_or_default();
            if existing != buffer {
                if let Some(parent) = zip_path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                std::fs::write(&zip_path, &buffer)
                    .unwrap_or_else(|e| panic!("write {}: {e}", zip_path.display()));
            }
        }
    }
}

fn collect_files(root: &Path, dir: &Path, files: &mut Vec<(String, std::path::PathBuf)>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') || name == "_cache" {
                continue;
            }
            if path.is_dir() {
                collect_files(root, &path, files);
            } else if path.is_file() {
                if let Ok(rel) = path.strip_prefix(root) {
                    let rel_str = rel.to_string_lossy().replace('\\', "/");
                    files.push((rel_str, path));
                }
            }
        }
    }
}
