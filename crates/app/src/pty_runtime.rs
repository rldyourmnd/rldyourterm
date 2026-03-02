use std::io::{self, ErrorKind, Read, Write};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::terminal;
use rldyourterm_foundation::api::pty::{PtyFactory, PtySize, PtySpawnConfig};
use rldyourterm_foundation_platform::pty::PlatformPtyFactory;
use rldyourterm_services::render_mode::RenderMode;
use tracing::warn;

const DEFAULT_COLS: u16 = 80;
const DEFAULT_ROWS: u16 = 24;
const EVENT_POLL_TIMEOUT: Duration = Duration::from_millis(25);

struct RawModeGuard;

impl RawModeGuard {
    fn new() -> Result<Self> {
        terminal::enable_raw_mode().context("failed to enable terminal raw mode")?;
        Ok(Self)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
    }
}

pub fn run_interactive_pty(
    shell_executable: &str,
    shell_args: &[String],
    initial_mode: RenderMode,
    refresh_rate_millihz: u32,
    window_count: u8,
) -> Result<i32> {
    let _mvp_runtime_hints = (initial_mode, refresh_rate_millihz, window_count);

    let initial_size = current_pty_size();
    let spawn_config = PtySpawnConfig {
        shell_command: shell_executable.to_owned(),
        args: shell_args.to_vec(),
        cwd: None,
        env: Vec::new(),
        size: initial_size,
    };

    let factory = PlatformPtyFactory;
    let pty = factory
        .spawn(spawn_config)
        .context("failed to spawn interactive PTY")?;
    let reader = pty.take_reader().context("failed to acquire PTY reader")?;
    let mut writer = pty.take_writer().context("failed to acquire PTY writer")?;

    let _raw_mode_guard = RawModeGuard::new()?;
    let read_pump = spawn_read_pump(reader);

    let mut exit_code: Option<i32> = None;
    let mut requested_local_exit = false;
    let mut fatal_error: Option<anyhow::Error> = None;

    loop {
        match pty.try_wait().context("failed to poll PTY child status") {
            Ok(Some(code)) => {
                exit_code = Some(code);
                break;
            }
            Ok(None) => {}
            Err(error) => {
                fatal_error = Some(error);
                break;
            }
        }

        let has_event =
            match event::poll(EVENT_POLL_TIMEOUT).context("failed to poll terminal events") {
                Ok(value) => value,
                Err(error) => {
                    fatal_error = Some(error);
                    break;
                }
            };
        if !has_event {
            continue;
        }

        let terminal_event = match event::read().context("failed to read terminal event") {
            Ok(value) => value,
            Err(error) => {
                fatal_error = Some(error);
                break;
            }
        };

        match terminal_event {
            Event::Key(key_event) if is_press_like(key_event.kind) => {
                if is_local_shutdown_key(key_event) {
                    requested_local_exit = true;
                    break;
                }

                if let Some(bytes) = encode_key_event(key_event) {
                    if let Err(error) = write_all_and_flush(&mut *writer, &bytes) {
                        if is_disconnect_error(&error) {
                            match pty
                                .try_wait()
                                .context("failed to poll PTY after write failure")
                            {
                                Ok(Some(code)) => exit_code = Some(code),
                                Ok(None) => {}
                                Err(wait_error) => fatal_error = Some(wait_error),
                            }
                            break;
                        }
                        fatal_error = Some(
                            anyhow::Error::new(error).context("failed to write key event to PTY"),
                        );
                        break;
                    }
                }
            }
            Event::Resize(cols, rows) => {
                let resized = PtySize {
                    cols,
                    rows,
                    pixel_width: 0,
                    pixel_height: 0,
                };
                if let Err(error) = pty.resize(resized) {
                    match pty
                        .try_wait()
                        .context("failed to poll PTY after resize failure")
                    {
                        Ok(Some(code)) => {
                            exit_code = Some(code);
                        }
                        Ok(None) => {
                            fatal_error =
                                Some(anyhow::Error::new(error).context("failed to resize PTY"));
                        }
                        Err(wait_error) => {
                            fatal_error = Some(wait_error);
                        }
                    }
                    break;
                }
            }
            _ => {}
        }
    }

    if requested_local_exit {
        exit_code.get_or_insert(0);
    } else if exit_code.is_none() && fatal_error.is_none() {
        match pty
            .wait()
            .context("failed while waiting for PTY child exit")
        {
            Ok(code) => exit_code = Some(code),
            Err(error) => fatal_error = Some(error),
        }
    }

    if let Err(error) = pty.close().context("failed to close PTY session") {
        if fatal_error.is_none() {
            fatal_error = Some(error);
        }
    }

    if let Err(join_error) = read_pump.join() {
        warn!(?join_error, "pty read pump terminated unexpectedly");
    }

    if let Some(error) = fatal_error {
        return Err(error);
    }
    Ok(exit_code.unwrap_or(0))
}

