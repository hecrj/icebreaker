pub mod conversation;
pub mod search;

pub use conversation::Conversation;
pub use search::Search;

use iced::widget::horizontal_space;
use iced::Element;

pub enum Screen {
    Loading,
    Search(Search),
    Conversation(Conversation),
}

pub fn loading<'a, Message: 'a>() -> Element<'a, Message> {
    horizontal_space().into()
}
