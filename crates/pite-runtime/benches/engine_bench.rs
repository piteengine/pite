// SPDX-License-Identifier: MIT OR Apache-2.0
//! Baseline engine & GIL benchmarks for `pite-runtime`.
//!
//! Run with:
//! `cargo bench -p pite-runtime --locked`

#![allow(clippy::unwrap_used)]

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use pite_core::{global_positions, NodeDesc, NodeId, NodeTree, Props};
use pite_runtime::{GameSession, SessionOptions};

fn report(name: &str, iters: usize, samples: &mut [Duration]) {
    samples.sort_unstable();
    let total: Duration = samples.iter().sum();
    let mean = total / iters as u32;
    let p50 = samples[samples.len() / 2];
    let p95 = samples[(samples.len() * 95) / 100];
    println!(
        "{name:<40} mean={:>8.2} us   p50={:>8.2} us   p95={:>8.2} us",
        mean.as_secs_f64() * 1e6,
        p50.as_secs_f64() * 1e6,
        p95.as_secs_f64() * 1e6,
    );
}

fn bench<F: FnMut()>(name: &str, warmup: usize, iters: usize, mut f: F) {
    for _ in 0..warmup {
        f();
    }
    let mut samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        let t0 = Instant::now();
        f();
        samples.push(t0.elapsed());
    }
    report(name, iters, &mut samples);
}

fn bench_node_tree() {
    let mut tree = NodeTree::new();
    tree.insert(NodeDesc {
        id: NodeId::from("root"),
        name: "root".to_string(),
        type_name: "Node2D".to_string(),
        parent: None,
        props: Props::default(),
        script: None,
    })
    .unwrap();
    for i in 0..999 {
        let parent = if i < 10 {
            "root".to_string()
        } else {
            format!("n_{}", i % 10)
        };
        tree.insert(NodeDesc {
            id: NodeId::from(format!("n_{i}")),
            name: format!("n_{i}"),
            type_name: "Node2D".to_string(),
            parent: Some(NodeId::from(parent)),
            props: Props::default(),
            script: None,
        })
        .unwrap();
    }

    bench("node_tree_global_positions_1000", 50, 500, || {
        let map = global_positions(&tree);
        std::hint::black_box(map);
    });
}

fn make_scripted_project(tag: &str, count: usize, method: &str, body: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pite-bench-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("scenes")).unwrap();
    std::fs::create_dir_all(dir.join("scripts")).unwrap();

    std::fs::write(
        dir.join("pite.toml"),
        "[project]\nname = \"bench\"\npite_version = \"0.5.0\"\nmain_scene = \"res://scenes/main.ptscn\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("scripts/mover.py"),
        format!(
            "import pite\n\nclass Mover(pite.Node2D):\n    def {method}(self, delta: float) -> None:\n        {body}\n"
        ),
    )
    .unwrap();

    let mut scene = String::from(
        "format_version = 1\nroot = \"root\"\n\n[[node]]\nid = \"root\"\nname = \"root\"\ntype = \"Node2D\"\n",
    );
    for i in 0..count {
        scene.push_str(&format!(
            "\n[[node]]\nid = \"n_{i}\"\nname = \"n_{i}\"\ntype = \"Node2D\"\nparent = \"root\"\nscript = {{ path = \"res://scripts/mover.py\", class = \"Mover\" }}\n"
        ));
    }
    std::fs::write(dir.join("scenes/main.ptscn"), scene).unwrap();
    dir
}

fn bench_session_frames() {
    for (label, count, method, body, iters) in [
        (
            "game_session_frame_0_nodes",
            0usize,
            "_process",
            "pass",
            500usize,
        ),
        (
            "game_session_frame_100_process",
            100,
            "_process",
            "self.position.x += 10.0 * delta",
            200,
        ),
        ("game_session_frame_500_noop", 500, "_process", "pass", 100),
        (
            "game_session_frame_500_process",
            500,
            "_process",
            "self.position.x += 10.0 * delta",
            100,
        ),
        (
            "game_session_frame_500_physics",
            500,
            "_physics_process",
            "self.position.x += 10.0 * delta",
            100,
        ),
    ] {
        let dir = make_scripted_project(label, count, method, body);
        let mut session = GameSession::open_with_options(
            &dir.join("scenes/main.ptscn"),
            SessionOptions {
                no_reload: true,
                safe_mode: false,
                allow_plugin_errors: false,
            },
        )
        .unwrap();
        bench(label, 20, iters, || {
            session.frame(1.0 / 60.0);
        });
        let _ = std::fs::remove_dir_all(&dir);
    }
}

