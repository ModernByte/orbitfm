//! Политика выбора GPU для окна стола (ADR-0059).
//!
//! wgpu умеет выбирать адаптер по подсказке `PowerPreference`, но подсказка —
//! не решение проекта: на гибридной машине окно может получить встроенное
//! устройство, а на машине без дискретной карты — отказаться от программного
//! рендера. Здесь правило закреплено кодом и печатается в лог, чтобы ответ на
//! вопрос «чем рисуется глобус» был проверяемым, а не предполагаемым.

use std::sync::Arc;

use egui_wgpu::{WgpuSetup, wgpu};

/// Порог «программного рендера» в ранжировании устройств.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Rank {
    Discrete,
    Integrated,
    Virtual,
    Other,
    Cpu,
}

impl Rank {
    fn of(device_type: wgpu::DeviceType) -> Self {
        match device_type {
            wgpu::DeviceType::DiscreteGpu => Self::Discrete,
            wgpu::DeviceType::IntegratedGpu => Self::Integrated,
            wgpu::DeviceType::VirtualGpu => Self::Virtual,
            // `Other` до `Cpu`: llvmpipe/lavapipe честно репортят `Cpu`, а
            // «прочее» устройство всё же может оказаться железом.
            wgpu::DeviceType::Other => Self::Other,
            wgpu::DeviceType::Cpu => Self::Cpu,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Discrete => "дискретная",
            Self::Integrated => "интегрированная",
            Self::Virtual => "виртуальная",
            Self::Other => "прочая",
            Self::Cpu => "ЦПУ",
        }
    }
}

/// Конфигурация wgpu для окна стола: своя политика выбора адаптера и задержка
/// показа в один кадр (ADR-0059 § 3).
pub fn wgpu_options() -> egui_wgpu::WgpuConfiguration {
    let mut options = egui_wgpu::WgpuConfiguration {
        surface: egui_wgpu::SurfaceConfig::LOW_LATENCY,
        ..Default::default()
    };

    // `WGPU_BACKEND` / `WGPU_POWER_PREF` — внешнее переключение пользователя:
    // своя политика его не отменяет (ADR-0059 § 1.5).
    let forced =
        wgpu::PowerPreference::from_env().is_some() || std::env::var_os("WGPU_BACKEND").is_some();
    if forced {
        tracing::info!(
            "orbitfm gpu: выбор адаптера оставлен wgpu (задан WGPU_BACKEND/WGPU_POWER_PREF)"
        );
        return options;
    }

    if let WgpuSetup::CreateNew(setup) = &mut options.wgpu_setup {
        setup.native_adapter_selector = Some(Arc::new(select_adapter));
    }
    options
}

/// Выбирает адаптер по правилу ADR-0059 § 1: совместимый с поверхностью, затем
/// самый «мощный» тип устройства. Порядок перечисления wgpu сохраняется как
/// тай-брейк, поэтому выбор стабилен между запусками.
fn select_adapter(
    adapters: &[wgpu::Adapter],
    surface: Option<&wgpu::Surface<'_>>,
) -> Result<wgpu::Adapter, String> {
    let mut decisions = Vec::with_capacity(adapters.len());
    let mut eligible: Vec<(Rank, usize)> = Vec::new();
    for (index, adapter) in adapters.iter().enumerate() {
        let info = adapter.get_info();
        let compatible = surface.is_none_or(|surface| adapter.is_surface_supported(surface));
        decisions.push(format!(
            "{:?} backend={:?} device_type={:?} surface={}",
            info.name,
            info.backend,
            info.device_type,
            if compatible { "да" } else { "нет" }
        ));
        if compatible {
            eligible.push((Rank::of(info.device_type), index));
        }
    }

    let Some((rank, index)) = eligible
        .iter()
        .min_by_key(|(rank, index)| (*rank, *index))
        .copied()
    else {
        tracing::warn!(
            "orbitfm gpu: нет адаптера, совместимого с поверхностью окна; рассмотрено [{}]",
            decisions.join(", ")
        );
        return Err("нет адаптера, совместимого с поверхностью окна".to_owned());
    };

    let info = adapters[index].get_info();
    tracing::info!(
        "orbitfm gpu: выбран {:?} ({}, backend={:?}) из [{}]",
        info.name,
        rank.label(),
        info.backend,
        decisions.join(", ")
    );
    Ok(adapters[index].clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ранжирование — единственная часть политики, проверяемая без GPU:
    /// дискретная раньше всех, программный рендер — последним.
    #[test]
    fn discrete_ranks_before_integrated_and_cpu() {
        let order = [
            wgpu::DeviceType::DiscreteGpu,
            wgpu::DeviceType::IntegratedGpu,
            wgpu::DeviceType::VirtualGpu,
            wgpu::DeviceType::Other,
            wgpu::DeviceType::Cpu,
        ];
        let ranks: Vec<Rank> = order.iter().map(|kind| Rank::of(*kind)).collect();
        assert_eq!(
            ranks,
            vec![
                Rank::Discrete,
                Rank::Integrated,
                Rank::Virtual,
                Rank::Other,
                Rank::Cpu
            ]
        );

        let mut sorted = ranks.clone();
        sorted.sort();
        assert_eq!(
            sorted, ranks,
            "ранг должен быть монотонным по силе устройства"
        );
    }

    /// Дефолт окна стола: один кадр в очереди и наша политика адаптера.
    #[test]
    fn desk_window_asks_for_low_latency_and_own_policy() {
        let options = wgpu_options();
        assert_eq!(
            options.surface.desired_maximum_frame_latency,
            Some(1),
            "ADR-0059 § 3: ввод отдаётся в кадр через один кадр, а не через два"
        );
        assert_eq!(options.surface.present_mode, wgpu::PresentMode::AutoVsync);
        // Без WGPU_BACKEND/WGPU_POWER_PREF политика проекта установлена.
        if std::env::var_os("WGPU_BACKEND").is_none() && wgpu::PowerPreference::from_env().is_none()
        {
            match &options.wgpu_setup {
                WgpuSetup::CreateNew(setup) => assert!(setup.native_adapter_selector.is_some()),
                WgpuSetup::Existing(_) => panic!("окно стола не использует чужой setup"),
            }
        }
    }
}
