// SPDX-License-Identifier: MIT OR Apache-2.0
//! Binary scene cache: TOML stays source of truth; `_cache/` holds
//! content-hash-keyed, version-stamped binaries.
//!
//! Pure logic lives here (hash, paths, encode/decode, stale matching);
//! `lib.rs` owns the filesystem flow (`load_cached`).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::{SceneDoc, SceneInstance, SceneNode, SceneScript, FORMAT_VERSION};

/// Cache container version. A bump rebuilds every cache from TOML
/// instead of misreading it.
pub const CACHE_VERSION: u32 = 1;

/// Sibling directory next to each scene file holding its binaries.
pub const CACHE_DIR_NAME: &str = "_cache";

/// Extension for cache files: `<stem>.<content-hash>.bin`.
pub const CACHE_EXT: &str = "bin";

/// Magic prefix of every cache file; anything else is corruption.
const MAGIC: &[u8; 8] = b"PITSCN01";

/// Header carried by every cache file: which container and scene
/// versions it was written with, plus the exact source it came from.
#[derive(Debug, Clone, PartialEq)]
pub struct CacheHeader {
    pub cache_version: u32,
    pub format_version: u32,
    pub source_len: u64,
    pub source_hash: u64,
}

/// How a scene load resolved: cache hit, clean miss (no usable cache),
/// or rebuild (cache existed but was stale/corrupt/mismatched).
#[derive(Debug, Clone, PartialEq)]
pub enum CacheStatus {
    Hit,
    Miss,
    Rebuilt(String),
}

impl std::fmt::Display for CacheStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CacheStatus::Hit => write!(f, "hit"),
            CacheStatus::Miss => write!(f, "miss"),
            CacheStatus::Rebuilt(reason) => write!(f, "rebuilt ({reason})"),
        }
    }
}

/// FNV-1a 64-bit over bytes (same function as `pite-assets`, zero deps).
pub fn fnv1a_u64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 14695981039346656037;
    for b in bytes {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(1099511628211);
    }
    hash
}

/// Lowercase hex content hash used in cache filenames.
pub fn content_hash(bytes: &[u8]) -> String {
    format!("{:016x}", fnv1a_u64(bytes))
}

/// Cache path for a scene file + hex content hash:
/// `<scene_dir>/_cache/<stem>.<hash>.bin`. The hash keys the cache,
/// so any source change is a different file (old ones get pruned).
pub fn cache_path_for(scene_path: &Path, source_hash_hex: &str) -> PathBuf {
    let dir = scene_path.parent().unwrap_or_else(|| Path::new("."));
    let stem = scene_path
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("scene");
    dir.join(CACHE_DIR_NAME)
        .join(format!("{stem}.{source_hash_hex}.{CACHE_EXT}"))
}

/// Remove sibling caches for `scene_path` whose hash differs from
/// `keep_hash_hex`; returns removed paths. Best-effort: unreadable
/// dirs or failed removals are ignored, never errors.
pub fn prune_stale(scene_path: &Path, keep_hash_hex: &str) -> Vec<PathBuf> {
    let dir = scene_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(CACHE_DIR_NAME);
    let stem = scene_path
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("scene");
    let keep = format!("{stem}.{keep_hash_hex}.{CACHE_EXT}");
    let prefix = format!("{stem}.");
    let suffix = format!(".{CACHE_EXT}");
    let mut removed = Vec::new();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return removed;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == keep || !name.starts_with(&prefix) || !name.ends_with(&suffix) {
            continue;
        }
        let path = entry.path();
        if std::fs::remove_file(&path).is_ok() {
            removed.push(path);
        }
    }
    removed.sort();
    removed
}

fn put_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put_u64(out: &mut Vec<u8>, v: u64) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put_i64(out: &mut Vec<u8>, v: i64) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put_f64(out: &mut Vec<u8>, v: f64) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put_str(out: &mut Vec<u8>, s: &str) {
    put_u32(out, s.len() as u32);
    out.extend_from_slice(s.as_bytes());
}

fn put_opt_str(out: &mut Vec<u8>, value: &Option<String>) {
    out.push(u8::from(value.is_some()));
    if let Some(s) = value {
        put_str(out, s);
    }
}

