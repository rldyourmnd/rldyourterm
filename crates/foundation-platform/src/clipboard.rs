use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard};

use arboard::{Clipboard, Error as ArboardError};
use rldyourterm_foundation::api::clipboard::ClipboardAdapter;
use rldyourterm_foundation::clipboard::ClipboardHealth;
use rldyourterm_foundation::error::{
    ClipboardFailureCode, ClipboardOperation, FoundationError, FoundationResult, Recoverability,
};

type BackendFactory = Arc<dyn Fn() -> Result<Clipboard, ArboardError> + Send + Sync>;
const MAX_FALLBACK_CLIPBOARD_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BackendMode {
    Uninitialized,
    Ready,
    PermanentlyUnavailable,
}

struct ClipboardState {
    backend: Option<Clipboard>,
    fallback_text: Option<String>,
    health: ClipboardHealth,
    backend_mode: BackendMode,
}

pub struct PlatformClipboard {
    state: Mutex<ClipboardState>,
    backend_factory: BackendFactory,
}

impl fmt::Debug for PlatformClipboard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.state.lock() {
            Ok(state) => f
                .debug_struct("PlatformClipboard")
                .field("backend_initialized", &state.backend.is_some())
                .field("fallback_text_present", &state.fallback_text.is_some())
                .field("health", &state.health)
                .field("backend_mode", &state.backend_mode)
                .finish(),
            Err(_) => f
                .debug_struct("PlatformClipboard")
                .field("state", &"poisoned")
                .finish(),
        }
    }
}

impl Default for PlatformClipboard {
    fn default() -> Self {
        Self::new_with_backend_factory(Arc::new(Clipboard::new))
    }
}

impl PlatformClipboard {
    fn cap_fallback_text(text: &str) -> String {
        if text.len() <= MAX_FALLBACK_CLIPBOARD_BYTES {
            return text.to_owned();
        }

        let mut end = MAX_FALLBACK_CLIPBOARD_BYTES;
        while end > 0 && !text.is_char_boundary(end) {
            end -= 1;
        }
        text[..end].to_owned()
    }

    fn new_with_backend_factory(backend_factory: BackendFactory) -> Self {
        Self {
            state: Mutex::new(ClipboardState {
                backend: None,
                fallback_text: None,
                health: ClipboardHealth::Unavailable,
                backend_mode: BackendMode::Uninitialized,
            }),
            backend_factory,
        }
    }

    pub fn health(&self) -> ClipboardHealth {
        self.state
            .lock()
            .map(|state| state.health)
            .unwrap_or(ClipboardHealth::Unavailable)
    }

