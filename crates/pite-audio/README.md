# pite-audio

Audio decoding, multi-voice mixing, and hardware output subsystem for the [Pite](https://github.com/piteengine/pite) game engine.

Provides pure-Rust audio decoding via `symphonia` for WAV, Ogg Vorbis (`.ogg`), MP3 (`.mp3`), and FLAC (`.flac`), with multi-voice mixing and hardware output via `cpal` (and automatic silent-simulation fallback when no hardware audio device is available).

## Versioning

Published in lockstep with the engine: `pite-audio 0.x.y` corresponds to engine version `0.x.y`.

## Documentation

- Documentation & Guides: <https://piteengine.github.io/pite>
- API Docs: <https://docs.rs/pite-audio>
- Engine Repository: <https://github.com/piteengine/pite>

## License

Dual-licensed MIT + Apache-2.0 ([LICENSE-MIT](https://github.com/piteengine/pite/blob/main/LICENSE-MIT), [LICENSE-APACHE](https://github.com/piteengine/pite/blob/main/LICENSE-APACHE)).
