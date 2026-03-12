// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Danil Silantyev (rldyourmnd), NDDev OpenNetwork

use rldyourterm_foundation::api::common::ContractResult;
use rldyourterm_foundation::api::diagnostics::{
    DiagnosticEvent as FoundationDiagnosticEvent, DiagnosticSink as FoundationDiagnosticSink,
};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::{CorrelationId, Event, EventKind, now_timestamp_ms};

#[derive(Debug)]
pub struct DiagnosticsSink {
    next_id: AtomicU64,
}

impl Default for DiagnosticsSink {
    fn default() -> Self {
        Self {
            next_id: AtomicU64::new(1),
        }
    }
}

impl DiagnosticsSink {
    fn emit_foundation_event(&self, canonical_event: &FoundationDiagnosticEvent) {
        tracing::info!(
            event_id = %canonical_event.event_id.0,
            kind = ?canonical_event.kind,
            severity = ?canonical_event.severity,
            layer = ?canonical_event.layer,
            correlation_id = ?canonical_event.correlation_id,
            "diagnostics event emitted"
        );
    }

    pub fn emit(&self, mut event: Event) -> Event {
        if event.event_id.is_empty() {
            event.event_id = self.next_event_id();
        }
        if event.timestamp_ms == 0 {
            event.timestamp_ms = now_timestamp_ms();
        }

        let canonical_event = event.to_foundation_event();
        self.emit_foundation_event(&canonical_event);

        event
    }

    pub fn emit_kind(&self, kind: EventKind, message: impl Into<String>) -> Event {
        self.emit(Event::new(kind, message))
    }

    pub fn with_correlation<'a>(
        &'a self,
        correlation_id: CorrelationId,
    ) -> CorrelatedDiagnosticsSink<'a> {
        CorrelatedDiagnosticsSink {
            sink: self,
            correlation_id,
        }
    }

    fn next_event_id(&self) -> String {
        let sequence = self.next_id.fetch_add(1, Ordering::Relaxed);
        format!("diag-{sequence}")
    }
}

impl FoundationDiagnosticSink for DiagnosticsSink {
    fn emit(&self, event: FoundationDiagnosticEvent) -> ContractResult<()> {
        self.emit_foundation_event(&event);
        Ok(())
    }

    fn flush(&self) -> ContractResult<()> {
        Ok(())
    }
}

#[derive(Debug)]
pub struct CorrelatedDiagnosticsSink<'a> {
    sink: &'a DiagnosticsSink,
    correlation_id: CorrelationId,
}

impl<'a> CorrelatedDiagnosticsSink<'a> {
    pub fn emit(&self, event: Event) -> Event {
        if event.correlation_id.is_some() {
            return self.sink.emit(event);
        }

        self.sink
            .emit(event.with_correlation(self.correlation_id.clone()))
    }

    pub fn emit_kind(&self, kind: EventKind, message: impl Into<String>) -> Event {
        self.emit(Event::new(kind, message))
    }
}