/// `toml::Value` tags. Tables sort by key on write (they already do:
/// `toml::Map` is a `BTreeMap`), so encoding is deterministic.
fn put_toml_value(out: &mut Vec<u8>, value: &toml::Value) {
    match value {
        toml::Value::String(s) => {
            out.push(0);
            put_str(out, s);
        }
        toml::Value::Integer(i) => {
            out.push(1);
            put_i64(out, *i);
        }
        toml::Value::Float(f) => {
            out.push(2);
            put_f64(out, *f);
        }
        toml::Value::Boolean(b) => {
            out.push(3);
            out.push(u8::from(*b));
        }
        toml::Value::Array(items) => {
            out.push(4);
            put_u32(out, items.len() as u32);
            for item in items {
                put_toml_value(out, item);
            }
        }
        toml::Value::Table(table) => {
            out.push(5);
            put_u32(out, table.len() as u32);
            for (k, v) in table {
                put_str(out, k);
                put_toml_value(out, v);
            }
        }
        toml::Value::Datetime(dt) => {
            out.push(6);
            put_str(out, &dt.to_string());
        }
    }
}

fn put_script(out: &mut Vec<u8>, script: &Option<SceneScript>) {
    out.push(u8::from(script.is_some()));
    if let Some(s) = script {
        put_str(out, &s.path);
        put_str(out, &s.class);
    }
}

fn put_node(out: &mut Vec<u8>, node: &SceneNode) {
    put_str(out, &node.id);
    put_str(out, &node.type_name);
    put_str(out, &node.name);
    put_opt_str(out, &node.parent);
    let mut keys: Vec<&String> = node.props.keys().collect();
    keys.sort();
    put_u32(out, keys.len() as u32);
    for k in keys {
        put_str(out, k);
        put_toml_value(out, &node.props[k]);
    }
    put_script(out, &node.script);
}

fn put_instance(out: &mut Vec<u8>, inst: &SceneInstance) {
    put_str(out, &inst.scene);
    put_opt_str(out, &inst.parent);
    put_str(out, &inst.prefix);
    let mut keys: Vec<&String> = inst.overrides.keys().collect();
    keys.sort();
    put_u32(out, keys.len() as u32);
    for k in keys {
        put_str(out, k);
        put_toml_value(out, &inst.overrides[k]);
    }
}

fn put_doc(out: &mut Vec<u8>, doc: &SceneDoc) {
    put_u32(out, doc.format_version);
    put_str(out, &doc.root);
    put_u32(out, doc.node.len() as u32);
    for node in &doc.node {
        put_node(out, node);
    }
    put_u32(out, doc.instance.len() as u32);
    for inst in &doc.instance {
        put_instance(out, inst);
    }
}

/// Encode a scene document as a cache payload stamped with the live
/// [`CACHE_VERSION`] and [`FORMAT_VERSION`].
pub fn encode(doc: &SceneDoc, source_hash: u64, source_len: u64) -> Vec<u8> {
    encode_with(doc, source_hash, source_len, CACHE_VERSION, FORMAT_VERSION)
}

/// Encode with explicit stamps (test hook: simulates version bumps
/// without touching the live constants).
pub fn encode_with(
    doc: &SceneDoc,
    source_hash: u64,
    source_len: u64,
    cache_version: u32,
    format_version: u32,
) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    put_u32(&mut out, cache_version);
    put_u32(&mut out, format_version);
    put_u64(&mut out, source_len);
    put_u64(&mut out, source_hash);
    put_doc(&mut out, doc);
    out
}

