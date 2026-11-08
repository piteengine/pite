# Node Reference

Every node type in the engine, derived from `pite_core::NODE_SCHEMAS` at build time. Do not edit these files by hand — `pite-editor/build.rs` rewrites them whenever the schema changes.

- [`Area2D`](Area2D.md) — Trigger zone for overlap detection. Emits body_entered/body_exited and area_* signals.
- [`Button`](Button.md) — Clickable rectangle with a caption. Fires the engine-owned `pressed` signal.
- [`Camera2D`](Camera2D.md) — Frames the view: the screen centers on this node's global position.
- [`CharacterBody2D`](CharacterBody2D.md) — Kinematic body driven by script via `move_and_slide()`. Slides along obstacles, ignores sensors.
- [`CollisionShape2D`](CollisionShape2D.md) — Geometric collider attached to a parent physics body or Area2D.
- [`Label`](Label.md) — Draws text centered on its global position.
- [`Node`](Node.md) — Base class for all scene objects. Does not have a 2D transform.
- [`Node2D`](Node2D.md) — 2D game object with position and rotation.
- [`RigidBody2D`](RigidBody2D.md) — Dynamic physics body simulated by gravity, impulses, and forces.
- [`Sprite2D`](Sprite2D.md) — Displays a 2D texture or one frame from a spritesheet atlas.
- [`StaticBody2D`](StaticBody2D.md) — Immovable physics body: floors, walls, platforms. Never responds to impulses.
- [`Timer`](Timer.md) — Fires the engine-owned `timeout` signal every `wait_time` seconds.
