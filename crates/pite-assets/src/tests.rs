// SPDX-License-Identifier: MIT OR Apache-2.0
use super::*;
use crate::importers::PNG_SIG;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static TEST_SEQ: AtomicU64 = AtomicU64::new(0);

fn test_dir(name: &str) -> PathBuf {
    let seq = TEST_SEQ.fetch_add(1, Ordering::SeqCst);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!(
        "pite-assets-{name}-{}-{nanos}-{seq}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("create temp test dir");
    dir
}

fn cleanup(dir: &Path) {
    let _ = std::fs::remove_dir_all(dir);
}

fn valid_png(extra: &[u8]) -> Vec<u8> {
    [PNG_SIG.as_slice(), extra].concat()
}

fn valid_wav() -> Vec<u8> {
    [
        b"RIFF".as_slice(),
        &[0x24, 0x00, 0x00, 0x00],
        b"WAVE".as_slice(),
        &[0x00, 0x01, 0x02, 0x03],
    ]
    .concat()
}

#[test]
fn rename_tracks_content_hash() {
    let root = test_dir("rename");
    let old_fs = root.join("a.png");
    std::fs::write(&old_fs, valid_png(b"hello")).unwrap();

    let (reg1, rep1) = scan_project(&root).unwrap();
    assert_eq!(rep1.added, vec!["res://a.png".to_string()]);
    assert!(rep1.renamed.is_empty());
    let hash1 = reg1.get("res://a.png").unwrap().hash.clone();

    std::fs::rename(&old_fs, root.join("b.png")).unwrap();
    let (reg2, rep2) = scan_project(&root).unwrap();
    assert_eq!(
        rep2.renamed,
        vec![("res://a.png".to_string(), "res://b.png".to_string())]
    );
    assert!(rep2.added.is_empty());
    assert!(rep2.removed.is_empty());
    let hash2 = reg2.get("res://b.png").unwrap().hash.clone();
    assert_eq!(hash1, hash2);
    assert!(reg2.get("res://a.png").is_none());

    cleanup(&root);
}

#[test]
fn removed_is_reported() {
    let root = test_dir("removed");
    std::fs::write(root.join("gone.png"), valid_png(b"x")).unwrap();
    let (_, rep1) = scan_project(&root).unwrap();
    assert_eq!(rep1.added.len(), 1);

    std::fs::remove_file(root.join("gone.png")).unwrap();
    let (reg2, rep2) = scan_project(&root).unwrap();
    assert_eq!(rep2.removed, vec!["res://gone.png".to_string()]);
    assert!(rep2.added.is_empty());
    assert!(reg2.get("res://gone.png").is_none());

    cleanup(&root);
}

#[test]
fn reimport_runs_on_changed_content() {
    let root = test_dir("reimport");
    std::fs::write(root.join("sprite.png"), valid_png(b"v1")).unwrap();
    let (reg1, _) = scan_project(&root).unwrap();
    let hash1 = reg1
        .iter()
        .find(|e| e.path == "res://sprite.png")
        .unwrap()
        .hash
        .clone();

    std::fs::write(root.join("sprite.png"), valid_png(b"v2-longer")).unwrap();
    let (reg2, report) = reimport_project(&root).unwrap();
    assert_eq!(report.reimported, vec!["res://sprite.png".to_string()]);
    assert!(report.failed.is_empty());
    let hash2 = reg2
        .iter()
        .find(|e| e.path == "res://sprite.png")
        .unwrap()
        .hash
        .clone();
    assert_ne!(hash1, hash2);
    assert_eq!(hash2, fnv1a_hex(&valid_png(b"v2-longer")));

    cleanup(&root);
}

#[test]
fn importers_reject_bad_magic() {
    let root = test_dir("magic");
    let bad_png = root.join("bad.png");
    std::fs::write(&bad_png, b"definitely not a png").unwrap();
    assert!(PngImporter
        .import(bad_png.to_string_lossy().as_ref())
        .is_err());
    assert!(PngImporter
        .import(root.join("missing.png").to_string_lossy().as_ref())
        .is_err());
    let good_png = root.join("good.png");
    std::fs::write(&good_png, valid_png(b"payload")).unwrap();
    assert!(PngImporter
        .import(good_png.to_string_lossy().as_ref())
        .is_ok());

    let bad_wav = root.join("bad.wav");
    std::fs::write(&bad_wav, b"RIFFxxNOTWAVE!").unwrap();
    assert!(WavImporter
        .import(bad_wav.to_string_lossy().as_ref())
        .is_err());
    let short_wav = root.join("short.wav");
    std::fs::write(&short_wav, b"RIFF").unwrap();
    assert!(WavImporter
        .import(short_wav.to_string_lossy().as_ref())
        .is_err());
    let good_wav = root.join("good.wav");
    std::fs::write(&good_wav, valid_wav()).unwrap();
    assert!(WavImporter
        .import(good_wav.to_string_lossy().as_ref())
        .is_ok());

    assert!(WavImporter.match_ext("wav"));
    assert!(WavImporter.match_ext("WAV"));
    assert!(!WavImporter.match_ext("png"));

    // Ogg tests
    let bad_ogg = root.join("bad.ogg");
    std::fs::write(&bad_ogg, b"NOT_OGG").unwrap();
    assert!(OggImporter
        .import(bad_ogg.to_string_lossy().as_ref())
        .is_err());
    let good_ogg = root.join("good.ogg");
    std::fs::write(&good_ogg, b"OggS\x00\x02extra").unwrap();
    assert!(OggImporter
        .import(good_ogg.to_string_lossy().as_ref())
        .is_ok());
    assert!(OggImporter.match_ext("ogg"));
    assert!(OggImporter.match_ext("OGG"));

    // MP3 tests
    let bad_mp3 = root.join("bad.mp3");
    std::fs::write(&bad_mp3, b"NOT_MP3").unwrap();
    assert!(Mp3Importer
        .import(bad_mp3.to_string_lossy().as_ref())
        .is_err());
    let good_mp3_id3 = root.join("good_id3.mp3");
    std::fs::write(&good_mp3_id3, b"ID3\x04\x00").unwrap();
    assert!(Mp3Importer
        .import(good_mp3_id3.to_string_lossy().as_ref())
        .is_ok());
    let good_mp3_sync = root.join("good_sync.mp3");
    std::fs::write(&good_mp3_sync, [0xFF, 0xFB, 0x90, 0x64]).unwrap();
    assert!(Mp3Importer
        .import(good_mp3_sync.to_string_lossy().as_ref())
        .is_ok());
    assert!(Mp3Importer.match_ext("mp3"));
    assert!(Mp3Importer.match_ext("MP3"));

    // FLAC tests
    let bad_flac = root.join("bad.flac");
    std::fs::write(&bad_flac, b"NOT_FLAC").unwrap();
    assert!(FlacImporter
        .import(bad_flac.to_string_lossy().as_ref())
        .is_err());
    let good_flac = root.join("good.flac");
    std::fs::write(&good_flac, b"fLaC\x00\x00").unwrap();
    assert!(FlacImporter
        .import(good_flac.to_string_lossy().as_ref())
        .is_ok());
    assert!(FlacImporter.match_ext("flac"));
    assert!(FlacImporter.match_ext("FLAC"));

    let importers = default_importers();
    assert_eq!(importers.len(), 5);

    cleanup(&root);
}

#[test]
fn replace_res_ref_only_touches_quoted() {
    let text = "texture = \"res://assets/player.png\"\n\
            pite.play('res://assets/player.png')\n\
            bare res://assets/player.png stays\n";
    let (out, count) = replace_res_ref(text, "res://assets/player.png", "res://assets/hero.png");
    assert_eq!(count, 2);
    assert!(out.contains("\"res://assets/hero.png\""));
    assert!(out.contains("'res://assets/hero.png'"));
    assert!(out.contains("bare res://assets/player.png stays"));
    assert!(!out.contains("\"res://assets/player.png\""));
    assert!(!out.contains("'res://assets/player.png'"));

    let (same, zero) = replace_res_ref("nothing here res://x", "res://x", "res://y");
    assert_eq!(zero, 0);
    assert_eq!(same, "nothing here res://x");
}

#[test]
fn manifest_round_trip_is_sorted() {
    let root = test_dir("manifest");
    std::fs::write(root.join("b.png"), valid_png(b"b")).unwrap();
    std::fs::write(root.join("a.png"), valid_png(b"a")).unwrap();
    std::fs::write(root.join("hit.wav"), valid_wav()).unwrap();
    let (reg1, _) = scan_project(&root).unwrap();
    assert_eq!(reg1.len(), 3);
    assert!(!reg1.is_empty());

    let text = std::fs::read_to_string(root.join(MANIFEST_FILE)).unwrap();
    let pos_a = text.find("res://a.png").expect("a.png in manifest");
    let pos_b = text.find("res://b.png").expect("b.png in manifest");
    let pos_w = text.find("res://hit.wav").expect("hit.wav in manifest");
    assert!(pos_a < pos_b);
    assert!(pos_b < pos_w);

    let reg2 = load_registry(&root).unwrap();
    assert_eq!(reg2.len(), 3);
    for entry in reg1.iter() {
        let again = reg2
            .iter()
            .find(|e| e.path == entry.path)
            .expect("entry survives round-trip");
        assert_eq!(again.hash, entry.hash);
        assert_eq!(again.size, entry.size);
    }

    let empty_root = test_dir("manifest-empty");
    let empty = load_registry(&empty_root).unwrap();
    assert!(empty.is_empty());
    assert_eq!(empty.len(), 0);

    cleanup(&root);
    cleanup(&empty_root);
}

#[test]
fn scan_skips_hidden_and_target() {
    let root = test_dir("skips");
    std::fs::create_dir_all(root.join(".hidden")).unwrap();
    std::fs::create_dir_all(root.join("target")).unwrap();
    std::fs::write(root.join(".hidden").join("s.png"), valid_png(b"s")).unwrap();
    std::fs::write(root.join("target").join("t.png"), valid_png(b"t")).unwrap();
    std::fs::write(root.join(".dot.png"), valid_png(b"d")).unwrap();
    std::fs::write(root.join("keep.png"), valid_png(b"k")).unwrap();
    std::fs::write(root.join("notes.txt"), b"not an asset").unwrap();

    let files = scan_files(&root).unwrap();
    let paths: Vec<&str> = files.iter().map(|f| f.res_path.as_str()).collect();
    assert_eq!(paths, vec!["res://keep.png"]);

    cleanup(&root);
}

#[test]
fn apply_renames_rewrites_scene_and_script() {
    let root = test_dir("renames");
    std::fs::write(root.join("old.png"), valid_png(b"o")).unwrap();
    let (_, rep1) = scan_project(&root).unwrap();
    assert_eq!(rep1.added, vec!["res://old.png".to_string()]);

    std::fs::rename(root.join("old.png"), root.join("new.png")).unwrap();
    std::fs::write(root.join("main.ptscn"), "texture = \"res://old.png\"\n").unwrap();
    std::fs::write(
        root.join("play.py"),
        "pite.play('res://old.png')\nbare res://old.png\n",
    )
    .unwrap();

    let (reg, report) = reimport_project(&root).unwrap();
    assert_eq!(
        report.refs_updated,
        vec!["res://main.ptscn".to_string(), "res://play.py".to_string()]
    );
    let scene = std::fs::read_to_string(root.join("main.ptscn")).unwrap();
    assert!(scene.contains("\"res://new.png\""));
    let script = std::fs::read_to_string(root.join("play.py")).unwrap();
    assert!(script.contains("'res://new.png'"));
    assert!(script.contains("bare res://old.png"));
    assert!(reg.get("res://new.png").is_some());

    cleanup(&root);
}

#[test]
fn legacy_manifest_with_uid_is_supported_and_stripped_on_save() {
    let root = test_dir("legacy_uid");
    let legacy_toml = r#"
[[asset]]
path = "res://a.png"
uid = "0123456789abcdef"
hash = "e3b0c44298fc1c14"
size = 123
"#;
    std::fs::write(root.join(MANIFEST_FILE), legacy_toml).unwrap();
    let reg = load_registry(&root).unwrap();
    assert_eq!(reg.len(), 1);
    let entry = reg.get("res://a.png").unwrap();
    assert_eq!(entry.path, "res://a.png");
    assert_eq!(entry.hash, "e3b0c44298fc1c14");
    assert_eq!(entry.size, 123);

    save_registry(&root, &reg).unwrap();
    let saved = std::fs::read_to_string(root.join(MANIFEST_FILE)).unwrap();
    assert!(!saved.contains("uid ="));
    assert!(saved.contains("res://a.png"));
    assert!(saved.contains("e3b0c44298fc1c14"));
    cleanup(&root);
}

#[test]
fn played_assets_extracts_play_literals_and_strips_comments() {
    let src = r#"
    pite.play("res://a.wav")
    pite.play('res://b.ogg')
    # pite.play("res://commented.wav")
    x = display("res://not_play.wav")
    "#;
    assert_eq!(played_assets(src), vec!["res://a.wav", "res://b.ogg"]);
}

#[test]
fn reimport_migrates_legacy_atlas_json_to_ptatlas() {
    let root = test_dir("legacy_atlas_migration");
    std::fs::write(root.join("sheet.png"), valid_png(b"s")).unwrap();
    let legacy_json = r#"{
            "texture": "sheet.png",
            "size": [64, 32],
            "frames": {
                "player": { "x": 0, "y": 0, "w": 32, "h": 32 }
            }
        }"#;
    std::fs::write(root.join("sheet.atlas.json"), legacy_json).unwrap();
    std::fs::write(
        root.join("main.ptscn"),
        "atlas = \"res://sheet.atlas.json\"\n",
    )
    .unwrap();
    std::fs::write(
        root.join("level.ptmap"),
        "atlas = \"res://sheet.atlas.json\"\n",
    )
    .unwrap();

    assert_eq!(find_legacy_atlases(&root), vec!["sheet.atlas.json"]);

    let (_reg, report) = reimport_project(&root).unwrap();
    assert!(!root.join("sheet.atlas.json").exists());
    assert!(root.join("sheet.ptatlas").exists());
    assert!(find_legacy_atlases(&root).is_empty());

    let scene = std::fs::read_to_string(root.join("main.ptscn")).unwrap();
    assert!(scene.contains("\"res://sheet.ptatlas\""));
    let tilemap = std::fs::read_to_string(root.join("level.ptmap")).unwrap();
    assert!(tilemap.contains("\"res://sheet.ptatlas\""));

    assert_eq!(
        report.refs_updated,
        vec![
            "res://level.ptmap".to_string(),
            "res://main.ptscn".to_string()
        ]
    );

    cleanup(&root);
}
