// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

use rldyourterm_core::{ANSI_PALETTE, TerminalTheme};

use crate::ThemePreset;

const CUBERPUNK_DEFAULT_FG: (u8, u8, u8) = (0xd8, 0xd8, 0xd8);
const CUBERPUNK_DEFAULT_BG: (u8, u8, u8) = (0x14, 0x1b, 0x1f);
const CUBERPUNK_CURSOR_BG: (u8, u8, u8) = (0x73, 0xf7, 0xff);
const CUBERPUNK_SELECTION_BG: (u8, u8, u8) = (0x3e, 0x4b, 0x53);
const CUBERPUNK_ANSI: [u32; 16] = [
    0x00141b1f, 0x00ff7a90, 0x00b9f27c, 0x00ffd36e, 0x007ad5ff, 0x00ff80bf, 0x0073f7ff, 0x00d8d8d8,
    0x003e4b53, 0x00ff9db0, 0x00d7ff9b, 0x00ffe39f, 0x0099e2ff, 0x00ff9ed2, 0x0090ffff, 0x00f5f7fa,
];

const AURORA_DEFAULT_FG: (u8, u8, u8) = (0xd7, 0xe7, 0xee);
const AURORA_DEFAULT_BG: (u8, u8, u8) = (0x0f, 0x19, 0x24);
const AURORA_CURSOR_BG: (u8, u8, u8) = (0x77, 0xe2, 0xd7);
const AURORA_SELECTION_BG: (u8, u8, u8) = (0x37, 0x45, 0x57);
const AURORA_ANSI: [u32; 16] = [
    0x000f1924, 0x00ff8a7a, 0x007fd7a6, 0x00f2d06b, 0x007ab8ff, 0x00d6a6ff, 0x0077e2d7, 0x00d7e7ee,
    0x00374557, 0x00ffb29f, 0x00a7efc2, 0x00ffe08f, 0x0094c8ff, 0x00e2b9ff, 0x0091f0e7, 0x00f4fbff,
];

const MONOCHROME_DEFAULT_FG: (u8, u8, u8) = (0x22, 0x22, 0x22);
const MONOCHROME_DEFAULT_BG: (u8, u8, u8) = (0xf5, 0xf5, 0xf5);
const MONOCHROME_CURSOR_BG: (u8, u8, u8) = (0x22, 0x22, 0x22);
const MONOCHROME_SELECTION_BG: (u8, u8, u8) = (0xd0, 0xd0, 0xd0);
const MONOCHROME_ANSI: [u32; 16] = [
    0x00181818, 0x00333333, 0x00444444, 0x00555555, 0x00666666, 0x00777777, 0x00888888, 0x00e2e2e2,
    0x00909090, 0x00a0a0a0, 0x00b0b0b0, 0x00c0c0c0, 0x00d0d0d0, 0x00dadada, 0x00e5e5e5, 0x00ffffff,
];

const DARK_DEFAULT_FG: (u8, u8, u8) = (0xdc, 0xdf, 0xe4);
const DARK_DEFAULT_BG: (u8, u8, u8) = (0x28, 0x2c, 0x34);
const DARK_CURSOR_BG: (u8, u8, u8) = (0x61, 0xaf, 0xef);
const DARK_SELECTION_BG: (u8, u8, u8) = (0x3e, 0x44, 0x51);
const DARK_ANSI: [u32; 16] = [
    0x00282c34, 0x00e06c75, 0x0098c379, 0x00e5c07b, 0x0061afef, 0x00c678dd, 0x0056b6c2, 0x00dcdfe4,
    0x005c6370, 0x00f28b95, 0x00b8db87, 0x00f3d08c, 0x007abdf5, 0x00d19aef, 0x0075c6d0, 0x00f4f7ff,
];

const LIGHT_DEFAULT_FG: (u8, u8, u8) = (0x38, 0x3a, 0x42);
const LIGHT_DEFAULT_BG: (u8, u8, u8) = (0xfa, 0xfa, 0xfa);
const LIGHT_CURSOR_BG: (u8, u8, u8) = (0x40, 0x78, 0xf2);
const LIGHT_SELECTION_BG: (u8, u8, u8) = (0xdb, 0xe4, 0xed);
const LIGHT_ANSI: [u32; 16] = [
    0x00383a42, 0x00e45649, 0x0050a14f, 0x00c18401, 0x000184bc, 0x00a626a4, 0x000997b3, 0x00fafafa,
    0x005a5d68, 0x00ef5f66, 0x0068b357, 0x00d29a16, 0x004078f2, 0x00b751b6, 0x0020a5ba, 0x00ffffff,
];

const SOLARIZED_DEFAULT_FG: (u8, u8, u8) = (0x83, 0x94, 0x96);
const SOLARIZED_DEFAULT_BG: (u8, u8, u8) = (0x00, 0x2b, 0x36);
const SOLARIZED_CURSOR_BG: (u8, u8, u8) = (0x93, 0xa1, 0xa1);
const SOLARIZED_SELECTION_BG: (u8, u8, u8) = (0x07, 0x36, 0x42);
const SOLARIZED_ANSI: [u32; 16] = [
    0x00073642, 0x00dc322f, 0x00859900, 0x00b58900, 0x00268bd2, 0x00d33682, 0x002aa198, 0x00eee8d5,
    0x00002b36, 0x00cb4b16, 0x00586e75, 0x00657b83, 0x00839496, 0x006c71c4, 0x0093a1a1, 0x00fdf6e3,
];

