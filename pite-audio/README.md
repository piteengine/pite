# pite-audio

Audio decode and output subsystem for the [Pite](https://github.com/piteengine/pite) game engine.

Pure-Rust audio decoding powered by `symphonia` for WAV, Ogg Vorbis (`.ogg`), MP3 (`.mp3`), and FLAC (`.flac`), with multi-voice mixing and hardware output via `cpal` (and automatic silent-simulation fallback when no hardware device is available).

## Versioning

Lockstep with the engine: `pite-audio 0.x.y` is engine `0.x.y`.

## Docs

- API docs: <https://docs.rs/pite-audio>
- Engine repo: <https://github.com/piteengine/pite>

## License

Dual-licensed MIT + Apache-2.0 ([LICENSE-MIT](https://github.com/piteengine/pite/blob/main/LICENSE-MIT), [LICENSE-APACHE](https://github.com/piteengine/pite/blob/main/LICENSE-APACHE)).
