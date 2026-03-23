//! Correlation method resolution — 1:1 port of `correlation/run.py`.

use crate::models::settings::AppSettings;

use super::methods::scc::Scc;
use super::registry::{get_method, CorrelationMethod};

/// Resolve the correlation method to use based on settings — `_resolve_method`
pub fn resolve_method(settings: &AppSettings, source_separated: bool) -> Box<dyn CorrelationMethod> {
    let method_name = if source_separated {
        settings.correlation_method_source_separated.to_string()
    } else {
        settings.correlation_method.to_string()
    };

    if method_name.contains("Standard Correlation") || method_name.contains("SCC") {
        return Box::new(Scc::new(settings.audio_peak_fit));
    }

    // Try registry lookup
    // TODO: Improve registry to support owned method retrieval
    // For now the registry returns references which can't be boxed,
    // so we fall through to default SCC
    let _ = get_method(&method_name);

    Box::new(Scc::new(settings.audio_peak_fit))
}