const DRACULA_DEFAULT_FG: (u8, u8, u8) = (0xf8, 0xf8, 0xf2);
const DRACULA_DEFAULT_BG: (u8, u8, u8) = (0x28, 0x2a, 0x36);
const DRACULA_CURSOR_BG: (u8, u8, u8) = (0xf8, 0xf8, 0xf2);
const DRACULA_SELECTION_BG: (u8, u8, u8) = (0x44, 0x47, 0x5a);
const DRACULA_ANSI: [u32; 16] = [
    0x0021222c, 0x00ff5555, 0x0050fa7b, 0x00f1fa8c, 0x00bd93f9, 0x00ff79c6, 0x008be9fd, 0x00f8f8f2,
    0x006272a4, 0x00ff6e6e, 0x0069ff94, 0x00ffffa5, 0x00d6acff, 0x00ff92df, 0x00a4ffff, 0x00ffffff,
];

const CATPPUCCIN_DEFAULT_FG: (u8, u8, u8) = (0xcd, 0xd6, 0xf4);
const CATPPUCCIN_DEFAULT_BG: (u8, u8, u8) = (0x1e, 0x1e, 0x2e);
const CATPPUCCIN_CURSOR_BG: (u8, u8, u8) = (0xf5, 0xe0, 0xdc);
const CATPPUCCIN_SELECTION_BG: (u8, u8, u8) = (0x45, 0x47, 0x5a);
const CATPPUCCIN_ANSI: [u32; 16] = [
    0x0045475a, 0x00f38ba8, 0x00a6e3a1, 0x00f9e2af, 0x0089b4fa, 0x00f5c2e7, 0x0094e2d5, 0x00bac2de,
    0x00585b70, 0x00f38ba8, 0x00a6e3a1, 0x00f9e2af, 0x0089b4fa, 0x00cba6f7, 0x0074c7ec, 0x00cdd6f4,
];

pub fn theme_for_preset(preset: ThemePreset) -> TerminalTheme {
    match preset {
        ThemePreset::Cuberpunk => preset_theme(
            CUBERPUNK_DEFAULT_FG,
            CUBERPUNK_DEFAULT_BG,
            CUBERPUNK_CURSOR_BG,
            CUBERPUNK_SELECTION_BG,
            CUBERPUNK_ANSI,
        ),
        ThemePreset::Aurora => preset_theme(
            AURORA_DEFAULT_FG,
            AURORA_DEFAULT_BG,
            AURORA_CURSOR_BG,
            AURORA_SELECTION_BG,
            AURORA_ANSI,
        ),
        ThemePreset::Monochrome => preset_theme(
            MONOCHROME_DEFAULT_FG,
            MONOCHROME_DEFAULT_BG,
            MONOCHROME_CURSOR_BG,
            MONOCHROME_SELECTION_BG,
            MONOCHROME_ANSI,
        ),
        ThemePreset::Dark => preset_theme(
            DARK_DEFAULT_FG,
            DARK_DEFAULT_BG,
            DARK_CURSOR_BG,
            DARK_SELECTION_BG,
            DARK_ANSI,
        ),
        ThemePreset::Light => preset_theme(
            LIGHT_DEFAULT_FG,
            LIGHT_DEFAULT_BG,
            LIGHT_CURSOR_BG,
            LIGHT_SELECTION_BG,
            LIGHT_ANSI,
        ),
        ThemePreset::Solarized => preset_theme(
            SOLARIZED_DEFAULT_FG,
            SOLARIZED_DEFAULT_BG,
            SOLARIZED_CURSOR_BG,
            SOLARIZED_SELECTION_BG,
            SOLARIZED_ANSI,
        ),
        ThemePreset::Dracula => preset_theme(
            DRACULA_DEFAULT_FG,
            DRACULA_DEFAULT_BG,
            DRACULA_CURSOR_BG,
            DRACULA_SELECTION_BG,
            DRACULA_ANSI,
        ),
        ThemePreset::Catppuccin => preset_theme(
            CATPPUCCIN_DEFAULT_FG,
            CATPPUCCIN_DEFAULT_BG,
            CATPPUCCIN_CURSOR_BG,
            CATPPUCCIN_SELECTION_BG,
            CATPPUCCIN_ANSI,
        ),
    }
}

fn preset_theme(
    default_fg: (u8, u8, u8),
    default_bg: (u8, u8, u8),
    cursor_bg: (u8, u8, u8),
    selection_bg: (u8, u8, u8),
    base_ansi: [u32; 16],
) -> TerminalTheme {
    TerminalTheme {
        default_fg,
        default_bg,
        cursor_fg: default_bg,
        cursor_bg,
        selection_fg: default_fg,
        selection_bg,
        palette: themed_palette(base_ansi),
    }
}

fn themed_palette(base_ansi: [u32; 16]) -> [u32; 256] {
    let mut palette = ANSI_PALETTE;
    palette[..16].copy_from_slice(&base_ansi);
    palette
}
