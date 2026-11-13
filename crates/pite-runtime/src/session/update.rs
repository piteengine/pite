// SPDX-License-Identifier: MIT OR Apache-2.0

use pite_core::{global_position, PropValue};
use pite_physics::PhysicsEvent;

use super::GameSession;

impl GameSession {
    pub(crate) fn update(&mut self, delta: f64) {
        pite_audio::poll_audio();

        // Install the current camera pose so `pite.screen_to_world` (used by
        // `PyButton::_process` to hit-test clicks against world-space button
        // positions) sees this frame's camera before the `_process` loop.
        let ((cx, cy), _) = self.camera_view();
        pite_script::set_camera_view((cx, cy), self.effective_zoom(), self.viewport);

        const FIXED_STEP: f64 = 1.0 / 60.0;
        self.physics_accumulator += delta;
        if self.physics_accumulator > 0.25 {
            self.physics_accumulator = 0.25;
        }

        while self.physics_accumulator >= FIXED_STEP {
            for slot in &mut self.instances {
                if slot.errored || !slot.has_physics_process {
                    continue;
                }
                if let Err(e) = slot.backend.call_physics_process(FIXED_STEP) {
                    tracing::error!(node = %slot.id, "physics process error: {e:#}");
                    slot.errored = true;
                    if self.pause_on_error && self.last_error.is_none() {
                        self.last_error = slot.backend.last_script_error();
                    }
                }
                let pos = self.host.with_tree(|t| {
                    match t.get(&slot.id).and_then(|n| n.props.get("position")) {
                        Some(PropValue::Vec2(x, y)) => Some((*x, *y)),
                        _ => None,
                    }
                });
                if let Some((x, y)) = pos {
                    let mut lock = self.physics.lock().unwrap();
                    lock.set_body_position(&slot.id, (x, y));
                }
            }

            // Sync tree positions for non-rigid bodies/areas into physics before step
            self.host.with_tree(|tree| {
                let mut lock = self.physics.lock().unwrap();
                for node in tree.iter() {
                    let id = &node.id;
                    if !lock.is_rigid_body(id) {
                        let pos = global_position(tree, id);
                        lock.set_body_position(id, pos);
                    }
                }
            });

            let events = {
                let mut lock = self.physics.lock().unwrap();
                lock.step();
                lock.drain_events()
            };

            {
                let lock = self.physics.lock().unwrap();
                let mut moved = Vec::new();
                for slot in &self.instances {
                    if lock.is_rigid_body(&slot.id) {
                        if let Some(pos) = lock.get_body_position(&slot.id) {
                            moved.push((slot.id.clone(), pos));
                        }
                    }
                }
                drop(lock);
                for (id, (x, y)) in moved {
                    self.host.with_tree_mut(|tree| {
                        if let Some(node) = tree.get_mut(&id) {
                            node.props
                                .insert("position".to_string(), PropValue::Vec2(x, y));
                        }
                    });
                }
            }

            for ev in events {
                match ev {
                    PhysicsEvent::BodyEntered { area_id, body_id } => {
                        if let Err(e) = pite_script::fire_body_entered(
                            &self.host,
                            area_id.as_str(),
                            body_id.as_str(),
                        ) {
                            tracing::error!(area = %area_id, body = %body_id, "body_entered error: {e:#}");
                        }
                    }
                    PhysicsEvent::BodyExited { area_id, body_id } => {
                        if let Err(e) = pite_script::fire_body_exited(
                            &self.host,
                            area_id.as_str(),
                            body_id.as_str(),
                        ) {
                            tracing::error!(area = %area_id, body = %body_id, "body_exited error: {e:#}");
                        }
                    }
                    PhysicsEvent::AreaEntered {
                        area_id,
                        other_area_id,
                    } => {
                        if let Err(e) = pite_script::fire_area_entered(
                            &self.host,
                            area_id.as_str(),
                            other_area_id.as_str(),
                        ) {
                            tracing::error!(area = %area_id, other = %other_area_id, "area_entered error: {e:#}");
                        }
                    }
                    PhysicsEvent::AreaExited {
                        area_id,
                        other_area_id,
                    } => {
                        if let Err(e) = pite_script::fire_area_exited(
                            &self.host,
                            area_id.as_str(),
                            other_area_id.as_str(),
                        ) {
                            tracing::error!(area = %area_id, other = %other_area_id, "area_exited error: {e:#}");
                        }
                    }
                }
            }

            self.physics_accumulator -= FIXED_STEP;
        }

        // `_process` loop: iterates only slots that actually define
        // `_process` (via the probe cached in `InstanceSlot`). Base `Timer`
        // and `Button` now carry engine-owned `_process` here — no
        // `tick_timers`/`poll_buttons` dispatch.
        for slot in &mut self.instances {
            if slot.errored || !slot.has_process {
                continue;
            }
            if let Err(e) = slot.backend.call_process(delta) {
                tracing::error!(node = %slot.id, "{e:#}");
                slot.errored = true;
                if self.pause_on_error && self.last_error.is_none() {
                    self.last_error = slot.backend.last_script_error();
                }
            }
        }

        self.tick_animations(delta);
    }