fn current_pty_size() -> PtySize {
    match terminal::size() {
        Ok((cols, rows)) => PtySize {
            cols,
            rows,
            pixel_width: 0,
            pixel_height: 0,
        },
        Err(error) => {
            warn!(?error, "failed to read terminal size; using 80x24 default");
            PtySize {
                cols: DEFAULT_COLS,
                rows: DEFAULT_ROWS,
                pixel_width: 0,
                pixel_height: 0,
            }
        }
    }
}

fn spawn_read_pump(mut reader: Box<dyn Read + Send>) -> JoinHandle<()> {
    thread::spawn(move || {
        let mut stdout = io::stdout();
        let mut buffer = [0_u8; 4096];

        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(read_bytes) => {
                    if stdout.write_all(&buffer[..read_bytes]).is_err() {
                        break;
                    }
                    if stdout.flush().is_err() {
                        break;
                    }
                }
                Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                Err(_) => break,
            }
        }
    })
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

fn is_press_like(kind: KeyEventKind) -> bool {
    matches!(kind, KeyEventKind::Press | KeyEventKind::Repeat)
}

fn is_local_shutdown_key(key_event: KeyEvent) -> bool {
    key_event.modifiers.contains(KeyModifiers::CONTROL)
        && matches!(key_event.code, KeyCode::Char('q') | KeyCode::Char('Q'))
}

fn encode_key_event(key_event: KeyEvent) -> Option<Vec<u8>> {
    match key_event.code {
        KeyCode::Enter => Some(vec![b'\r']),
        KeyCode::Backspace => Some(vec![0x7f]),
        KeyCode::Tab => Some(vec![b'\t']),
        KeyCode::Esc => Some(vec![0x1b]),
        KeyCode::Up => Some(b"\x1b[A".to_vec()),
        KeyCode::Down => Some(b"\x1b[B".to_vec()),
        KeyCode::Right => Some(b"\x1b[C".to_vec()),
        KeyCode::Left => Some(b"\x1b[D".to_vec()),
        KeyCode::Home => Some(b"\x1b[H".to_vec()),
        KeyCode::End => Some(b"\x1b[F".to_vec()),
        KeyCode::Delete => Some(b"\x1b[3~".to_vec()),
        KeyCode::Char(ch) if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
            encode_ctrl_letter(ch).map(|code| vec![code])
        }
        KeyCode::Char(ch) => Some(ch.to_string().into_bytes()),
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

#[cfg(test)]
mod tests {
    use super::{encode_ctrl_letter, encode_key_event};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[test]
    fn encodes_basic_control_keys() {
        assert_eq!(
            encode_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            Some(vec![b'\r'])
        );
        assert_eq!(
            encode_key_event(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE)),
            Some(vec![0x7f])
        );
        assert_eq!(
            encode_key_event(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
            Some(vec![b'\t'])
        );
        assert_eq!(
            encode_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            Some(vec![0x1b])
        );
    }

    #[test]
    fn encodes_navigation_keys() {
        assert_eq!(
            encode_key_event(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE)),
            Some(b"\x1b[A".to_vec())
        );
        assert_eq!(
            encode_key_event(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)),
            Some(b"\x1b[B".to_vec())
        );
        assert_eq!(
            encode_key_event(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE)),
            Some(b"\x1b[C".to_vec())
        );
        assert_eq!(
            encode_key_event(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE)),
            Some(b"\x1b[D".to_vec())
        );
        assert_eq!(
            encode_key_event(KeyEvent::new(KeyCode::Home, KeyModifiers::NONE)),
            Some(b"\x1b[H".to_vec())
        );
        assert_eq!(
            encode_key_event(KeyEvent::new(KeyCode::End, KeyModifiers::NONE)),
            Some(b"\x1b[F".to_vec())
        );
        assert_eq!(
            encode_key_event(KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE)),
            Some(b"\x1b[3~".to_vec())
        );
    }

    #[test]
    fn encodes_ctrl_letters_case_insensitive() {
        assert_eq!(encode_ctrl_letter('a'), Some(0x01));
        assert_eq!(encode_ctrl_letter('c'), Some(0x03));
        assert_eq!(encode_ctrl_letter('z'), Some(0x1a));
        assert_eq!(encode_ctrl_letter('Q'), Some(0x11));
        assert_eq!(encode_ctrl_letter('1'), None);
    }

    #[test]
    fn key_event_uses_ctrl_letter_encoding() {
        assert_eq!(
            encode_key_event(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            Some(vec![0x03])
        );
        assert_eq!(
            encode_key_event(KeyEvent::new(
                KeyCode::Char('Q'),
                KeyModifiers::CONTROL | KeyModifiers::SHIFT
            )),
            Some(vec![0x11])
        );
    }
}
