use std::path::Path;
use std::sync::LazyLock;

pub fn config() -> &'static Path {
    PROJECT
        .as_ref()
        .map(directories::ProjectDirs::config_dir)
        .unwrap_or(Path::new("./config"))
}

pub fn data() -> &'static Path {
    PROJECT
        .as_ref()
        .map(directories::ProjectDirs::data_dir)
        .unwrap_or(Path::new("./data"))
}

static PROJECT: LazyLock<Option<directories::ProjectDirs>> =
    LazyLock::new(|| directories::ProjectDirs::from("rs.icebreaker", "", "icebreaker"));
