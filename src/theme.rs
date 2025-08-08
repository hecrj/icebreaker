pub use iced::Theme;

use crate::core::settings;

pub fn to_data(theme: &Theme) -> settings::Theme {
    match theme {
        Theme::Light => settings::Theme::Light,
        Theme::Dark => settings::Theme::Dark,
        Theme::Dracula => settings::Theme::Dracula,
        Theme::Nord => settings::Theme::Nord,
        Theme::SolarizedLight => settings::Theme::SolarizedLight,
        Theme::SolarizedDark => settings::Theme::SolarizedDark,
        Theme::GruvboxLight => settings::Theme::GruvboxLight,
        Theme::GruvboxDark => settings::Theme::GruvboxDark,
        Theme::CatppuccinLatte => settings::Theme::CatppuccinLatte,
        Theme::CatppuccinFrappe => settings::Theme::CatppuccinFrappe,
        Theme::CatppuccinMacchiato => settings::Theme::CatppuccinMacchiato,
        Theme::CatppuccinMocha => settings::Theme::CatppuccinMocha,
        Theme::TokyoNight => settings::Theme::TokyoNight,
        Theme::TokyoNightStorm => settings::Theme::TokyoNightStorm,
        Theme::TokyoNightLight => settings::Theme::TokyoNightLight,
        Theme::KanagawaWave => settings::Theme::KanagawaWave,
        Theme::KanagawaDragon => settings::Theme::KanagawaDragon,
        Theme::KanagawaLotus => settings::Theme::KanagawaLotus,
        Theme::Moonfly => settings::Theme::Moonfly,
        Theme::Nightfly => settings::Theme::Nightfly,
        Theme::Oxocarbon => settings::Theme::Oxocarbon,
        Theme::Ferra => settings::Theme::Ferra,
        Theme::Custom(custom) => settings::Theme::Other(custom.to_string()),
    }
}

pub fn from_data(theme: &settings::Theme) -> Theme {
    match theme {
        settings::Theme::Light => Theme::Light,
        settings::Theme::Dark => Theme::Dark,
        settings::Theme::Dracula => Theme::Dracula,
        settings::Theme::Nord => Theme::Nord,
        settings::Theme::SolarizedLight => Theme::SolarizedLight,
        settings::Theme::SolarizedDark => Theme::SolarizedDark,
        settings::Theme::GruvboxLight => Theme::GruvboxLight,
        settings::Theme::GruvboxDark => Theme::GruvboxDark,
        settings::Theme::CatppuccinLatte => Theme::CatppuccinLatte,
        settings::Theme::CatppuccinFrappe => Theme::CatppuccinFrappe,
        settings::Theme::CatppuccinMacchiato => Theme::CatppuccinMacchiato,
        settings::Theme::CatppuccinMocha => Theme::CatppuccinMocha,
        settings::Theme::TokyoNight => Theme::TokyoNight,
        settings::Theme::TokyoNightStorm => Theme::TokyoNightStorm,
        settings::Theme::TokyoNightLight => Theme::TokyoNightLight,
        settings::Theme::KanagawaWave => Theme::KanagawaWave,
        settings::Theme::KanagawaDragon => Theme::KanagawaDragon,
        settings::Theme::KanagawaLotus => Theme::KanagawaLotus,
        settings::Theme::Moonfly => Theme::Moonfly,
        settings::Theme::Nightfly => Theme::Nightfly,
        settings::Theme::Oxocarbon => Theme::Oxocarbon,
        settings::Theme::Ferra => Theme::Ferra,
        settings::Theme::Other(_) => Theme::CatppuccinMocha,
    }
}
