# Foundation API Contracts v1.0 (Strict Interface Spec)

## Дата: 2026-02-24

Документ задает строгие интерфейсы для слоя `foundation/api` и их ожидаемое поведение при интеграции с `core/services/features/ui` в v1.0.

## 1) Общие положения

- Все внешние зависимости (PTY, window system, GPU backend, clipboard, tracing sink и т.д.) подключаются только через `foundation/api` traits.
- Контракты должны быть минимальными, синхронными по API и с явной асинхронной моделью для потоков I/O/событий.
- Любая ошибка нормализуется в `foundation` domain errors и прокидывается в `services` как структурированные события.

## 2) Типы ошибок

### 2.1 RuntimeError

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RuntimeLayer {
    Core,
    FoundationPty,
    FoundationWindow,
    FoundationClipboard,
    FoundationRender,
    Services,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ErrorSeverity { Low, Medium, High, Fatal }

#[derive(Debug, Clone)]
pub struct RuntimeError {
    pub layer: RuntimeLayer,
    pub code: String,
    pub message: String,
    pub severity: ErrorSeverity,
    pub recoverable: bool,
    pub correlation_id: Option<String>,
    pub raw: Option<String>,
}
```

- `recoverable=true` означает допустимость retry/degrade.
- `recoverable=false` требует controlled stop или user decision path.

### 2.2 Error handling rules

- foundation never panics on runtime fault; it emits `RuntimeError`.
- services решает strategy: retry/degrade/controlled_stop.

## 3) foundation/api/common.rs

```rust
pub type Result<T> = std::result::Result<T, RuntimeError>;

#[derive(Debug, Clone)]
pub struct CorrelationId(pub String);

#[derive(Debug, Clone, Copy)]
pub struct ViewportSize {
    pub cols: u16,
    pub rows: u16,
    pub width_px: u16,
    pub height_px: u16,
}

#[derive(Debug, Clone, Copy)]
pub struct FontMetrics {
    pub cell_width: u16,
    pub cell_height: u16,
    pub ascender: i16,
    pub descender: i16,
}

#[derive(Debug, Clone)]
pub struct MonitorTiming {
    pub monitor_name: Option<String>,
    pub refresh_rate_millihz: Option<u32>,
}
```

## 4) foundation/api/pty.rs

### 4.1 Command/size contracts

```rust
#[derive(Debug, Clone)]
pub struct PtySpawnConfig {
    pub shell_command: String,
    pub args: Vec<String>,
    pub cwd: Option<String>,
    pub env: Vec<(String, String)>,
    pub cols: u16,
    pub rows: u16,
}

#[derive(Debug, Clone, Copy)]
pub struct PtySize {
    pub cols: u16,
    pub rows: u16,
    pub pixel_width: u16,
    pub pixel_height: u16,
}
```

### 4.2 IO primitives

```rust
pub trait PtyIo: Send + Sync {
    fn take_reader(&self) -> Result<Box<dyn std::io::Read + Send>>;
    fn take_writer(&self) -> Result<Box<dyn std::io::Write + Send>>;
    fn resize(&self, size: PtySize) -> Result<()>;
    fn kill(&self) -> Result<()>;
    fn wait(&self) -> Result<i32>;
    fn try_wait(&self) -> Result<Option<i32>>;
    fn close(&self) -> Result<()>;
}

pub trait PtyFactory: Send + Sync {
    fn spawn(&self, cfg: PtySpawnConfig) -> Result<Box<dyn PtyIo>>;
}
```

### 4.3 Behavioral guarantees

- `PtyIo` реализуется один раз на сессию.
- `take_writer()` должен поддерживать только один успешный захват (`single writer`).
- `resize` обязателен к вызову при каждом стабильном изменении viewport.
- `try_wait`/`wait` должны быть idempotent-safe после завершения процесса.

## 5) foundation/api/window.rs

### 5.1 Window lifecycle + event callback contract

```rust
#[derive(Debug, Clone)]
pub struct WindowConfig {
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub min_width: u32,
    pub min_height: u32,
    pub high_dpi: bool,
}

#[derive(Debug, Clone)]
pub enum WindowEvent {
    CloseRequested,
    Moved { x: i32, y: i32 },
    Resized { width: u32, height: u32, cols: u16, rows: u16 },
    ScaleFactorChanged { scale: f64 },
    DisplayRefreshChanged { refresh_rate_millihz: Option<u32>, monitor_name: Option<String> },
    RedrawRequested,
    Focused(bool),
    ModifierChanged { shift: bool, ctrl: bool, alt: bool, logo: bool },
    Keyboard { input: WindowInput },
    MouseWheel { delta_x: f32, delta_y: f32 },
    MouseMove { x: f64, y: f64 },
    MouseButton { button: u8, pressed: bool },
}

#[derive(Debug, Clone)]
pub enum WindowInput {
    Text { text: String },
    Key { key_code: u32, name: String, pressed: bool, repeat: bool },
}

pub trait WindowEventSink: Send + Sync {
    fn on_event(&self, event: WindowEvent);
}

pub trait WindowControl: Send + Sync {
    fn request_redraw(&self) -> Result<()>;
    fn set_title(&self, title: &str) -> Result<()>;
    fn current_monitor_timing(&self) -> Result<MonitorTiming>;
    fn clipboard_text(&self) -> Result<String>;
    fn set_clipboard_text(&self, text: &str) -> Result<()>;
    fn close(&self) -> Result<()>;
    fn poll_events(&self) -> Result<Vec<WindowEvent>>;
}

pub trait WindowFactory: Send + Sync {
    fn init(&self, config: WindowConfig, sink: Box<dyn WindowEventSink>) -> Result<Box<dyn WindowControl>>;
}
```

### 5.2 Behavioral guarantees

- События ввода должны идти в порядке доставки event-loop.
- `RedrawRequested` обязателен для отложенного/коалесцируемого рендера.
- `request_redraw` не должен блокировать event-loop.
- `DisplayRefreshChanged` должен эмититься при переносе окна между мониторами или при изменении monitor timing.
- `current_monitor_timing` не должен паниковать, даже если refresh-rate недоступен (`None` допускается).

## 6) foundation/api/diagnostics.rs

```rust
#[derive(Debug, Clone)]
pub enum EventKind {
    SessionStarted,
    SessionEnded,
    SessionError,
    PtyError,
    RenderModeTransition,
    DisplayRefreshChanged,
    RenderCadenceUpdated,
    SettingsApply,
    Resize,
    ResourceWarning,
}

#[derive(Debug, Clone)]
pub struct DiagnosticsEvent {
    pub id: String,
    pub kind: EventKind,
    pub correlation_id: Option<String>,
    pub layer: String,
    pub message: String,
    pub payload_json: Option<String>,
    pub timestamp_ms: u64,
}

pub trait DiagnosticsSink: Send + Sync {
    fn emit(&self, event: DiagnosticsEvent) -> Result<()>;
}

pub trait DiagnosticsConfig {
    fn is_enabled(&self) -> bool;
    fn is_debug_mode(&self) -> bool;
}
```

## 7) foundation/api/clipboard.rs

```rust
pub trait ClipboardAdapter: Send + Sync {
    fn set_text(&self, text: &str) -> Result<()>;
    fn get_text(&self) -> Result<Option<String>>;
    fn clear(&self) -> Result<()>;
}
```

## 8) services API usage mapping

### 8.1 services/session contract
- `PtyFactory::spawn` -> создание сессии.
- `WindowFactory::init` -> старт окна и подписка событий.
- `PtyIo::take_reader` -> отдельный поток/задача для чтения.
- ошибки -> `RuntimeError{recoverable,...}`.

### 8.2 services/settings contract
- UI commands from palette -> `SettingsPatch` в `services/settings`.
- любые изменения валидируются до применения в core/state.
- invalid -> транзакционный rollback и event emit with reason.

### 8.3 services/render pacing contract
- `WindowControl::current_monitor_timing` используется для monitor-driven cadence.
- При `WindowEvent::Moved`/`ScaleFactorChanged`/`DisplayRefreshChanged` cadence пересчитывается и применяется без restart.
- При недоступном refresh-rate сервис не падает: использует безопасный present path и эмитит диагностическое событие.

## 9) foundation implementation expectations by OS

- Linux/macOS v1.0: full adapters for `PtyFactory`, `WindowFactory`, `ClipboardAdapter`.
- Windows v1.0: skeleton-compatible implementations, parity feature-flagged for post-v1.

## 10) Рекомендуемая тест-матрица для contract layer

1) Single-writer invariant: повторный `take_writer` должен возвращать recoverable error.
2) resize: многократный resize storm должен приводить к последнему размеру без гонок.
3) close/kill/read: идемпотентность после завершения процесса.
4) window events: `RedrawRequested` после resize для визуальной консистенции.
5) monitor transfer: перенос 144Hz -> 60Hz и обратно приводит к `DisplayRefreshChanged` и `RenderCadenceUpdated` без падения.
6) diagnostics: `RenderModeTransition` при fallback всегда с correlation id.
