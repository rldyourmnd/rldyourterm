// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

mod input;
mod mouse;
mod palette;
mod state;

pub use input::{
    encode_crossterm_key_event, encode_winit_key_event, is_local_shutdown_key,
    is_local_shutdown_key_crossterm, is_local_shutdown_key_winit, is_runtime_palette_shortcut,
    is_runtime_palette_shortcut_crossterm, is_runtime_palette_shortcut_winit,
    runtime_key_event_from_crossterm, runtime_key_event_from_winit,
    runtime_key_from_winit_borrowed,
};
pub use mouse::{
    PRIMARY_MOUSE_BUTTON_CODE, active_mouse_button_code, encode_mouse_event,
    mouse_button_code_from_winit,
};
pub use palette::{
    RUNTIME_PALETTE_CLOSED_LINE, RUNTIME_PALETTE_HELP_LINE, RuntimePaletteAction,
    RuntimePaletteDecision, RuntimePaletteDispatchResult, RuntimePaletteView,
    dispatch_runtime_palette_command, handle_runtime_palette_key_input, runtime_palette_info_line,
    runtime_palette_status_line, toggle_runtime_palette,
};
pub use rldyourterm_core::{
    RuntimeKey, RuntimeKeyEvent, RuntimeKeyModifiers, TerminalModeFlags, encode_runtime_key_event,
};
pub use state::{GridPoint, InteractionState, PointerState, SelectionRange};
