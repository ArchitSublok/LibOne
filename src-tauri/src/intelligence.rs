//! Optional local intelligence provider interface for future Phase 3 capabilities.
//!
//! Strict architectural boundaries:
//! - Zero network requests (offline only)
//! - No API keys
//! - No account setup
//! - No document payload uploads
//! - No default model downloads
//! - Deterministic metadata rules remain the default and fallback engine.

use serde::{Deserialize, Serialize};

/// Status report for the local intelligence subsystem.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalAiStatus {
    /// True only if a local offline provider is configured and ready.
    pub is_available: bool,
    /// Identifier of the provider (e.g. "DisabledLocalProvider").
    pub provider_name: String,
    /// User-facing status string. Returns "Local AI is not configured" when unavailable.
    pub status_message: String,
    /// Active local model name, if any (None when disabled).
    pub model_name: Option<String>,
    /// Security invariant: network access is strictly forbidden for all intelligence providers.
    pub allows_network: bool,
}

/// Abstract provider interface for optional local intelligence.
///
/// Implementations must run strictly on-device without outbound network connections,
/// credential prompts, or external telemetry.
pub trait LocalIntelligenceProvider: Send + Sync {
    /// Whether the local provider is available offline.
    fn is_available(&self) -> bool;

    /// Provider identifier for telemetry/logging purposes.
    fn provider_name(&self) -> &str;

    /// User-facing status explanation.
    fn status_message(&self) -> &str;

    /// Produce a complete status snapshot.
    fn status(&self) -> LocalAiStatus {
        LocalAiStatus {
            is_available: self.is_available(),
            provider_name: self.provider_name().to_string(),
            status_message: self.status_message().to_string(),
            model_name: None,
            allows_network: false,
        }
    }

    /// Optional local suggestion rationale enhancement.
    /// Returns None when the provider is disabled or unavailable.
    fn enhance_rationale(&self, _context: &str) -> Option<String> {
        None
    }
}

/// Default provider implementation when local AI is not configured.
/// Guaranteed to perform zero network calls, require no credentials, and download no models.
#[derive(Debug, Default, Clone)]
pub struct DisabledLocalProvider;

impl DisabledLocalProvider {
    pub fn new() -> Self {
        Self
    }
}

impl LocalIntelligenceProvider for DisabledLocalProvider {
    fn is_available(&self) -> bool {
        false
    }

    fn provider_name(&self) -> &str {
        "DisabledLocalProvider"
    }

    fn status_message(&self) -> &str {
        "Local AI is not configured"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_disabled_local_provider_invariants() {
        let provider = DisabledLocalProvider::new();
        assert!(!provider.is_available());
        assert_eq!(provider.provider_name(), "DisabledLocalProvider");
        assert_eq!(provider.status_message(), "Local AI is not configured");

        let status = provider.status();
        assert!(!status.is_available);
        assert_eq!(status.status_message, "Local AI is not configured");
        assert_eq!(status.provider_name, "DisabledLocalProvider");
        assert!(status.model_name.is_none());
        assert!(
            !status.allows_network,
            "Network access must be strictly disallowed"
        );

        assert!(provider.enhance_rationale("test context").is_none());
    }

    #[test]
    fn test_custom_mock_provider_follows_trait() {
        struct MockUnavailableProvider;
        impl LocalIntelligenceProvider for MockUnavailableProvider {
            fn is_available(&self) -> bool {
                false
            }
            fn provider_name(&self) -> &str {
                "MockProvider"
            }
            fn status_message(&self) -> &str {
                "Local AI is not configured"
            }
        }

        let p = MockUnavailableProvider;
        assert!(!p.is_available());
        let st = p.status();
        assert_eq!(st.status_message, "Local AI is not configured");
        assert!(!st.allows_network);
    }
}
