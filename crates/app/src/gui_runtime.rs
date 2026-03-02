use std::io::{self, ErrorKind, Read, Write};
use std::num::NonZeroU32;
use std::rc::Rc;
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use anyhow::{Context, Result, anyhow};
use font8x8::{BASIC_FONTS, UnicodeFonts};
use rldyourterm_foundation::api::pty::{PtyFactory, PtyIo, PtySize, PtySpawnConfig};
use rldyourterm_foundation_platform::pty::PlatformPtyFactory;
use rldyourterm_services::render_mode::RenderMode;
use softbuffer::{Context as SoftbufferContext, Surface as SoftbufferSurface};
use tracing::{info, warn};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::event::{ElementState, Ime, KeyEvent as WinitKeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{Window, WindowId};

const DEFAULT_GUI_WIDTH: u32 = 1280;
const DEFAULT_GUI_HEIGHT: u32 = 800;
const DEFAULT_COLS: u16 = 120;
const DEFAULT_ROWS: u16 = 32;
const MAX_SCROLLBACK_LINES: usize = 50_000;

const CELL_WIDTH: usize = 8;
const CELL_HEIGHT: usize = 16;
const TAB_WIDTH: usize = 4;

const COLOR_BG: u32 = 0x0014_1b1f;
const COLOR_FG: u32 = 0x00d8_d8d8;

#[derive(Debug)]
enum GuiEvent {
    Output(Vec<u8>),
    Exited(i32),
    Failure(String),
}

type SpawnedPty = (Arc<dyn PtyIo>, Box<dyn Write + Send>, Box<dyn Read + Send>);

pub fn run_interactive_gui_pty(
    shell_executable: &str,
    shell_args: &[String],
    initial_mode: RenderMode,
    refresh_rate_millihz: u32,
    window_count: u8,
) -> Result<i32> {
    if window_count != 1 {
        return Err(anyhow!(
            "gui runtime requires single-window mode; got window_count={window_count}"
        ));
    }

    let (pty, writer, reader) = spawn_pty(shell_executable, shell_args)?;

    let event_loop = EventLoop::<GuiEvent>::with_user_event()
        .build()
        .context("failed to create GUI event loop")?;
    let proxy = event_loop.create_proxy();

    let reader_pump = spawn_reader_pump(reader, proxy.clone());
    let wait_pump = spawn_wait_pump(Arc::clone(&pty), proxy.clone());

    let mut app = GuiRuntimeApp::new(
        pty,
        writer,
        reader_pump,
        wait_pump,
        initial_mode,
        refresh_rate_millihz,
    );

    info!(
        mode = ?initial_mode,
        refresh_rate_millihz,
        windows = window_count,
        "starting GUI runtime"
    );

    event_loop
        .run_app(&mut app)
        .context("GUI event loop failed")?;

    app.shutdown();

    if let Some(error) = app.fatal_error.take() {
        return Err(error);
    }

    Ok(app.exit_code.unwrap_or(0))
}

fn spawn_pty(shell_executable: &str, shell_args: &[String]) -> Result<SpawnedPty> {
    let spawn_config = PtySpawnConfig {
        shell_command: shell_executable.to_owned(),
        args: shell_args.to_vec(),
        cwd: None,
        env: Vec::new(),
        size: PtySize {
            cols: DEFAULT_COLS,
            rows: DEFAULT_ROWS,
            pixel_width: 0,
            pixel_height: 0,
        },
    };

    let factory = PlatformPtyFactory;
    let pty_box = factory
        .spawn(spawn_config)
        .context("failed to spawn PTY for GUI runtime")?;

    let reader = pty_box
        .take_reader()
        .context("failed to acquire PTY reader for GUI runtime")?;
    let writer = pty_box
        .take_writer()
        .context("failed to acquire PTY writer for GUI runtime")?;

    let pty: Arc<dyn PtyIo> = Arc::from(pty_box);
    Ok((pty, writer, reader))
}

fn spawn_reader_pump(
    mut reader: Box<dyn Read + Send>,
    proxy: EventLoopProxy<GuiEvent>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        let mut buffer = [0_u8; 4096];

        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(read_bytes) => {
                    if proxy
                        .send_event(GuiEvent::Output(buffer[..read_bytes].to_vec()))
                        .is_err()
                    {
                        break;
                    }
                }
                Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                Err(error) => {
                    let _ = proxy.send_event(GuiEvent::Failure(format!(
                        "PTY reader pump failed: {error}"
                    )));
                    break;
                }
            }
        }
    })
}

