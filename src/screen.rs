pub mod conversation;
pub mod search;
pub mod settings;

pub use conversation::Conversation;
pub use search::Search;
pub use settings::Settings;

use iced::widget::horizontal_space;
use iced::Element;

pub enum Screen {
    Loading,
    Search(Search),
    Conversation(Conversation),
    Settings(Settings),
}

pub fn loading<'a, Message: 'a>() -> Element<'a, Message> {
    horizontal_space().into()
}