    fn lock_state(
        &self,
        operation: ClipboardOperation,
    ) -> FoundationResult<MutexGuard<'_, ClipboardState>> {
        self.state.lock().map_err(|_| {
            FoundationError::clipboard(
                operation,
                ClipboardFailureCode::BoundaryFault,
                Recoverability::Fatal,
                "clipboard state lock poisoned",
                None,
            )
        })
    }

    fn classify_arboard_error(error: &ArboardError) -> (ClipboardFailureCode, ClipboardHealth) {
        match error {
            ArboardError::ContentNotAvailable => {
                (ClipboardFailureCode::Unavailable, ClipboardHealth::Degraded)
            }
            ArboardError::ClipboardNotSupported => (
                ClipboardFailureCode::Unsupported,
                ClipboardHealth::Unavailable,
            ),
            ArboardError::ClipboardOccupied => (
                ClipboardFailureCode::AccessDenied,
                ClipboardHealth::Degraded,
            ),
            ArboardError::ConversionFailure => (
                ClipboardFailureCode::BoundaryFault,
                ClipboardHealth::Degraded,
            ),
            ArboardError::Unknown { .. } => (
                ClipboardFailureCode::BoundaryFault,
                ClipboardHealth::Degraded,
            ),
            _ => (
                ClipboardFailureCode::BoundaryFault,
                ClipboardHealth::Degraded,
            ),
        }
    }

    fn update_health(state: &mut ClipboardState, next: ClipboardHealth, reason: &'static str) {
        if state.health != next {
            tracing::info!(
                from = ?state.health,
                to = ?next,
                reason,
                "clipboard health transition"
            );
        }
        state.health = next;
    }

    fn apply_runtime_failure(
        state: &mut ClipboardState,
        operation: ClipboardOperation,
        code: ClipboardFailureCode,
        health: ClipboardHealth,
    ) {
        if matches!(code, ClipboardFailureCode::Unsupported) {
            state.backend = None;
            state.backend_mode = BackendMode::PermanentlyUnavailable;
            Self::update_health(
                state,
                ClipboardHealth::Unavailable,
                "backend became unsupported",
            );
            tracing::warn!(
                ?operation,
                "clipboard backend marked permanently unavailable"
            );
            return;
        }

        Self::update_health(state, health, "runtime clipboard operation failed");
    }

    fn ensure_backend(
        &self,
        state: &mut ClipboardState,
        operation: ClipboardOperation,
    ) -> FoundationResult<()> {
        if state.backend.is_some() {
            state.backend_mode = BackendMode::Ready;
            Self::update_health(state, ClipboardHealth::Available, "backend available");
            return Ok(());
        }

        if matches!(state.backend_mode, BackendMode::PermanentlyUnavailable) {
            Self::update_health(state, ClipboardHealth::Unavailable, "backend unavailable");
            return Err(FoundationError::clipboard(
                operation,
                ClipboardFailureCode::Unavailable,
                Recoverability::Degrade,
                "clipboard backend unavailable; using deterministic fallback",
                None,
            ));
        }

        match (self.backend_factory)() {
            Ok(backend) => {
                state.backend = Some(backend);
                state.backend_mode = BackendMode::Ready;
                Self::update_health(state, ClipboardHealth::Available, "backend initialized");
                Ok(())
            }
            Err(error) => {
                let (code, health) = Self::classify_arboard_error(&error);
                if matches!(code, ClipboardFailureCode::Unsupported) {
                    state.backend = None;
                    state.backend_mode = BackendMode::PermanentlyUnavailable;
                    Self::update_health(
                        state,
                        ClipboardHealth::Unavailable,
                        "backend init unsupported",
                    );
                } else {
                    state.backend_mode = BackendMode::Uninitialized;
                    Self::update_health(state, health, "backend init failed");
                }
                Err(FoundationError::clipboard(
                    operation,
                    code,
                    Recoverability::Degrade,
                    format!("clipboard init failed: {error}"),
                    None,
                ))
            }
        }
    }
}

impl ClipboardAdapter for PlatformClipboard {
    fn set_text(&self, text: &str) -> FoundationResult<()> {
        let mut state = self.lock_state(ClipboardOperation::SetText)?;
        state.fallback_text = Some(Self::cap_fallback_text(text));

        if let Err(error) = self.ensure_backend(&mut state, ClipboardOperation::SetText) {
            tracing::warn!(
                error = %error,
                "clipboard backend unavailable; keeping fallback clipboard text"
            );
            return Ok(());
        }

        if let Some(backend) = state.backend.as_mut() {
            if let Err(error) = backend.set_text(text.to_owned()) {
                let (code, health) = Self::classify_arboard_error(&error);
                Self::apply_runtime_failure(&mut state, ClipboardOperation::SetText, code, health);
                tracing::warn!(
                    ?code,
                    "clipboard set_text failed; fallback clipboard text retained"
                );
            } else {
                Self::update_health(&mut state, ClipboardHealth::Available, "set_text succeeded");
            }
        }

        Ok(())
    }

    fn get_text(&self) -> FoundationResult<Option<String>> {
        let mut state = self.lock_state(ClipboardOperation::GetText)?;

        if let Err(error) = self.ensure_backend(&mut state, ClipboardOperation::GetText) {
            tracing::warn!(
                error = %error,
                "clipboard backend unavailable; returning fallback text"
            );
            return Ok(state.fallback_text.clone());
        }

        if let Some(backend) = state.backend.as_mut() {
            match backend.get_text() {
                Ok(text) => {
                    state.fallback_text = Some(Self::cap_fallback_text(&text));
                    Self::update_health(
                        &mut state,
                        ClipboardHealth::Available,
                        "get_text succeeded",
                    );
                    Ok(Some(text))
                }
                Err(error) => {
                    let (code, health) = Self::classify_arboard_error(&error);
                    Self::apply_runtime_failure(
                        &mut state,
                        ClipboardOperation::GetText,
                        code,
                        health,
                    );
                    tracing::warn!(?code, "clipboard get_text failed; returning fallback text");
                    Ok(state.fallback_text.clone())
                }
            }
        } else {
            Ok(state.fallback_text.clone())
        }
    }

