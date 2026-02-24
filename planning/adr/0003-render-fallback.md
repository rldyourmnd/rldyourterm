# ADR-0003: CPU/GPU Rendering Strategy and Fallback

## Status
Accepted

## Context
Цель проекта — скорость и стабильность. Один GPU-платформенный путь не должен приводить к падениям сессии.

## Decision
1. Поддерживаются три режима запуска:
   - `cpu`: принудительно CPU renderer;
   - `gpu`: принудительно GPU renderer;
   - `auto`: GPU попытка + fallback на CPU на любой устойчивый GPU failure.
2. В GPU mode используется `wgpu` как cross-platform графический слой.
3. Вводится `RenderModeController` в `services/render`.
4. Ошибки GPU:
   - маппируются в typed events (`DeviceLost`, `SurfaceError`, `SubmitError`, `SwapchainOutOfDate`);
   - триггерят bounded retry с обратным отсчётом;
   - после порога ошибок — безопасный переключатель на CPU.
5. Пороговые условия fallback:
   - больше 2 подряд critical GPU errors в 2–3 с;
   - 1 error `DeviceLost` без успешного recover.

## Rationale
- `wgpu` сам по себе cross-platform (`Vulkan`, `Metal`, `D3D12`, `OpenGL`) и API-устойчив к backend различиям.
- Автопереключение обязано быть user-invisible для долгих AI сессий.

## Consequences
- Добавляется дополнительный код наблюдаемости и деградации.
- Данные рендер-ошибок неизбежно сохраняются в trace-логах и метриках.
- Пользовательский видит уведомление и факт текущего режима.

## References
- Context7 `wgpu` для request_device/surface config/error callbacks.  
  Source: `https://docs.rs/wgpu/latest` (`/websites/rs_wgpu`, `/gfx-rs/wgpu`)
