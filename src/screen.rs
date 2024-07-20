pub mod boot;
pub mod conversation;
pub mod search;

pub use boot::Boot;
pub use conversation::Conversation;
pub use search::Search;

use iced::widget::horizontal_space;
use iced::Element;

pub enum Screen {
    Loading,
    Search(Search),
    Boot(Boot),
    Conversation(Conversation),
}

pub fn loading<'a, Message: 'a>() -> Element<'a, Message> {
    horizontal_space().into()
}