fn spawn_wait_pump(pty: Arc<dyn PtyIo>, proxy: EventLoopProxy<GuiEvent>) -> JoinHandle<()> {
    thread::spawn(move || match pty.wait() {
        Ok(code) => {
            let _ = proxy.send_event(GuiEvent::Exited(code));
        }
        Err(error) => {
            let _ = proxy.send_event(GuiEvent::Failure(format!("PTY wait failed: {error}")));
        }
    })
}

struct GuiRuntimeApp {
    pty: Arc<dyn PtyIo>,
    writer: Box<dyn Write + Send>,
    reader_pump: Option<JoinHandle<()>>,
    wait_pump: Option<JoinHandle<()>>,
    initial_mode: RenderMode,
    refresh_rate_millihz: u32,

    window: Option<Rc<Window>>,
    window_id: Option<WindowId>,
    _context: Option<SoftbufferContext<Rc<Window>>>,
    surface: Option<SoftbufferSurface<Rc<Window>, Rc<Window>>>,
    window_size: PhysicalSize<u32>,
    terminal: TerminalBuffer,
    modifiers: ModifiersState,
    redraw_pending: bool,

    exit_code: Option<i32>,
    fatal_error: Option<anyhow::Error>,
}

impl GuiRuntimeApp {
    fn new(
        pty: Arc<dyn PtyIo>,
        writer: Box<dyn Write + Send>,
        reader_pump: JoinHandle<()>,
        wait_pump: JoinHandle<()>,
        initial_mode: RenderMode,
        refresh_rate_millihz: u32,
    ) -> Self {
        Self {
            pty,
            writer,
            reader_pump: Some(reader_pump),
            wait_pump: Some(wait_pump),
            initial_mode,
            refresh_rate_millihz,
            window: None,
            window_id: None,
            _context: None,
            surface: None,
            window_size: PhysicalSize::new(DEFAULT_GUI_WIDTH, DEFAULT_GUI_HEIGHT),
            terminal: TerminalBuffer::new(MAX_SCROLLBACK_LINES),
            modifiers: ModifiersState::default(),
            redraw_pending: true,
            exit_code: None,
            fatal_error: None,
        }
    }

    fn bootstrap_window(&mut self, event_loop: &ActiveEventLoop) -> Result<()> {
        if self.window.is_some() {
            return Ok(());
        }

        let title = format!(
            "rldyourterm GUI MVP [{:?}] {} mHz",
            self.initial_mode, self.refresh_rate_millihz
        );

        let attributes = Window::default_attributes()
            .with_title(title)
            .with_inner_size(LogicalSize::new(DEFAULT_GUI_WIDTH, DEFAULT_GUI_HEIGHT));
        let window = Rc::new(
            event_loop
                .create_window(attributes)
                .context("failed to create GUI window")?,
        );

        let context = SoftbufferContext::new(window.clone())
            .map_err(|error| anyhow!("failed to create softbuffer context: {error}"))?;
        let surface = SoftbufferSurface::new(&context, window.clone())
            .map_err(|error| anyhow!("failed to create softbuffer surface: {error}"))?;

        self.window_size = window.inner_size();
        self.window_id = Some(window.id());
        self._context = Some(context);
        self.surface = Some(surface);
        self.window = Some(window);

        self.update_viewport_geometry();
        self.queue_redraw();
        Ok(())
    }

    fn shutdown(&mut self) {
        if let Err(error) = self.pty.close() {
            warn!(error = %error, "failed to close PTY during GUI shutdown");
            if self.fatal_error.is_none() {
                self.fatal_error = Some(anyhow!("failed to close PTY: {error}"));
            }
        }

        if let Some(handle) = self.reader_pump.take()
            && let Err(join_error) = handle.join()
        {
            warn!(?join_error, "PTY reader pump thread join failed");
        }

        if let Some(handle) = self.wait_pump.take()
            && let Err(join_error) = handle.join()
        {
            warn!(?join_error, "PTY wait pump thread join failed");
        }
    }

    fn queue_redraw(&mut self) {
        self.redraw_pending = true;
    }