fn bench_input_actions() {
    let mut map = HashMap::new();
    map.insert(
        "move_left".to_string(),
        vec![vec!["A".to_string()], vec!["ArrowLeft".to_string()]],
    );
    map.insert(
        "move_right".to_string(),
        vec![vec!["D".to_string()], vec!["ArrowRight".to_string()]],
    );
    map.insert("move_up".to_string(), vec![vec!["W".to_string()]]);
    map.insert("move_down".to_string(), vec![vec!["S".to_string()]]);
    map.insert(
        "sprint_jump".to_string(),
        vec![vec!["Shift".to_string(), "Space".to_string()]],
    );
    pite_script::input_set_actions(map);
    pite_script::input_begin_frame();
    pite_script::input_set_key("D", true);
    pite_script::input_set_key("Shift", true);
    pite_script::input_set_key("Space", true);

    bench("input_action_eval_1000", 50, 300, || {
        for _ in 0..1000 {
            let held = pite_script::action_held("sprint_jump");
            let vec = pite_script::action_vector("move_left", "move_right", "move_up", "move_down");
            std::hint::black_box((held, vec));
        }
    });

    pite_script::input_clear();
    pite_script::input_set_actions(HashMap::new());
}

fn bench_physics_world() {
    use pite_physics::PhysicsWorld;

    let mut world = PhysicsWorld::new();
    for i in 0..200 {
        let x = (i % 20) as f32 * 40.0 - 400.0;
        let y = (i / 20) as f32 * 40.0 - 200.0;
        world.add_rigid_body(NodeId::from(format!("b_{i}")), (x, y), 1.0, 1.0, 0.5, false);
    }

    bench("physics_world_step_200_bodies", 20, 200, || {
        world.step();
    });
}

fn bench_scene_loading() {
    let dir = make_scripted_project("scene_load", 200, "_process", "pass");
    let scene_path = dir.join("scenes/main.ptscn");

    bench("scene_load_toml_200_nodes", 5, 50, || {
        let doc = pite_scene::load_scene(&scene_path).unwrap();
        std::hint::black_box(doc);
    });

    // Warm the binary cache, then benchmark cache hits.
    let _ = pite_scene::load_scene_cached(&scene_path).unwrap();
    bench("scene_load_cached_hit_200_nodes", 20, 200, || {
        let doc = pite_scene::load_scene_cached(&scene_path).unwrap();
        std::hint::black_box(doc);
    });

    let _ = std::fs::remove_dir_all(&dir);
}

fn bench_tweens_500() {
    let dir = std::env::temp_dir().join(format!("pite-bench-tweens-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("scenes")).unwrap();
    std::fs::create_dir_all(dir.join("scripts")).unwrap();

    std::fs::write(
        dir.join("pite.toml"),
        "[project]\nname = \"bench\"\npite_version = \"0.5.0\"\nmain_scene = \"res://scenes/main.ptscn\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("scripts/tween_setup.py"),
        "import pite\n\nclass TweenSetup(pite.Node2D):\n    def _ready(self) -> None:\n        for i in range(500):\n            pite.tween().property(f\"n_{i}\", \"position\", (200.0, 0.0), 10.0).loops(0)\n",
    )
    .unwrap();

    let mut scene = String::from(
        "format_version = 1\nroot = \"root\"\n\n[[node]]\nid = \"root\"\nname = \"root\"\ntype = \"Node2D\"\nscript = { path = \"res://scripts/tween_setup.py\", class = \"TweenSetup\" }\n",
    );
    for i in 0..500 {
        scene.push_str(&format!(
            "\n[[node]]\nid = \"n_{i}\"\nname = \"n_{i}\"\ntype = \"Node2D\"\nparent = \"root\"\n"
        ));
    }
    std::fs::write(dir.join("scenes/main.ptscn"), scene).unwrap();

    let mut session = GameSession::open_with_options(
        &dir.join("scenes/main.ptscn"),
        SessionOptions {
            no_reload: true,
            safe_mode: false,
            allow_plugin_errors: false,
        },
    )
    .unwrap();
    bench("game_session_frame_500_tweens", 20, 100, || {
        session.frame(1.0 / 60.0);
    });
    pite_script::tween_clear();
    let _ = std::fs::remove_dir_all(&dir);
}

fn main() {
    println!("Pite Baseline Benchmarks (release)");
    println!("--------------------------------------------------------------------------");
    bench_node_tree();
    bench_session_frames();
    bench_tweens_500();
    bench_physics_world();
    bench_scene_loading();
    bench_input_actions();
}