struct Cursor<'a> {
    b: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self
            .pos
            .checked_add(n)
            .context("scene cache is corrupt (truncated)")?;
        if end > self.b.len() {
            anyhow::bail!("scene cache is corrupt (truncated)");
        }
        let slice = &self.b[self.pos..end];
        self.pos = end;
        Ok(slice)
    }

    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn u32(&mut self) -> Result<u32> {
        let b: [u8; 4] = self.take(4)?.try_into().expect("take(4) is 4 bytes");
        Ok(u32::from_le_bytes(b))
    }

    fn u64(&mut self) -> Result<u64> {
        let b: [u8; 8] = self.take(8)?.try_into().expect("take(8) is 8 bytes");
        Ok(u64::from_le_bytes(b))
    }

    fn i64(&mut self) -> Result<i64> {
        let b: [u8; 8] = self.take(8)?.try_into().expect("take(8) is 8 bytes");
        Ok(i64::from_le_bytes(b))
    }

    fn f64(&mut self) -> Result<f64> {
        let b: [u8; 8] = self.take(8)?.try_into().expect("take(8) is 8 bytes");
        Ok(f64::from_le_bytes(b))
    }

    fn str(&mut self) -> Result<String> {
        let len = self.u32()? as usize;
        let bytes = self.take(len)?;
        String::from_utf8(bytes.to_vec()).context("scene cache is corrupt (bad UTF-8)")
    }

    fn opt_str(&mut self) -> Result<Option<String>> {
        match self.u8()? {
            0 => Ok(None),
            1 => Ok(Some(self.str()?)),
            tag => anyhow::bail!("scene cache is corrupt (bad option tag {tag})"),
        }
    }
}

fn get_toml_value(cur: &mut Cursor<'_>) -> Result<toml::Value> {
    match cur.u8()? {
        0 => Ok(toml::Value::String(cur.str()?)),
        1 => Ok(toml::Value::Integer(cur.i64()?)),
        2 => Ok(toml::Value::Float(cur.f64()?)),
        3 => Ok(toml::Value::Boolean(match cur.u8()? {
            0 => false,
            1 => true,
            tag => anyhow::bail!("scene cache is corrupt (bad bool tag {tag})"),
        })),
        4 => {
            let len = cur.u32()? as usize;
            let mut items = Vec::with_capacity(len.min(1024));
            for _ in 0..len {
                items.push(get_toml_value(cur)?);
            }
            Ok(toml::Value::Array(items))
        }
        5 => {
            let len = cur.u32()? as usize;
            let mut table = toml::map::Map::new();
            for _ in 0..len {
                let k = cur.str()?;
                let v = get_toml_value(cur)?;
                table.insert(k, v);
            }
            Ok(toml::Value::Table(table))
        }
        6 => {
            let raw = cur.str()?;
            let dt: toml::value::Datetime = raw
                .parse()
                .context("scene cache is corrupt (bad datetime)")?;
            Ok(toml::Value::Datetime(dt))
        }
        tag => anyhow::bail!("scene cache is corrupt (bad value tag {tag})"),
    }
}

fn get_script(cur: &mut Cursor<'_>) -> Result<Option<SceneScript>> {
    match cur.u8()? {
        0 => Ok(None),
        1 => Ok(Some(SceneScript {
            path: cur.str()?,
            class: cur.str()?,
        })),
        tag => anyhow::bail!("scene cache is corrupt (bad script tag {tag})"),
    }
}

fn get_node(cur: &mut Cursor<'_>) -> Result<SceneNode> {
    let id = cur.str()?;
    let type_name = cur.str()?;
    let name = cur.str()?;
    let parent = cur.opt_str()?;
    let prop_len = cur.u32()? as usize;
    let mut props = std::collections::BTreeMap::new();
    for _ in 0..prop_len {
        let k = cur.str()?;
        let v = get_toml_value(cur)?;
        props.insert(k, v);
    }
    let script = get_script(cur)?;
    Ok(SceneNode {
        id,
        type_name,
        name,
        parent,
        props,
        script,
    })
}

fn get_instance(cur: &mut Cursor<'_>) -> Result<SceneInstance> {
    let scene = cur.str()?;
    let parent = cur.opt_str()?;
    let prefix = cur.str()?;
    let len = cur.u32()? as usize;
    let mut overrides = std::collections::BTreeMap::new();
    for _ in 0..len {
        let k = cur.str()?;
        let v = get_toml_value(cur)?;
        overrides.insert(k, v);
    }
    Ok(SceneInstance {
        scene,
        parent,
        prefix,
        overrides,
    })
}

