pub mod gcc_phat;
pub mod gcc_scot;
pub mod gcc_whiten;
pub mod onset;
pub mod scc;
pub mod spectrogram;

/// Register all built-in correlation methods.
pub fn register_all() {
    use super::registry::register;

    register(Box::new(scc::Scc::new(false)));
    register(Box::new(gcc_phat::GccPhat));
    register(Box::new(onset::OnsetDetection));
    register(Box::new(gcc_scot::GccScot));
    register(Box::new(gcc_whiten::GccWhiten));
    register(Box::new(spectrogram::SpectrogramCorrelation));
}
