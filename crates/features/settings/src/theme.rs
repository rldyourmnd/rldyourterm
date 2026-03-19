// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

use rldyourterm_core::{ANSI_PALETTE, TerminalTheme};

use crate::ThemePreset;

const CUBERPUNK_DEFAULT_FG: (u8, u8, u8) = (0xd8, 0xd8, 0xd8);
const CUBERPUNK_DEFAULT_BG: (u8, u8, u8) = (0x14, 0x1b, 0x1f);
const CUBERPUNK_ANSI: [u32; 16] = [
    0x00141b1f, 0x00ff7a90, 0x00b9f27c, 0x00ffd36e, 0x007ad5ff, 0x00ff80bf, 0x0073f7ff, 0x00d8d8d8,
    0x003e4b53, 0x00ff9db0, 0x00d7ff9b, 0x00ffe39f, 0x0099e2ff, 0x00ff9ed2, 0x0090ffff, 0x00f5f7fa,
];

const AURORA_DEFAULT_FG: (u8, u8, u8) = (0xd7, 0xe7, 0xee);
const AURORA_DEFAULT_BG: (u8, u8, u8) = (0x0f, 0x19, 0x24);
const AURORA_ANSI: [u32; 16] = [
    0x000f1924, 0x00ff8a7a, 0x007fd7a6, 0x00f2d06b, 0x007ab8ff, 0x00d6a6ff, 0x0077e2d7, 0x00d7e7ee,
    0x00374557, 0x00ffb29f, 0x00a7efc2, 0x00ffe08f, 0x0094c8ff, 0x00e2b9ff, 0x0091f0e7, 0x00f4fbff,
];

const MONOCHROME_DEFAULT_FG: (u8, u8, u8) = (0x22, 0x22, 0x22);
const MONOCHROME_DEFAULT_BG: (u8, u8, u8) = (0xf5, 0xf5, 0xf5);
const MONOCHROME_ANSI: [u32; 16] = [
    0x00181818, 0x00333333, 0x00444444, 0x00555555, 0x00666666, 0x00777777, 0x00888888, 0x00e2e2e2,
    0x00909090, 0x00a0a0a0, 0x00b0b0b0, 0x00c0c0c0, 0x00d0d0d0, 0x00dadada, 0x00e5e5e5, 0x00ffffff,
];

pub fn theme_for_preset(preset: ThemePreset) -> TerminalTheme {
    match preset {
        ThemePreset::Cuberpunk => TerminalTheme {
            default_fg: CUBERPUNK_DEFAULT_FG,
            default_bg: CUBERPUNK_DEFAULT_BG,
            palette: themed_palette(CUBERPUNK_ANSI),
        },
        ThemePreset::Aurora => TerminalTheme {
            default_fg: AURORA_DEFAULT_FG,
            default_bg: AURORA_DEFAULT_BG,
            palette: themed_palette(AURORA_ANSI),
        },
        ThemePreset::Monochrome => TerminalTheme {
            default_fg: MONOCHROME_DEFAULT_FG,
            default_bg: MONOCHROME_DEFAULT_BG,
            palette: themed_palette(MONOCHROME_ANSI),
        },
    }
}

fn themed_palette(base_ansi: [u32; 16]) -> [u32; 256] {
    let mut palette = ANSI_PALETTE;
    palette[..16].copy_from_slice(&base_ansi);
    palette
}