    fn request_redraw_if_needed(&mut self) {
        if !self.redraw_pending {
            return;
        }
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
            self.redraw_pending = false;
        }
    }

    fn update_viewport_geometry(&mut self) {
        let cols = ((self.window_size.width as usize) / CELL_WIDTH)
            .max(1)
            .min(u16::MAX as usize) as u16;
        let rows = ((self.window_size.height as usize) / CELL_HEIGHT)
            .max(1)
            .min(u16::MAX as usize) as u16;

        self.terminal.set_columns(cols as usize);

        if let Err(error) = self.pty.resize(PtySize {
            cols,
            rows,
            pixel_width: self.window_size.width.min(u16::MAX as u32) as u16,
            pixel_height: self.window_size.height.min(u16::MAX as u32) as u16,
        }) {
            warn!(error = %error, cols, rows, "failed to resize PTY to viewport");
        }
    }

    fn handle_close_requested(&mut self, event_loop: &ActiveEventLoop) {
        self.exit_code.get_or_insert(0);
        event_loop.exit();
    }

    fn handle_keyboard_input(&mut self, event: &WinitKeyEvent, event_loop: &ActiveEventLoop) {
        if event.state != ElementState::Pressed {
            return;
        }

        if is_local_shutdown_key(event, self.modifiers) {
            self.exit_code.get_or_insert(0);
            event_loop.exit();
            return;
        }

        if let Some(bytes) = encode_winit_key_event(event, self.modifiers)
            && let Err(error) = write_all_and_flush(&mut *self.writer, &bytes)
        {
            if self.handle_writer_error(error, event_loop) {
                return;
            }
            self.fatal_error = Some(anyhow!("failed to write keyboard input to PTY"));
            event_loop.exit();
        }
    }

    fn handle_text_commit(&mut self, text: &str, event_loop: &ActiveEventLoop) {
        if text.is_empty() {
            return;
        }

        if let Err(error) = write_all_and_flush(&mut *self.writer, text.as_bytes()) {
            if self.handle_writer_error(error, event_loop) {
                return;
            }
            self.fatal_error = Some(anyhow!("failed to write IME text to PTY"));
            event_loop.exit();
        }
    }

    fn handle_writer_error(&mut self, error: io::Error, event_loop: &ActiveEventLoop) -> bool {
        if !is_disconnect_error(&error) {
            return false;
        }

        match self.pty.try_wait() {
            Ok(Some(code)) => {
                self.exit_code = Some(code);
            }
            Ok(None) => {
                self.exit_code.get_or_insert(0);
            }
            Err(wait_error) => {
                self.fatal_error = Some(anyhow!(
                    "failed to poll PTY after write error: {wait_error}"
                ));
            }
        }

        event_loop.exit();
        true
    }

    fn draw_frame(&mut self) -> Result<()> {
        let width = self.window_size.width;
        let height = self.window_size.height;
        if width == 0 || height == 0 {
            return Ok(());
        }

        let surface = self
            .surface
            .as_mut()
            .ok_or_else(|| anyhow!("softbuffer surface is not initialized"))?;

        let nz_width = NonZeroU32::new(width).ok_or_else(|| anyhow!("zero width is invalid"))?;
        let nz_height = NonZeroU32::new(height).ok_or_else(|| anyhow!("zero height is invalid"))?;
        surface
            .resize(nz_width, nz_height)
            .map_err(|error| anyhow!("failed to resize softbuffer surface: {error}"))?;

        let mut buffer = surface
            .buffer_mut()
            .map_err(|error| anyhow!("failed to acquire softbuffer frame: {error}"))?;
        render_terminal(&mut buffer, width as usize, height as usize, &self.terminal);
        buffer
            .present()
            .map_err(|error| anyhow!("failed to present GUI frame: {error}"))?;
        Ok(())
    }
}

