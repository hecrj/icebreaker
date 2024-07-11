pub mod boot;
pub mod conversation;
pub mod search;

pub use boot::Boot;
pub use conversation::Conversation;
pub use search::Search;

pub enum Screen {
    Search(Search),
    Boot(Boot),
    Conversation(Conversation),
}
