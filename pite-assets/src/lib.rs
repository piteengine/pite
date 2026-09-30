//! `pite-assets`: minimal import (`Importer` trait + `uid` registry).
//! M0 registers `png` only; atlases/audio are new impls on the same registry.

use std::collections::HashMap;

use anyhow::Result;

pub trait Importer: Send + Sync {
    fn match_ext(&self, ext: &str) -> bool;
    fn import(&self, path: &str) -> Result<()>;
}

pub struct PngImporter;

impl Importer for PngImporter {
    fn match_ext(&self, ext: &str) -> bool {
        ext.eq_ignore_ascii_case("png")
    }

    fn import(&self, path: &str) -> Result<()> {
        if !std::path::Path::new(path).is_file() {
            anyhow::bail!("asset not found: {path}");
        }
        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct UidRegistry {
    by_path: HashMap<String, String>,
}

impl UidRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, path: impl Into<String>, uid: impl Into<String>) {
        self.by_path.insert(path.into(), uid.into());
    }

    pub fn get(&self, path: &str) -> Option<&str> {
        self.by_path.get(path).map(String::as_str)
    }
}

pub fn default_importers() -> Vec<Box<dyn Importer>> {
    vec![Box::new(PngImporter)]
}