impl ApplicationHandler<GuiEvent> for GuiRuntimeApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if let Err(error) = self.bootstrap_window(event_loop) {
            self.fatal_error = Some(error);
            event_loop.exit();
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: GuiEvent) {
        match event {
            GuiEvent::Output(data) => {
                self.terminal.push_bytes(&data);
                self.queue_redraw();
            }
            GuiEvent::Exited(code) => {
                self.exit_code = Some(code);
                event_loop.exit();
            }
            GuiEvent::Failure(message) => {
                self.fatal_error = Some(anyhow!(message));
                event_loop.exit();
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        if Some(window_id) != self.window_id {
            return;
        }

        match event {
            WindowEvent::CloseRequested => self.handle_close_requested(event_loop),
            WindowEvent::RedrawRequested => {
                if let Err(error) = self.draw_frame() {
                    self.fatal_error = Some(error);
                    event_loop.exit();
                }
            }
            WindowEvent::Resized(size) => {
                self.window_size = size;
                self.update_viewport_geometry();
                self.queue_redraw();
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                if let Some(window) = self.window.as_ref() {
                    self.window_size = window.inner_size();
                    self.update_viewport_geometry();
                    self.queue_redraw();
                }
            }
            WindowEvent::KeyboardInput {
                event,
                is_synthetic,
                ..
            } if !is_synthetic => self.handle_keyboard_input(&event, event_loop),
            WindowEvent::Ime(Ime::Commit(text)) => self.handle_text_commit(&text, event_loop),
            WindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers.state();
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        self.request_redraw_if_needed();
    }
}

fn render_terminal(buffer: &mut [u32], width: usize, height: usize, terminal: &TerminalBuffer) {
    buffer.fill(COLOR_BG);

    if width == 0 || height == 0 {
        return;
    }

    let rows = (height / CELL_HEIGHT).max(1);
    let cols = (width / CELL_WIDTH).max(1);
    let visible_lines = terminal.visible_lines(rows);
    let top_row = rows.saturating_sub(visible_lines.len());

    for (row_offset, line) in visible_lines.iter().enumerate() {
        draw_line(buffer, width, height, top_row + row_offset, cols, line);
    }
}

fn draw_line(buffer: &mut [u32], width: usize, height: usize, row: usize, cols: usize, text: &str) {
    let base_y = row * CELL_HEIGHT;
    if base_y >= height {
        return;
    }

    for (col, ch) in text.chars().take(cols).enumerate() {
        draw_char(buffer, width, height, col * CELL_WIDTH, base_y, ch);
    }
}

fn draw_char(buffer: &mut [u32], width: usize, height: usize, x: usize, y: usize, ch: char) {
    let glyph = BASIC_FONTS
        .get(ch)
        .or_else(|| BASIC_FONTS.get('?'))
        .unwrap_or([0; 8]);

    for (glyph_y, row_bits) in glyph.iter().enumerate() {
        for glyph_x in 0..8 {
            if (row_bits >> glyph_x) & 1 == 0 {
                continue;
            }

            let pixel_x = x + glyph_x;
            let pixel_y = y + glyph_y * 2;
            if pixel_x >= width || pixel_y >= height {
                continue;
            }

            let index = pixel_y * width + pixel_x;
            buffer[index] = COLOR_FG;

            let pixel_y2 = pixel_y + 1;
            if pixel_y2 < height {
                let index2 = pixel_y2 * width + pixel_x;
                buffer[index2] = COLOR_FG;
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EscapeState {
    None,
    Esc,
    Csi,
}

#[derive(Debug)]
struct TerminalBuffer {
    lines: Vec<String>,
    max_lines: usize,
    cols: usize,
    escape_state: EscapeState,
}

impl TerminalBuffer {
    fn new(max_lines: usize) -> Self {
        Self {
            lines: vec![String::new()],
            max_lines,
            cols: DEFAULT_COLS as usize,
            escape_state: EscapeState::None,
        }
    }

    fn set_columns(&mut self, cols: usize) {
        self.cols = cols.max(1);
    }

    fn visible_lines(&self, rows: usize) -> &[String] {
        let count = rows.max(1);
        let start = self.lines.len().saturating_sub(count);
        &self.lines[start..]
    }

    fn push_bytes(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            if self.consume_escape(byte) {
                continue;
            }

            match byte {
                b'\x1b' => self.escape_state = EscapeState::Esc,
                b'\r' => {
                    self.current_line_mut().clear();
                }
                b'\n' => self.new_line(),
                0x08 | 0x7f => {
                    self.current_line_mut().pop();
                }
                b'\t' => {
                    for _ in 0..TAB_WIDTH {
                        self.push_char(' ');
                    }
                }
                0x20..=0x7e => self.push_char(byte as char),
                _ if byte >= 0x80 => self.push_char('�'),
                _ => {}
            }
        }
    }

    fn consume_escape(&mut self, byte: u8) -> bool {
        match self.escape_state {
            EscapeState::None => false,
            EscapeState::Esc => {
                self.escape_state = if byte == b'[' {
                    EscapeState::Csi
                } else {
                    EscapeState::None
                };
                true
            }
            EscapeState::Csi => {
                if (0x40..=0x7e).contains(&byte) {
                    self.escape_state = EscapeState::None;
                }
                true
            }
        }
    }

    fn push_char(&mut self, ch: char) {
        let line = self.current_line_mut();
        line.push(ch);
        if line.chars().count() >= self.cols {
            self.new_line();
        }
    }

    fn new_line(&mut self) {
        self.lines.push(String::new());
        while self.lines.len() > self.max_lines {
            self.lines.remove(0);
        }
    }

    fn current_line_mut(&mut self) -> &mut String {
        if self.lines.is_empty() {
            self.lines.push(String::new());
        }
        let last = self.lines.len() - 1;
        &mut self.lines[last]
    }
}

fn is_local_shutdown_key(event: &WinitKeyEvent, modifiers: ModifiersState) -> bool {
    if !modifiers.control_key() {
        return false;
    }

    match event.logical_key.as_ref() {
        Key::Character(text) => text.eq_ignore_ascii_case("q"),
        _ => false,
    }
}

fn encode_winit_key_event(event: &WinitKeyEvent, modifiers: ModifiersState) -> Option<Vec<u8>> {
    match event.logical_key.as_ref() {
        Key::Named(NamedKey::Enter) => Some(vec![b'\r']),
        Key::Named(NamedKey::Tab) => Some(vec![b'\t']),
        Key::Named(NamedKey::Escape) => Some(vec![0x1b]),
        Key::Named(NamedKey::Backspace) => Some(vec![0x7f]),
        Key::Named(NamedKey::ArrowUp) => Some(b"\x1b[A".to_vec()),
        Key::Named(NamedKey::ArrowDown) => Some(b"\x1b[B".to_vec()),
        Key::Named(NamedKey::ArrowRight) => Some(b"\x1b[C".to_vec()),
        Key::Named(NamedKey::ArrowLeft) => Some(b"\x1b[D".to_vec()),
        Key::Named(NamedKey::Home) => Some(b"\x1b[H".to_vec()),
        Key::Named(NamedKey::End) => Some(b"\x1b[F".to_vec()),
        Key::Named(NamedKey::Delete) => Some(b"\x1b[3~".to_vec()),
        Key::Character(text) if modifiers.control_key() => {
            let mut chars = text.chars();
            let ch = chars.next()?;
            if chars.next().is_some() {
                return None;
            }
            encode_ctrl_letter(ch).map(|code| vec![code])
        }
        _ => None,
    }
}

fn encode_ctrl_letter(ch: char) -> Option<u8> {
    let lower = ch.to_ascii_lowercase();
    if lower.is_ascii_lowercase() {
        Some((lower as u8) - b'a' + 1)
    } else {
        None
    }
}

fn write_all_and_flush(writer: &mut dyn Write, bytes: &[u8]) -> io::Result<()> {
    writer.write_all(bytes)?;
    writer.flush()
}

fn is_disconnect_error(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        ErrorKind::BrokenPipe
            | ErrorKind::ConnectionAborted
            | ErrorKind::ConnectionReset
            | ErrorKind::NotConnected
            | ErrorKind::UnexpectedEof
    )
}

#[cfg(test)]
mod tests {
    use super::{TerminalBuffer, encode_ctrl_letter};

    #[test]
    fn terminal_buffer_keeps_recent_lines() {
        let mut buffer = TerminalBuffer::new(3);
        buffer.set_columns(80);
        buffer.push_bytes(b"one\ntwo\nthree\nfour\n");

        let lines = buffer.visible_lines(3);
        assert_eq!(lines, ["three", "four", ""]);
    }

    #[test]
    fn terminal_buffer_strips_ansi_sequences() {
        let mut buffer = TerminalBuffer::new(10);
        buffer.set_columns(80);
        buffer.push_bytes(b"\x1b[31mred\x1b[0m\n");

        let lines = buffer.visible_lines(2);
        assert_eq!(lines, ["red", ""]);
    }

    #[test]
    fn ctrl_letter_encoding_matches_ascii_control_range() {
        assert_eq!(encode_ctrl_letter('a'), Some(0x01));
        assert_eq!(encode_ctrl_letter('c'), Some(0x03));
        assert_eq!(encode_ctrl_letter('z'), Some(0x1a));
        assert_eq!(encode_ctrl_letter('1'), None);
    }
}