    fn tick_animations(&mut self, delta: f64) {
        let nodes: Vec<(String, String, String, i64, f64, Option<bool>)> =
            self.host.with_tree(|tree| {
                tree.iter()
                    .filter(|n| n.type_name == "AnimatedSprite2D")
                    .filter_map(|n| {
                        let playing = match n.props.get("playing") {
                            Some(PropValue::Bool(b)) => *b,
                            _ => false,
                        };
                        if !playing {
                            return None;
                        }
                        let atlas = match n.props.get("atlas") {
                            Some(PropValue::Str(s)) if !s.is_empty() => s.clone(),
                            _ => return None,
                        };
                        let animation = match n.props.get("animation") {
                            Some(PropValue::Str(s)) if !s.is_empty() => s.clone(),
                            _ => match n.props.get("autoplay") {
                                Some(PropValue::Str(s)) if !s.is_empty() => s.clone(),
                                _ => return None,
                            },
                        };
                        let frame = match n.props.get("frame") {
                            Some(PropValue::Int(i)) => *i,
                            Some(PropValue::Num(f)) => *f as i64,
                            _ => 0,
                        };
                        let speed_scale = match n.props.get("speed_scale") {
                            Some(PropValue::Num(f)) => *f,
                            Some(PropValue::Int(i)) => *i as f64,
                            _ => 1.0,
                        };
                        let loop_override = match n.props.get("_loop_override") {
                            Some(PropValue::Bool(b)) => Some(*b),
                            _ => None,
                        };
                        Some((
                            n.id.to_string(),
                            atlas,
                            animation,
                            frame,
                            speed_scale,
                            loop_override,
                        ))
                    })
                    .collect()
            });

        for (id, atlas, animation, mut frame, speed_scale, loop_override) in nodes {
            if speed_scale == 0.0 {
                continue;
            }
            let sidecar = self.resolve_path(&atlas);
            let atlas_obj = match self.atlas_cache.entry(sidecar.clone()) {
                std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
                std::collections::hash_map::Entry::Vacant(e) => {
                    match pite_render::atlas::load_atlas(&sidecar) {
                        Ok(a) => e.insert(a),
                        Err(_) => continue,
                    }
                }
            };

            let (frames_len, fps, looping) = match atlas_obj.animation(&animation) {
                Ok(a) => (a.frames.len() as i64, a.fps, a.looping),
                Err(_) => continue,
            };

            if frames_len == 0 || fps <= 0.0 {
                continue;
            }

            let is_looping = loop_override.unwrap_or(looping);
            let frame_duration = 1.0 / (fps as f64 * speed_scale.abs());
            let elapsed = self.animation_times.entry(id.clone()).or_insert(0.0);
            *elapsed += delta;

            let mut stopped = false;
            let mut looped = false;

            while *elapsed >= frame_duration {
                *elapsed -= frame_duration;
                if speed_scale >= 0.0 {
                    frame += 1;
                    if frame >= frames_len {
                        if is_looping {
                            frame = 0;
                            looped = true;
                        } else {
                            frame = frames_len - 1;
                            stopped = true;
                            *elapsed = 0.0;
                            break;
                        }
                    }
                } else {
                    frame -= 1;
                    if frame < 0 {
                        if is_looping {
                            frame = frames_len - 1;
                            looped = true;
                        } else {
                            frame = 0;
                            stopped = true;
                            *elapsed = 0.0;
                            break;
                        }
                    }
                }
            }

            self.host.set_prop(&id, "frame", PropValue::Int(frame));
            if stopped {
                self.host.set_prop(&id, "playing", PropValue::Bool(false));
                if let Err(e) = pite_script::fire_animation_finished(&self.host, &id, &animation) {
                    tracing::error!(node = %id, "animation_finished error: {e:#}");
                }
            } else if looped {
                if let Err(e) = pite_script::fire_animation_looped(&self.host, &id, &animation) {
                    tracing::error!(node = %id, "animation_looped error: {e:#}");
                }
            }
        }
    }
}
