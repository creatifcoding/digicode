//! Typed invalidation signals for provider configuration and model catalogs.
//!
//! Provider routes, model context metadata, authentication-derived availability,
//! and the `/model` picker all read shared process state.  A route memo or picker
//! cache therefore needs a signal that says *why* its inputs changed instead of
//! guessing from unrelated TTLs.  This module is the small in-process contract
//! shared by those producers and future live-refresh consumers.

use std::sync::{OnceLock, atomic::Ordering};

use tokio::sync::broadcast;

const EVENT_CHANNEL_CAPACITY: usize = 64;

/// Producer responsible for a provider/model-catalog invalidation.
///
/// The variants intentionally describe stable boundaries rather than individual
/// providers.  Consumers should refresh their provider/model snapshot for every
/// variant, while diagnostics can still explain whether the trigger was a config
/// edit, credential change, or catalog refresh.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderCatalogInvalidationSource {
    /// `config.toml` or a config-derived environment override was reloaded.
    ConfigReload,
    /// Credentials or auth-derived provider availability changed.
    AuthChanged,
    /// A provider model catalog or route cache was refreshed.
    CatalogRefresh,
}

impl ProviderCatalogInvalidationSource {
    /// Stable diagnostic label for logs and future wire adapters.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ConfigReload => "config_reload",
            Self::AuthChanged => "auth_changed",
            Self::CatalogRefresh => "catalog_refresh",
        }
    }
}

/// A provider/model catalog invalidation with its resulting route-catalog
/// generation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderCatalogInvalidationEvent {
    /// Monotonic process-local generation after this invalidation.
    pub catalog_generation: u64,
    /// Typed producer boundary for the invalidation.
    pub source: ProviderCatalogInvalidationSource,
}

impl ProviderCatalogInvalidationEvent {
    /// Return whether this event is newer than a previously observed generation.
    pub const fn is_newer_than(&self, generation: u64) -> bool {
        self.catalog_generation > generation
    }
}

fn sender() -> &'static broadcast::Sender<ProviderCatalogInvalidationEvent> {
    static SENDER: OnceLock<broadcast::Sender<ProviderCatalogInvalidationEvent>> = OnceLock::new();
    SENDER.get_or_init(|| {
        let (sender, _) = broadcast::channel(EVENT_CHANNEL_CAPACITY);
        sender
    })
}

/// Subscribe to provider/model catalog invalidations.
pub fn subscribe() -> broadcast::Receiver<ProviderCatalogInvalidationEvent> {
    sender().subscribe()
}

/// Return the current process-local provider/model catalog generation.
pub fn current_generation() -> u64 {
    super::CATALOG_GENERATION.load(Ordering::Relaxed)
}

/// Publish a typed invalidation and advance the shared route-catalog generation.
///
/// The returned event lets synchronous producers correlate the generation they
/// just invalidated with later refresh work.  Consumers that subscribe after an
/// event was published can use [`current_generation`] to detect that they missed
/// a broadcast frame and rebuild once.
pub fn invalidate(source: ProviderCatalogInvalidationSource) -> ProviderCatalogInvalidationEvent {
    let catalog_generation = super::CATALOG_GENERATION
        .fetch_add(1, Ordering::Relaxed)
        .wrapping_add(1);
    let event = ProviderCatalogInvalidationEvent {
        catalog_generation,
        source,
    };
    let _ = sender().send(event.clone());
    event
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_labels_are_stable_and_typed() {
        assert_eq!(
            ProviderCatalogInvalidationSource::ConfigReload.as_str(),
            "config_reload"
        );
        assert_eq!(
            ProviderCatalogInvalidationSource::AuthChanged.as_str(),
            "auth_changed"
        );
        assert_eq!(
            ProviderCatalogInvalidationSource::CatalogRefresh.as_str(),
            "catalog_refresh"
        );
    }

    #[test]
    fn invalidation_advances_generation_and_publishes_event() {
        let mut receiver = subscribe();
        let before = current_generation();

        let event = invalidate(ProviderCatalogInvalidationSource::ConfigReload);

        assert!(event.is_newer_than(before));
        assert_eq!(
            event.source,
            ProviderCatalogInvalidationSource::ConfigReload
        );
        assert!(std::iter::from_fn(|| receiver.try_recv().ok()).any(|observed| observed == event));
    }

    #[test]
    fn scheduler_generation_bump_uses_catalog_refresh_source() {
        let mut receiver = subscribe();
        let before = current_generation();

        super::super::catalog_scheduler::bump_catalog_generation();

        assert!(
            std::iter::from_fn(|| receiver.try_recv().ok()).any(|event| {
                event.source == ProviderCatalogInvalidationSource::CatalogRefresh
                    && event.is_newer_than(before)
            })
        );
    }
}
