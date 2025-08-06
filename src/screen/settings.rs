use std::{path::PathBuf, time::Duration};

use iced::{
    widget::{button, container, text, text_input, vertical_space, Column, Row}, Element, Font, Length::Fill, Task
};

use crate::icon::folder_open;

pub struct Settings {
    model_dir: String,
    model_change_temp: u64,
}

#[derive(Debug, Clone)]
pub enum Message {
    ManualTextChange(String),
    SaveConfigResult(Result<(), crate::config::Error>),
    TextChangeCooled,
    SetDirWithRFD,
    SetDirWithRFDResult(Option<PathBuf>),
}

pub enum Action {
    None,
    Run(Task<Message>),
}

impl Settings {
    pub fn new(config: &crate::config::Config) -> Self {
        Settings {
            model_dir: config.model_dir.to_string_lossy().to_string(),
            model_change_temp: 0,
        }
    }

    pub fn title(&self) -> String {
        "Settings - Icebreaker".to_owned()
    }

    pub fn update(&mut self, config: &mut crate::config::Config, message: Message) -> Action {
        match message {
            Message::ManualTextChange(text) => {
                self.model_dir = text;
                self.model_change_temp += 1;

                Action::Run(Task::perform(
                    tokio::time::sleep(Duration::from_secs(1)),
                    |_| Message::TextChangeCooled,
                ))
            }
            Message::TextChangeCooled => {
                self.model_change_temp = self.model_change_temp.saturating_sub(1);
                if self.model_change_temp == 0 {
                    let path = std::path::PathBuf::from(&self.model_dir);

                    if path.exists() && path.is_dir() {
                        config.model_dir = path;
                        let config = config.clone();
                        return Action::Run(Task::perform(
                            async move { config.save().await },
                            Message::SaveConfigResult,
                        ));
                    }
                }
                Action::None
            }
            Message::SaveConfigResult(result) => match result {
                Ok(_) => {
                    log::info!("Configuration saved successfully.");
                    Action::None
                }
                Err(e) => {
                    log::error!("Failed to save configuration: {}", e);
                    Action::None
                }
            },
            Message::SetDirWithRFD => Action::Run(Task::perform(
                async {
                    rfd::AsyncFileDialog::new()
                        .set_title("Select Model Directory")
                        .pick_folder()
                        .await
                        .map(|p| p.path().to_owned())
                },
                Message::SetDirWithRFDResult,
            )),
            Message::SetDirWithRFDResult(path) => {
                if let Some(p) = path {
                    if p.exists() && p.is_dir() {
                        log::info!("Selected directory: {}", p.display());
                        config.model_dir = p;
                        let config = config.clone();
                        Action::Run(Task::perform(
                            async move { config.save().await },
                            Message::SaveConfigResult,
                        ))
                    } else {
                        log::warn!("Selected path is not a valid directory: {}", p.display());
                        Action::None
                    }
                } else {
                    log::warn!("No directory selected.");
                    Action::None
                }
            }
        }
    }

    pub fn view<'a>(&'a self) -> Element<'a, Message> {
        let set_model_dir = Row::new()
            .push(text_input("model dir", &self.model_dir).on_input(Message::ManualTextChange))
            .push(button(folder_open()).on_press(Message::SetDirWithRFD));

        container(set_model_dir).into()
    }

    pub fn sidebar<'a>(&'a self) -> Element<'a, Message> {
        Column::new()
            .push(text("Settings").width(Fill).font(Font::MONOSPACE))
            .push(vertical_space())
            .into()
    }
}
