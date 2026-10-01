use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;

#[pyfunction]
#[pyo3(signature = (path, volume = 1.0))]
pub fn play(path: &str, volume: f32) -> PyResult<u64> {
    pite_audio::play_file(path, volume).map_err(|e| PyRuntimeError::new_err(e.to_string()))
}

#[pyfunction]
pub fn stop(id: u64) {
    pite_audio::stop_voice(id);
}

#[pyfunction]
pub fn set_volume(id: u64, volume: f32) {
    pite_audio::set_voice_volume(id, volume);
}

#[pyfunction]
pub fn is_playing(id: u64) -> bool {
    pite_audio::voice_playing(id)
}
