use super::{
    BackendSyncAction, CHILD_EXIT_DRAIN_MAX_WAIT, CLIPBOARD_PASTE_CAP_BYTES, DEFAULT_FG,
    DEFAULT_FG_U32, DeferredGpuInitState, GpuFailureHandling, MAX_FEED_BYTES_PER_CALL,
    MAX_FRAMEBUFFER_HEIGHT, MAX_FRAMEBUFFER_PIXELS, MAX_FRAMEBUFFER_WIDTH, MAX_VIEWPORT_CELLS,
    MAX_VIEWPORT_COLS, MAX_VIEWPORT_ROWS, MonitorAffectingWindowEvent, OUTPUT_BATCH_MAX_BYTES,
    OUTPUT_DRAIN_CRITICAL_MAX_BYTES_PER_TICK, OUTPUT_DRAIN_ELEVATED_MAX_BYTES_PER_TICK,
    OUTPUT_DRAIN_MAX_BYTES_PER_TICK, OUTPUT_DRAIN_MAX_LATENCY, OutputDrainBudget,
    OutputDrainPressure, OutputQueueSnapshot, PTY_OUTPUT_CHUNK_BYTES,
    PTY_OUTPUT_RECYCLE_POOL_WARMUP, RenderBackendCoordinator, RenderWaitPolicy, ViewportGeometry,
    cadence_resync_command_for_monitor_event, cap_framebuffer_extent, cap_paste_text,
    cap_terminal_geometry, child_exit_drain_timed_out, deferred_gpu_init_backoff,
    dispatch_gpu_failure_command, dispatch_runtime_palette_command,
    emit_gpu_auto_fallback_observability, encode_winit_key_event, is_runtime_palette_shortcut_key,
    output_drain_budget, output_drain_budget_exhausted, read_clipboard_text_for_paste,
    recycle_output_chunk_buffer, render_wait_policy, resolve_cell_colors,
    sample_monitor_refresh_rate_millihz, should_flush_output_batch, take_output_chunk_buffer,
    terminal_feed_chunks, viewport_geometry_changed, warm_output_chunk_pool,
};
use crate::shared::{PtyBoundaryPolicyDecision, classify_pty_boundary_failure};
use rldyourterm_diagnostics::{DiagnosticsSink, EventKind};
use rldyourterm_foundation::api::{
    clipboard::ClipboardAdapter,
    common::{ContractResult, MonitorTiming},
    window::WindowControl,
};
use rldyourterm_foundation::error::{
    ClipboardFailureCode, ClipboardOperation, FoundationError, Recoverability, WindowFailureCode,
    WindowOperation,
};
use rldyourterm_services::render_mode::{ActiveRenderPath, GpuFailureKind, RenderMode};
use rldyourterm_services::session::{FatalBoundaryReason, SessionBoundary, SessionController};
use rldyourterm_services::terminal::{ANSI_PALETTE, Attrs, Color, color_to_u32};
use rldyourterm_settings::SettingsService;
use rldyourterm_ui::{UiBootstrapConfig, UiRuntime, UiRuntimeCommand};
use std::sync::mpsc::sync_channel;
use std::time::{Duration, Instant};
use winit::dpi::PhysicalSize;
use winit::keyboard::{Key, ModifiersState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StubClipboardScenario {
    Text(&'static str),
    Empty,
    Error,
}

struct StubClipboard {
    scenario: StubClipboardScenario,
}

impl ClipboardAdapter for StubClipboard {
    fn set_text(&self, _text: &str) -> ContractResult<()> {
        Ok(())
    }

    fn get_text(&self) -> ContractResult<Option<String>> {
        match self.scenario {
            StubClipboardScenario::Text(text) => Ok(Some(text.to_owned())),
            StubClipboardScenario::Empty => Ok(None),
            StubClipboardScenario::Error => Err(FoundationError::clipboard(
                ClipboardOperation::GetText,
                ClipboardFailureCode::BoundaryFault,
                Recoverability::Degrade,
                "test clipboard failure",
                None,
            )),
        }
    }

    fn clear(&self) -> ContractResult<()> {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StubWindowControlScenario {
    Timing(Option<u32>),
    Error,
}

struct StubWindowControl {
    scenario: StubWindowControlScenario,
}

impl WindowControl for StubWindowControl {
    fn request_redraw(&self) -> ContractResult<()> {
        Ok(())
    }

    fn set_title(&self, _title: &str) -> ContractResult<()> {
        Ok(())
    }

    fn current_monitor_timing(&self) -> ContractResult<MonitorTiming> {
        match self.scenario {
            StubWindowControlScenario::Timing(refresh_rate_millihz) => Ok(MonitorTiming {
                monitor_name: Some("stub-monitor".to_owned()),
                refresh_rate_millihz,
            }),
            StubWindowControlScenario::Error => Err(FoundationError::window(
                WindowOperation::QueryMonitorTiming,
                WindowFailureCode::BoundaryFault,
                Recoverability::Degrade,
                "monitor timing unavailable",
                None,
            )),
        }
    }

    fn close(&self) -> ContractResult<()> {
        Ok(())
    }
}

#[test]
fn clipboard_dispatch_returns_non_empty_text() {
    let clipboard = StubClipboard {
        scenario: StubClipboardScenario::Text("wave3"),
    };
    assert_eq!(
        read_clipboard_text_for_paste(&clipboard),
        Some("wave3".to_owned())
    );
}

#[test]
fn clipboard_dispatch_ignores_empty_text() {
    let clipboard = StubClipboard {
        scenario: StubClipboardScenario::Empty,
    };
    assert_eq!(read_clipboard_text_for_paste(&clipboard), None);
}

#[test]
fn clipboard_dispatch_ignores_adapter_errors() {
    let clipboard = StubClipboard {
        scenario: StubClipboardScenario::Error,
    };
    assert_eq!(read_clipboard_text_for_paste(&clipboard), None);
}

#[test]
fn clipboard_paste_cap_limits_payload_to_64kb() {
    let payload = "x".repeat(70 * 1024);
    assert_eq!(cap_paste_text(&payload).len(), CLIPBOARD_PASTE_CAP_BYTES);
}

#[test]
fn clipboard_paste_cap_preserves_utf8_boundary() {
    let payload = format!("{}🚀", "a".repeat(CLIPBOARD_PASTE_CAP_BYTES - 1));
    let capped = cap_paste_text(&payload);
    assert_eq!(capped.len(), CLIPBOARD_PASTE_CAP_BYTES - 1);
    assert_eq!(capped.chars().last(), Some('a'));
}

#[test]
fn viewport_geometry_cap_preserves_small_dimensions() {
    assert_eq!(cap_terminal_geometry(120, 32), (120, 32));
}

#[test]
fn viewport_geometry_change_detects_pixel_only_updates() {
    assert!(viewport_geometry_changed(
        ViewportGeometry {
            cols: 120,
            rows: 32,
            pixel_width: 1280,
            pixel_height: 800,
        },
        ViewportGeometry {
            cols: 120,
            rows: 32,
            pixel_width: 1400,
            pixel_height: 800,
        }
    ));
    assert!(!viewport_geometry_changed(
        ViewportGeometry {
            cols: 120,
            rows: 32,
            pixel_width: 1280,
            pixel_height: 800,
        },
        ViewportGeometry {
            cols: 120,
            rows: 32,
            pixel_width: 1280,
            pixel_height: 800,
        }
    ));
}

#[test]
fn viewport_geometry_cap_enforces_axis_and_cell_limits() {
    let (cols, rows) = cap_terminal_geometry(20_000, 20_000);
    assert!(cols as usize <= MAX_VIEWPORT_COLS);
    assert!(rows as usize <= MAX_VIEWPORT_ROWS);
    assert!((cols as usize) * (rows as usize) <= MAX_VIEWPORT_CELLS);
}

#[test]
fn framebuffer_extent_cap_preserves_small_dimensions() {
    let capped = cap_framebuffer_extent(PhysicalSize::new(1920, 1080));
    assert_eq!(capped, PhysicalSize::new(1920, 1080));
}

#[test]
fn framebuffer_extent_cap_enforces_axis_and_pixel_limits() {
    let capped = cap_framebuffer_extent(PhysicalSize::new(100_000, 100_000));
    assert!(capped.width <= MAX_FRAMEBUFFER_WIDTH);
    assert!(capped.height <= MAX_FRAMEBUFFER_HEIGHT);
    assert!(u64::from(capped.width) * u64::from(capped.height) <= MAX_FRAMEBUFFER_PIXELS);
}

#[test]
fn deferred_gpu_init_backoff_is_bounded_and_monotonic() {
    let first = deferred_gpu_init_backoff(1);
    let second = deferred_gpu_init_backoff(2);
    let third = deferred_gpu_init_backoff(3);
    let fourth = deferred_gpu_init_backoff(4);
    let saturated = deferred_gpu_init_backoff(8);

    assert!(first <= second);
    assert!(second <= third);
    assert!(third <= fourth);
    assert_eq!(fourth, saturated);
}

#[test]
fn deferred_gpu_init_state_transitions_are_consistent() {
    let mut state = DeferredGpuInitState::new(true);
    assert!(state.is_pending());
    assert_eq!(state.next_attempt(), 1);
    assert_eq!(state.begin_attempt(), 1);
    assert_eq!(state.retry_deadline(), None);

    state.schedule_retry(1, Duration::from_millis(10));
    assert!(state.is_pending());
    assert_eq!(state.next_attempt(), 2);
    assert!(state.retry_deadline().is_some());

    state.record_failure_attempt(2);
    assert_eq!(state.next_attempt(), 3);

    state.mark_exhausted(2);
    assert!(!state.is_pending());
    assert_eq!(state.retry_deadline(), None);
    assert_eq!(state.next_attempt(), 3);

    state.sync_with_target_path(ActiveRenderPath::Gpu, false);
    assert!(state.is_pending());
    assert_eq!(state.next_attempt(), 3);

    state.mark_ready();
    assert!(!state.is_pending());
    assert_eq!(state.next_attempt(), 1);

    state.sync_with_target_path(ActiveRenderPath::Cpu, false);
    assert!(!state.is_pending());
    assert_eq!(state.next_attempt(), 1);
    assert_eq!(state.retry_deadline(), None);
}

#[test]
fn render_backend_coordinator_tracks_sequences_and_sync_policy() {
    let mut coordinator = RenderBackendCoordinator::new(RenderMode::Auto);

    assert!(coordinator.deferred_gpu_init_pending());
    assert_eq!(coordinator.begin_render_attempt(), 1);
    assert_eq!(coordinator.begin_render_attempt(), 2);
    assert_eq!(coordinator.current_render_attempt_sequence(), 2);
    assert_eq!(coordinator.next_gpu_failure_sequence(), 1);
    assert_eq!(coordinator.next_gpu_failure_sequence(), 2);

    assert_eq!(
        coordinator.sync_with_target_path(ActiveRenderPath::Cpu, true),
        BackendSyncAction::ReleaseGpuBackend
    );
    assert!(!coordinator.deferred_gpu_init_pending());

    coordinator.mark_deferred_ready();
    assert_eq!(
        coordinator.wait_policy(
            ActiveRenderPath::Gpu,
            true,
            true,
            Some(Duration::from_millis(16))
        ),
        RenderWaitPolicy::EventDriven
    );
}

#[test]
fn render_wait_policy_uses_event_driven_gpu_lane() {
    assert_eq!(
        render_wait_policy(true, true, Some(Duration::from_millis(8))),
        RenderWaitPolicy::EventDriven
    );
}

#[test]
fn render_wait_policy_uses_cpu_cadence_when_dirty() {
    assert_eq!(
        render_wait_policy(false, true, Some(Duration::from_millis(16))),
        RenderWaitPolicy::CadenceTimed(Duration::from_millis(16))
    );
}

#[test]
fn render_wait_policy_falls_back_to_event_driven_without_cadence() {
    assert_eq!(
        render_wait_policy(false, true, None),
        RenderWaitPolicy::EventDriven
    );
    assert_eq!(
        render_wait_policy(false, false, Some(Duration::from_millis(16))),
        RenderWaitPolicy::EventDriven
    );
}

#[test]
fn output_batch_flush_policy_only_triggers_on_overflow_with_existing_batch() {
    assert!(!should_flush_output_batch(0, 32));
    assert!(!should_flush_output_batch(128, 256));
    assert!(should_flush_output_batch(OUTPUT_BATCH_MAX_BYTES - 64, 128));
}

#[test]
fn output_chunk_pool_warmup_stops_at_channel_capacity() {
    let (recycle_tx, recycle_rx) = sync_channel::<Vec<u8>>(2);
    warm_output_chunk_pool(&recycle_tx);
    let chunk_count = recycle_rx.try_iter().count();
    assert_eq!(chunk_count, PTY_OUTPUT_RECYCLE_POOL_WARMUP.min(2));
}

#[test]
fn output_chunk_take_reuses_preallocated_buffer_when_available() {
    let (recycle_tx, recycle_rx) = sync_channel::<Vec<u8>>(1);
    let mut seeded = vec![0_u8; PTY_OUTPUT_CHUNK_BYTES];
    let seeded_ptr = seeded.as_ptr();
    seeded.truncate(32);
    recycle_tx.send(seeded).expect("seed recycle buffer");

    let reused = take_output_chunk_buffer(&recycle_rx);
    assert_eq!(reused.len(), PTY_OUTPUT_CHUNK_BYTES);
    assert_eq!(reused.as_ptr(), seeded_ptr);
}

#[test]
fn output_chunk_recycle_roundtrip_preserves_allocation() {
    let (recycle_tx, recycle_rx) = sync_channel::<Vec<u8>>(1);
    let mut chunk = vec![0_u8; PTY_OUTPUT_CHUNK_BYTES];
    let ptr = chunk.as_ptr();
    chunk.truncate(777);
    recycle_output_chunk_buffer(&recycle_tx, chunk);

    let roundtrip = take_output_chunk_buffer(&recycle_rx);
    assert_eq!(roundtrip.len(), PTY_OUTPUT_CHUNK_BYTES);
    assert_eq!(roundtrip.as_ptr(), ptr);
}

#[test]
fn output_drain_budget_triggers_on_byte_limit() {
    let budget = OutputDrainBudget {
        pressure: OutputDrainPressure::Normal,
        max_bytes_per_tick: OUTPUT_DRAIN_MAX_BYTES_PER_TICK,
        max_latency: OUTPUT_DRAIN_MAX_LATENCY,
    };
    assert!(!output_drain_budget_exhausted(
        OUTPUT_DRAIN_MAX_BYTES_PER_TICK - 1,
        Duration::ZERO,
        budget,
    ));
    assert!(output_drain_budget_exhausted(
        OUTPUT_DRAIN_MAX_BYTES_PER_TICK,
        Duration::ZERO,
        budget,
    ));
}

#[test]
fn output_drain_budget_triggers_on_elapsed_limit() {
    let budget = OutputDrainBudget {
        pressure: OutputDrainPressure::Normal,
        max_bytes_per_tick: OUTPUT_DRAIN_MAX_BYTES_PER_TICK,
        max_latency: OUTPUT_DRAIN_MAX_LATENCY,
    };
    assert!(!output_drain_budget_exhausted(
        0,
        OUTPUT_DRAIN_MAX_LATENCY.saturating_sub(Duration::from_millis(1)),
        budget,
    ));
    assert!(output_drain_budget_exhausted(
        0,
        OUTPUT_DRAIN_MAX_LATENCY,
        budget
    ));
}

#[test]
fn output_drain_budget_escalates_with_queue_pressure() {
    let normal = output_drain_budget(OutputQueueSnapshot {
        queued_bytes: 0,
        queued_chunks: 0,
    });
    assert_eq!(normal.pressure, OutputDrainPressure::Normal);
    assert_eq!(normal.max_bytes_per_tick, OUTPUT_DRAIN_MAX_BYTES_PER_TICK);

    let elevated = output_drain_budget(OutputQueueSnapshot {
        queued_bytes: 3 * 1024 * 1024,
        queued_chunks: 4,
    });
    assert_eq!(elevated.pressure, OutputDrainPressure::Elevated);
    assert_eq!(
        elevated.max_bytes_per_tick,
        OUTPUT_DRAIN_ELEVATED_MAX_BYTES_PER_TICK
    );

    let critical = output_drain_budget(OutputQueueSnapshot {
        queued_bytes: 10 * 1024 * 1024,
        queued_chunks: 220,
    });
    assert_eq!(critical.pressure, OutputDrainPressure::Critical);
    assert_eq!(
        critical.max_bytes_per_tick,
        OUTPUT_DRAIN_CRITICAL_MAX_BYTES_PER_TICK
    );
}

#[test]
fn child_exit_drain_timeout_boundary_is_deterministic() {
    let started_at = Instant::now();
    assert!(!child_exit_drain_timed_out(
        started_at,
        started_at + CHILD_EXIT_DRAIN_MAX_WAIT.saturating_sub(Duration::from_millis(1))
    ));
    assert!(child_exit_drain_timed_out(
        started_at,
        started_at + CHILD_EXIT_DRAIN_MAX_WAIT
    ));
}

#[test]
fn terminal_feed_chunking_respects_core_per_call_limit() {
    let payload = vec![b'x'; MAX_FEED_BYTES_PER_CALL * 2 + 17];
    let chunk_sizes: Vec<usize> = terminal_feed_chunks(&payload)
        .map(|chunk| chunk.len())
        .collect();
    assert_eq!(
        chunk_sizes,
        vec![MAX_FEED_BYTES_PER_CALL, MAX_FEED_BYTES_PER_CALL, 17]
    );
}

#[test]
fn detects_palette_shortcut_with_ctrl_or_cmd_shift_p() {
    let key = Key::Character("p".into());
    assert!(is_runtime_palette_shortcut_key(
        key.as_ref(),
        ModifiersState::CONTROL | ModifiersState::SHIFT
    ));
    assert!(is_runtime_palette_shortcut_key(
        key.as_ref(),
        ModifiersState::SUPER | ModifiersState::SHIFT
    ));
    assert!(!is_runtime_palette_shortcut_key(
        key.as_ref(),
        ModifiersState::CONTROL
    ));
    assert!(!is_runtime_palette_shortcut_key(
        key.as_ref(),
        ModifiersState::SHIFT
    ));
}

#[test]
fn palette_dispatch_updates_render_mode_via_runtime_path() {
    let mut ui_runtime = test_ui_runtime(RenderMode::Auto);
    let mut settings = SettingsService::default();

    let message = dispatch_runtime_palette_command(&mut ui_runtime, &mut settings, "mode cpu")
        .expect("dispatch mode cpu");
    assert!(message.contains("mode=cpu"));
    assert_eq!(settings.state().mode, RenderMode::Cpu);
    assert_eq!(ui_runtime.render_mode(), RenderMode::Cpu);
}

#[test]
fn palette_dispatch_toggles_diagnostics_state() {
    let mut ui_runtime = test_ui_runtime(RenderMode::Auto);
    let mut settings = SettingsService::default();

    let on_message = dispatch_runtime_palette_command(&mut ui_runtime, &mut settings, "debug on")
        .expect("dispatch debug on");
    assert!(on_message.contains("diagnostics=on"));
    assert!(settings.state().debug_mode);

    let off_message = dispatch_runtime_palette_command(&mut ui_runtime, &mut settings, "debug off")
        .expect("dispatch debug off");
    assert!(off_message.contains("diagnostics=off"));
    assert!(!settings.state().debug_mode);
}

fn test_ui_runtime(mode: RenderMode) -> UiRuntime {
    UiRuntime::bootstrap(UiBootstrapConfig::single_window(mode, 60_000))
        .expect("ui runtime bootstrap")
}

#[test]
fn injected_gpu_failure_falls_back_without_forcing_exit_in_auto_mode() {
    let mut ui_runtime = test_ui_runtime(RenderMode::Auto);
    assert_eq!(ui_runtime.active_render_path(), ActiveRenderPath::Gpu);

    let first = dispatch_gpu_failure_command(&mut ui_runtime, GpuFailureKind::SurfaceError, 10)
        .expect("first gpu failure");
    assert_eq!(
        first,
        GpuFailureHandling::RetryScheduled {
            failure_streak: 1,
            retry_budget_remaining: 1
        }
    );
    assert_eq!(ui_runtime.active_render_path(), ActiveRenderPath::Gpu);

    let second = dispatch_gpu_failure_command(&mut ui_runtime, GpuFailureKind::SubmitError, 20)
        .expect("second gpu failure");
    assert_eq!(
        second,
        GpuFailureHandling::RetryScheduled {
            failure_streak: 2,
            retry_budget_remaining: 0
        }
    );
    assert_eq!(ui_runtime.active_render_path(), ActiveRenderPath::Gpu);

    let third =
        dispatch_gpu_failure_command(&mut ui_runtime, GpuFailureKind::SwapchainOutOfDate, 30)
            .expect("third gpu failure");
    assert_eq!(
        third,
        GpuFailureHandling::FallbackToCpu {
            transition_sequence: 1
        }
    );
    assert_eq!(ui_runtime.active_render_path(), ActiveRenderPath::Cpu);
}

#[test]
fn forced_gpu_mode_reports_explicit_gpu_failure() {
    let mut ui_runtime = test_ui_runtime(RenderMode::Gpu);
    assert_eq!(ui_runtime.active_render_path(), ActiveRenderPath::Gpu);

    let decision = dispatch_gpu_failure_command(&mut ui_runtime, GpuFailureKind::SurfaceError, 7)
        .expect("gpu failure decision");
    assert_eq!(decision, GpuFailureHandling::FatalForcedGpu);
    assert_eq!(ui_runtime.active_render_path(), ActiveRenderPath::Gpu);
}

#[test]
fn monitor_timing_sampling_uses_window_control_contract() {
    let control = StubWindowControl {
        scenario: StubWindowControlScenario::Timing(Some(144_000)),
    };
    assert_eq!(
        sample_monitor_refresh_rate_millihz(Some(&control)),
        Some(144_000)
    );
}

#[test]
fn monitor_timing_sampling_returns_none_when_contract_fails() {
    let control = StubWindowControl {
        scenario: StubWindowControlScenario::Error,
    };
    assert_eq!(sample_monitor_refresh_rate_millihz(Some(&control)), None);
}

#[test]
fn monitor_affecting_events_emit_expected_cadence_resync_commands() {
    let sampled_refresh = Some(144_000);

    assert_eq!(
        cadence_resync_command_for_monitor_event(
            MonitorAffectingWindowEvent::Moved,
            sampled_refresh,
        ),
        UiRuntimeCommand::ResyncCadenceAfterTransfer {
            refresh_rate_millihz: 144_000,
        }
    );
    assert_eq!(
        cadence_resync_command_for_monitor_event(
            MonitorAffectingWindowEvent::Resized,
            sampled_refresh,
        ),
        UiRuntimeCommand::ResyncCadence {
            refresh_rate_millihz: 144_000,
        }
    );
    assert_eq!(
        cadence_resync_command_for_monitor_event(
            MonitorAffectingWindowEvent::ScaleFactorChanged,
            sampled_refresh,
        ),
        UiRuntimeCommand::ResyncCadenceAfterTransfer {
            refresh_rate_millihz: 144_000,
        }
    );
}

#[test]
fn cadence_resync_commands_use_zero_when_monitor_timing_is_unavailable() {
    assert_eq!(
        cadence_resync_command_for_monitor_event(MonitorAffectingWindowEvent::Moved, None),
        UiRuntimeCommand::ResyncCadenceAfterTransfer {
            refresh_rate_millihz: 0,
        }
    );
    assert_eq!(
        cadence_resync_command_for_monitor_event(MonitorAffectingWindowEvent::Resized, None),
        UiRuntimeCommand::ResyncCadence {
            refresh_rate_millihz: 0,
        }
    );
    assert_eq!(
        cadence_resync_command_for_monitor_event(
            MonitorAffectingWindowEvent::ScaleFactorChanged,
            None,
        ),
        UiRuntimeCommand::ResyncCadenceAfterTransfer {
            refresh_rate_millihz: 0,
        }
    );
}

#[test]
fn gpu_auto_fallback_emits_correlated_diagnostics_and_runtime_notice() {
    let diagnostics = DiagnosticsSink::default();
    let (event, notice) = emit_gpu_auto_fallback_observability(
        &diagnostics,
        7,
        3,
        41,
        GpuFailureKind::SwapchainOutOfDate,
        2_500,
    );

    assert_eq!(event.kind, EventKind::RenderModeTransition);
    let correlation = event
        .correlation_id
        .as_ref()
        .expect("fallback diagnostics must include correlation");
    assert!(event.message.contains("transition-seq=7"));
    assert!(event.message.contains("failure-seq=3"));
    assert!(event.message.contains("render-attempt-seq=41"));
    assert!(event.message.contains("observed-ms=2500"));
    assert!(notice.contains("transition-seq=7"));
    assert!(notice.contains("failure-seq=3"));
    assert!(notice.contains("render-attempt-seq=41"));
    assert!(notice.contains("observed-ms=2500"));
    assert!(notice.contains(correlation.as_str()));
}

#[test]
fn gui_write_boundary_policy_stays_recoverable_with_budget() {
    let mut session_policy = SessionController::with_recoverable_budget(2);
    session_policy
        .mark_running()
        .expect("session should enter running state");

    let decision = classify_pty_boundary_failure(&mut session_policy, SessionBoundary::PtyWrite)
        .expect("recoverable write boundary should classify");

    assert_eq!(
        decision,
        PtyBoundaryPolicyDecision::Continue {
            attempt: 1,
            remaining_budget: 1,
        }
    );
}

#[test]
fn gui_write_boundary_policy_escalates_after_budget_exhaustion() {
    let mut session_policy = SessionController::with_recoverable_budget(1);
    session_policy
        .mark_running()
        .expect("session should enter running state");

    let first = classify_pty_boundary_failure(&mut session_policy, SessionBoundary::PtyWrite)
        .expect("first write boundary should stay recoverable");
    assert_eq!(
        first,
        PtyBoundaryPolicyDecision::Continue {
            attempt: 1,
            remaining_budget: 0,
        }
    );

    let second = classify_pty_boundary_failure(&mut session_policy, SessionBoundary::PtyWrite)
        .expect("second write boundary should escalate after budget exhaustion");
    assert_eq!(
        second,
        PtyBoundaryPolicyDecision::Fatal {
            reason: FatalBoundaryReason::RecoverableBudgetExhausted,
        }
    );
}

#[test]
fn color_to_u32_default_uses_default_color() {
    let default_fg = color_to_u32(Color::Default, DEFAULT_FG);
    assert_eq!(default_fg, DEFAULT_FG_U32);
}

#[test]
fn color_to_u32_indexed_looks_up_palette() {
    let red = color_to_u32(Color::Indexed(1), DEFAULT_FG);
    assert_eq!(red, ANSI_PALETTE[1]);
}

#[test]
fn color_to_u32_rgb_constructs_correctly() {
    let c = color_to_u32(Color::Rgb(0xFF, 0x80, 0x00), DEFAULT_FG);
    assert_eq!(c, 0x00FF_8000);
}

#[test]
fn resolve_cell_colors_inverse_swaps_fg_bg() {
    let attrs = Attrs {
        fg: Color::Indexed(1),
        bg: Color::Indexed(2),
        inverse: true,
        ..Attrs::default()
    };
    let (fg, bg) = resolve_cell_colors(&attrs);
    assert_eq!(fg, ANSI_PALETTE[2]);
    assert_eq!(bg, ANSI_PALETTE[1]);
}

#[test]
fn resolve_cell_colors_dim_halves_fg() {
    let attrs = Attrs {
        fg: Color::Rgb(200, 100, 50),
        dim: true,
        ..Attrs::default()
    };
    let (fg, _bg) = resolve_cell_colors(&attrs);
    assert_eq!(fg, rldyourterm_render_cpu::rgb_to_u32(100, 50, 25));
}

#[test]
fn encode_named_keys_without_modifiers() {
    use winit::keyboard::NamedKey;

    let mods = ModifiersState::empty();

    assert_eq!(
        encode_winit_key_event(&Key::Named(NamedKey::F1), mods),
        Some(b"\x1bOP".to_vec()),
    );
    assert_eq!(
        encode_winit_key_event(&Key::Named(NamedKey::F5), mods),
        Some(b"\x1b[15~".to_vec()),
    );
    assert_eq!(
        encode_winit_key_event(&Key::Named(NamedKey::PageUp), mods),
        Some(b"\x1b[5~".to_vec()),
    );
    assert_eq!(
        encode_winit_key_event(&Key::Named(NamedKey::Insert), mods),
        Some(b"\x1b[2~".to_vec()),
    );
    assert_eq!(
        encode_winit_key_event(&Key::Named(NamedKey::ArrowUp), mods),
        Some(b"\x1b[A".to_vec()),
    );
}

#[test]
fn encode_ctrl_arrow_produces_modified_csi() {
    use winit::keyboard::NamedKey;

    let ctrl = ModifiersState::CONTROL;

    assert_eq!(
        encode_winit_key_event(&Key::Named(NamedKey::ArrowLeft), ctrl),
        Some(b"\x1b[1;5D".to_vec()),
    );
    assert_eq!(
        encode_winit_key_event(&Key::Named(NamedKey::ArrowRight), ctrl),
        Some(b"\x1b[1;5C".to_vec()),
    );
    assert_eq!(
        encode_winit_key_event(&Key::Named(NamedKey::ArrowUp), ctrl),
        Some(b"\x1b[1;5A".to_vec()),
    );
    assert_eq!(
        encode_winit_key_event(&Key::Named(NamedKey::ArrowDown), ctrl),
        Some(b"\x1b[1;5B".to_vec()),
    );
}

#[test]
fn encode_shift_arrow_produces_modified_csi() {
    use winit::keyboard::NamedKey;

    let shift = ModifiersState::SHIFT;

    assert_eq!(
        encode_winit_key_event(&Key::Named(NamedKey::ArrowUp), shift),
        Some(b"\x1b[1;2A".to_vec()),
    );
}

#[test]
fn encode_alt_arrow_produces_modified_csi() {
    use winit::keyboard::NamedKey;

    let alt = ModifiersState::ALT;

    assert_eq!(
        encode_winit_key_event(&Key::Named(NamedKey::ArrowLeft), alt),
        Some(b"\x1b[1;3D".to_vec()),
    );
}

#[test]
fn encode_ctrl_shift_f1_produces_modified_csi() {
    use winit::keyboard::NamedKey;

    let ctrl_shift = ModifiersState::CONTROL | ModifiersState::SHIFT;

    assert_eq!(
        encode_winit_key_event(&Key::Named(NamedKey::F1), ctrl_shift),
        Some(b"\x1b[1;6P".to_vec()),
    );
}

#[test]
fn encode_shift_tab_produces_reverse_tab() {
    use winit::keyboard::NamedKey;

    let shift = ModifiersState::SHIFT;

    assert_eq!(
        encode_winit_key_event(&Key::Named(NamedKey::Tab), shift),
        Some(b"\x1b[Z".to_vec()),
    );
}

#[test]
fn encode_alt_backspace_produces_esc_del() {
    use winit::keyboard::NamedKey;

    let alt = ModifiersState::ALT;

    assert_eq!(
        encode_winit_key_event(&Key::Named(NamedKey::Backspace), alt),
        Some(b"\x1b\x7f".to_vec()),
    );
}