    fn clear(&self) -> FoundationResult<()> {
        let mut state = self.lock_state(ClipboardOperation::Clear)?;
        state.fallback_text = None;

        if let Err(error) = self.ensure_backend(&mut state, ClipboardOperation::Clear) {
            tracing::warn!(
                error = %error,
                "clipboard backend unavailable during clear; fallback state already cleared"
            );
            return Ok(());
        }

        if let Some(backend) = state.backend.as_mut() {
            if let Err(error) = backend.clear() {
                let (code, health) = Self::classify_arboard_error(&error);
                Self::apply_runtime_failure(&mut state, ClipboardOperation::Clear, code, health);
                tracing::warn!(?code, "clipboard clear failed");
            } else {
                Self::update_health(&mut state, ClipboardHealth::Available, "clear succeeded");
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    fn clipboard_with_factory<F>(backend_factory: F) -> PlatformClipboard
    where
        F: Fn() -> Result<Clipboard, ArboardError> + Send + Sync + 'static,
    {
        PlatformClipboard::new_with_backend_factory(Arc::new(backend_factory))
    }

    #[test]
    fn deterministic_fallback_when_backend_is_unavailable() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_for_factory = Arc::clone(&attempts);
        let clipboard = clipboard_with_factory(move || {
            attempts_for_factory.fetch_add(1, Ordering::SeqCst);
            Err(ArboardError::ClipboardNotSupported)
        });

        clipboard
            .set_text("fallback-value")
            .expect("set_text should remain non-fatal");
        assert_eq!(clipboard.health(), ClipboardHealth::Unavailable);
        assert_eq!(
            clipboard
                .get_text()
                .expect("get_text should return fallback"),
            Some("fallback-value".to_owned())
        );

        clipboard
            .clear()
            .expect("clear should remain non-fatal in fallback mode");
        assert_eq!(
            clipboard
                .get_text()
                .expect("get_text after clear should succeed"),
            None
        );

        assert_eq!(
            attempts.load(Ordering::SeqCst),
            1,
            "backend init must not be retried after becoming unavailable"
        );
    }

    #[test]
    fn transient_backend_init_failure_keeps_fallback_text() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_for_factory = Arc::clone(&attempts);
        let clipboard = clipboard_with_factory(move || {
            attempts_for_factory.fetch_add(1, Ordering::SeqCst);
            Err(ArboardError::ClipboardOccupied)
        });

        clipboard
            .set_text("value")
            .expect("set_text should succeed with fallback");
        assert_eq!(clipboard.health(), ClipboardHealth::Degraded);
        assert_eq!(
            clipboard
                .get_text()
                .expect("get_text should return fallback"),
            Some("value".to_owned())
        );
        assert_eq!(
            attempts.load(Ordering::SeqCst),
            2,
            "temporary failures should keep retrying backend init"
        );
    }

    #[test]
    fn fallback_text_is_capped_to_prevent_unbounded_growth() {
        let clipboard = clipboard_with_factory(|| Err(ArboardError::ClipboardNotSupported));
        let payload = "x".repeat(MAX_FALLBACK_CLIPBOARD_BYTES + 4096);
        clipboard
            .set_text(&payload)
            .expect("set_text should keep capped fallback");

        let stored = clipboard
            .get_text()
            .expect("get_text should return fallback")
            .expect("fallback text should exist");
        assert_eq!(stored.len(), MAX_FALLBACK_CLIPBOARD_BYTES);
    }

    #[test]
    fn fallback_text_cap_preserves_utf8_boundaries() {
        let clipboard = clipboard_with_factory(|| Err(ArboardError::ClipboardNotSupported));
        let payload = format!("{}🚀", "a".repeat(MAX_FALLBACK_CLIPBOARD_BYTES - 1));
        clipboard
            .set_text(&payload)
            .expect("set_text should keep fallback");

        let stored = clipboard
            .get_text()
            .expect("get_text should return fallback")
            .expect("fallback text should exist");
        assert_eq!(stored.len(), MAX_FALLBACK_CLIPBOARD_BYTES - 1);
        assert_eq!(stored.chars().last(), Some('a'));
    }
}
