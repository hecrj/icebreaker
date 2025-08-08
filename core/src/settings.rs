use crate::directory;
use crate::model;
use crate::Error;

use decoder::{decode, encode, Value};
use tokio::fs;

use std::path::PathBuf;

#[derive(Debug, Clone, Default)]
pub struct Settings {
    pub library: model::Directory,
    pub theme: Theme,
}

impl Settings {
    pub fn fetch() -> Result<Self, Error> {
        use std::fs;

        let config = fs::read_to_string(Self::path())?;
        let config: Value = toml::from_str(&config)?;

        Ok(Self::decode(config)?)
    }

    pub async fn save(self) -> Result<(), Error> {
        let toml = toml::to_string_pretty(&self.encode())?;

        let path = Self::path();

        if let Some(directory) = path.parent() {
            fs::create_dir_all(directory).await?;
        }

        fs::write(path, toml).await?;

        Ok(())
    }

    fn decode(value: Value) -> decoder::Result<Self> {
        let mut settings = decode::map(value)?;

        let library = settings
            .optional("library", model::Directory::decode)?
            .unwrap_or_default();

        let theme = settings
            .optional("theme", Theme::decode)?
            .unwrap_or_default();

        Ok(Self { library, theme })
    }

    fn encode(&self) -> Value {
        encode::map([
            ("library", self.library.encode()),
            ("theme", self.theme.encode()),
        ])
        .into_value()
    }

    fn path() -> PathBuf {
        directory::config().join("settings.toml")
    }
}

#[derive(Debug, Clone, Default)]
pub enum Theme {
    Light,
    Dark,
    Dracula,
    Nord,
    SolarizedLight,
    SolarizedDark,
    GruvboxLight,
    GruvboxDark,
    CatppuccinLatte,
    CatppuccinFrappe,
    CatppuccinMacchiato,
    #[default]
    CatppuccinMocha,
    TokyoNight,
    TokyoNightStorm,
    TokyoNightLight,
    KanagawaWave,
    KanagawaDragon,
    KanagawaLotus,
    Moonfly,
    Nightfly,
    Oxocarbon,
    Ferra,
    Other(String),
}

impl Theme {
    fn decode(value: Value) -> decoder::Result<Self> {
        let slug = decode::string(value)?;

        // TODO: Sort by slug and perform a binary search
        Ok(Self::ALL
            .iter()
            .find(|theme| theme.slug() == slug)
            .cloned()
            .unwrap_or(Theme::Other(slug)))
    }

    fn encode(&self) -> Value {
        encode::string(self.slug())
    }

    fn slug(&self) -> &str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
            Self::Dracula => "dracula",
            Self::Nord => "nord",
            Self::SolarizedLight => "solarized_light",
            Self::SolarizedDark => "solarized_dark",
            Self::GruvboxLight => "gruvbox_light",
            Self::GruvboxDark => "gruvbox_dark",
            Self::CatppuccinLatte => "catppuccin_latte",
            Self::CatppuccinFrappe => "catppuccin_frappe",
            Self::CatppuccinMacchiato => "catppuccin_macchiato",
            Self::CatppuccinMocha => "catppuccin_mocha",
            Self::TokyoNight => "tokyo_night",
            Self::TokyoNightStorm => "tokyo_night_storm",
            Self::TokyoNightLight => "tokyo_night_light",
            Self::KanagawaWave => "kanagawa_wave",
            Self::KanagawaDragon => "kanagawa_dragon",
            Self::KanagawaLotus => "kanagawa_lotus",
            Self::Moonfly => "moonfly",
            Self::Nightfly => "nightfly",
            Self::Oxocarbon => "oxocarbon",
            Self::Ferra => "ferra",
            Self::Other(other) => other.as_str(),
        }
    }

    const ALL: &[Self] = &[
        Self::Light,
        Self::Dark,
        Self::Dracula,
        Self::Nord,
        Self::SolarizedLight,
        Self::SolarizedDark,
        Self::GruvboxLight,
        Self::GruvboxDark,
        Self::CatppuccinLatte,
        Self::CatppuccinFrappe,
        Self::CatppuccinMacchiato,
        Self::CatppuccinMocha,
        Self::TokyoNight,
        Self::TokyoNightStorm,
        Self::TokyoNightLight,
        Self::KanagawaWave,
        Self::KanagawaDragon,
        Self::KanagawaLotus,
        Self::Moonfly,
        Self::Nightfly,
        Self::Oxocarbon,
        Self::Ferra,
    ];
}
