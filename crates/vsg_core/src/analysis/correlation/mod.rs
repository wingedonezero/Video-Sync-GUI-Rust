pub mod confidence;
pub mod decode;
pub mod dense;
pub mod filtering;
pub mod gpu_backend;
pub mod gpu_correlation;
pub mod methods;
pub mod registry;
pub mod run;

// Re-export commonly used items
pub use decode::{decode_audio, get_audio_stream_info, normalize_lang, DEFAULT_SR};
pub use filtering::{apply_bandpass, apply_lowpass};
pub use gpu_backend::cleanup_gpu;
pub use registry::{get_method, list_methods, register, CorrelationMethod};