fn get_doc(cur: &mut Cursor<'_>) -> Result<SceneDoc> {
    let format_version = cur.u32()?;
    let root = cur.str()?;
    let node_len = cur.u32()? as usize;
    let mut node = Vec::with_capacity(node_len.min(1024));
    for _ in 0..node_len {
        node.push(get_node(cur)?);
    }
    let inst_len = cur.u32()? as usize;
    let mut instance = Vec::with_capacity(inst_len.min(64));
    for _ in 0..inst_len {
        instance.push(get_instance(cur)?);
    }
    Ok(SceneDoc {
        format_version,
        root,
        node,
        instance,
    })
}

/// Decode a cache payload. Bad magic, truncation, trailing bytes, or a
/// version stamp that is not the live one all fail loudly — the caller
/// falls back to TOML and says so, never silently reinterprets.
pub fn decode(bytes: &[u8]) -> Result<(SceneDoc, CacheHeader)> {
    let mut cur = Cursor { b: bytes, pos: 0 };
    let magic = cur
        .take(MAGIC.len())
        .context("scene cache is corrupt (truncated header)")?;
    if magic != MAGIC {
        anyhow::bail!("scene cache is corrupt (bad magic)");
    }
    let header = CacheHeader {
        cache_version: cur
            .u32()
            .context("scene cache is corrupt (truncated header)")?,
        format_version: cur
            .u32()
            .context("scene cache is corrupt (truncated header)")?,
        source_len: cur
            .u64()
            .context("scene cache is corrupt (truncated header)")?,
        source_hash: cur
            .u64()
            .context("scene cache is corrupt (truncated header)")?,
    };
    if header.cache_version != CACHE_VERSION {
        anyhow::bail!(
            "scene cache has cache version {} (engine uses {CACHE_VERSION})",
            header.cache_version
        );
    }
    if header.format_version != FORMAT_VERSION {
        anyhow::bail!(
            "scene cache has scene format_version {} (engine uses {FORMAT_VERSION})",
            header.format_version
        );
    }
    let doc = get_doc(&mut cur)?;
    if cur.pos != bytes.len() {
        anyhow::bail!(
            "scene cache is corrupt ({} trailing bytes)",
            bytes.len() - cur.pos
        );
    }
    Ok((doc, header))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCENE: &str = r#"
format_version = 1
root = "root"

[[node]]
id = "root"
type = "Node2D"
name = "Main"

[[node]]
id = "player"
type = "Sprite2D"
name = "Player"
parent = "root"

[node.props]
texture = "res://assets/player.png"
position = [100.0, 200.0]
visible = true
lives = 3
speed = 1.5
tags = ["a", "b"]
stamped = 1979-05-27T07:32:00Z

[node.props.extra]
level = 2

[node.script]
path = "res://scripts/player.py"
class = "Player"

[[instance]]
scene = "res://scenes/enemy.pitescene"
parent = "root"
prefix = "e1_"

[instance.overrides]
"sprite.position" = [300.0, 120.0]
"#;

    fn tmpdir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pite-cache-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_scene(dir: &Path, text: &str) -> PathBuf {
        let path = dir.join("main.pitescene");
        std::fs::write(&path, text).unwrap();
        path
    }

    fn cache_file_for(scene: &Path) -> PathBuf {
        let bytes = std::fs::read(scene).unwrap();
        cache_path_for(scene, &content_hash(&bytes))
    }

    #[test]
    fn binary_round_trip_preserves_doc() {
        let doc = crate::parse_scene_str(SCENE).unwrap();
        let bytes = std::fs::read("Cargo.toml")
            .map(|b| b.len() as u64)
            .unwrap_or(0);
        let (again, header) = decode(&encode(&doc, 0x1234, bytes)).unwrap();
        assert_eq!(doc, again);
        assert_eq!(header.cache_version, CACHE_VERSION);
        assert_eq!(header.format_version, FORMAT_VERSION);
    }

    #[test]
    fn miss_writes_cache_then_hit_reads() {
        let dir = tmpdir("miss-hit");
        let scene = write_scene(&dir, SCENE);
        let (doc, status, warning) = crate::load_cached(&scene).unwrap();
        assert_eq!(status, CacheStatus::Miss);
        assert!(
            warning.is_none(),
            "clean miss is transparent, got {warning:?}"
        );
        assert!(
            cache_file_for(&scene).is_file(),
            "miss must write the cache"
        );
        let (again, status, warning) = crate::load_cached(&scene).unwrap();
        assert_eq!(status, CacheStatus::Hit);
        assert!(warning.is_none(), "hit is transparent, got {warning:?}");
        assert_eq!(doc, again);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn source_change_invalidates_and_prunes() {
        let dir = tmpdir("invalidate");
        let scene = write_scene(&dir, SCENE);
        crate::load_cached(&scene).unwrap();
        let old_cache = cache_file_for(&scene);
        assert!(old_cache.is_file());
        let changed = format!(
            "{SCENE}\n[[node]]\nid = \"extra\"\ntype = \"Timer\"\nname = \"Extra\"\nparent = \"root\"\n"
        );
        std::fs::write(&scene, &changed).unwrap();
        let (doc, status, warning) = crate::load_cached(&scene).unwrap();
        assert_eq!(status, CacheStatus::Miss);
        assert!(
            warning.is_none(),
            "source change is a clean miss, got {warning:?}"
        );
        assert!(doc.node.iter().any(|n| n.id == "extra"));
        assert!(!old_cache.exists(), "stale hash file must be pruned");
        assert!(cache_file_for(&scene).is_file());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn cache_version_bump_rebuilds_loudly() {
        let dir = tmpdir("cache-bump");
        let scene = write_scene(&dir, SCENE);
        crate::load_cached(&scene).unwrap();
        let cpath = cache_file_for(&scene);
        let doc = crate::parse_scene_str(SCENE).unwrap();
        let bytes = std::fs::read(&scene).unwrap();
        let stale = encode_with(
            &doc,
            fnv1a_u64(&bytes),
            bytes.len() as u64,
            CACHE_VERSION + 1,
            FORMAT_VERSION,
        );
        std::fs::write(&cpath, &stale).unwrap();
        let (again, status, warning) = crate::load_cached(&scene).unwrap();
        assert!(matches!(status, CacheStatus::Rebuilt(_)), "got {status:?}");
        let warning = warning.expect("version bump must fall back loudly");
        assert!(warning.contains("cache version"), "got {warning:?}");
        assert_eq!(doc, again);
        let (_, status, warning) = crate::load_cached(&scene).unwrap();
        assert_eq!(status, CacheStatus::Hit);
        assert!(warning.is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn format_version_bump_rebuilds_loudly() {
        let dir = tmpdir("format-bump");
        let scene = write_scene(&dir, SCENE);
        crate::load_cached(&scene).unwrap();
        let cpath = cache_file_for(&scene);
        let doc = crate::parse_scene_str(SCENE).unwrap();
        let bytes = std::fs::read(&scene).unwrap();
        let stale = encode_with(
            &doc,
            fnv1a_u64(&bytes),
            bytes.len() as u64,
            CACHE_VERSION,
            FORMAT_VERSION + 1,
        );
        std::fs::write(&cpath, &stale).unwrap();
        let (again, status, warning) = crate::load_cached(&scene).unwrap();
        assert!(matches!(status, CacheStatus::Rebuilt(_)), "got {status:?}");
        let warning = warning.expect("format bump must fall back loudly");
        assert!(warning.contains("format_version"), "got {warning:?}");
        assert_eq!(doc, again);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn corrupt_cache_falls_back_loudly() {
        let dir = tmpdir("corrupt");
        let scene = write_scene(&dir, SCENE);
        crate::load_cached(&scene).unwrap();
        let cpath = cache_file_for(&scene);
        std::fs::write(&cpath, b"definitely not a scene cache").unwrap();
        let (doc, status, warning) = crate::load_cached(&scene).unwrap();
        assert!(matches!(status, CacheStatus::Rebuilt(_)), "got {status:?}");
        let warning = warning.expect("corruption must fall back loudly");
        assert!(warning.contains("corrupt"), "got {warning:?}");
        assert_eq!(doc, crate::parse_scene_str(SCENE).unwrap());
        let (_, status, warning) = crate::load_cached(&scene).unwrap();
        assert_eq!(status, CacheStatus::Hit);
        assert!(
            warning.is_none(),
            "rewritten cache must be valid, got {warning:?}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
