mod assistant;
mod icon;
mod screen;

use crate::screen::boot;
use crate::screen::conversation;
use crate::screen::search;
use crate::screen::Screen;

use iced::system;
use iced::{Element, Subscription, Task, Theme};

pub fn main() -> iced::Result {
    iced::application(Chat::title, Chat::update, Chat::view)
        .font(include_bytes!("../fonts/chat-icons.ttf"))
        .subscription(Chat::subscription)
        .theme(Chat::theme)
        .run_with(Chat::new)
}

struct Chat {
    screen: Screen,
    system: Option<system::Information>,
}

#[derive(Debug, Clone)]
enum Message {
    Search(search::Message),
    Boot(boot::Message),
    Conversation(conversation::Message),
    SystemFetched(system::Information),
}

impl Chat {
    pub fn new() -> (Self, Task<Message>) {
        let (search, task) = screen::Search::new();

        (
            Self {
                screen: Screen::Search(search),
                system: None,
            },
            Task::batch([
                system::fetch_information().map(Message::SystemFetched),
                task.map(Message::Search),
            ]),
        )
    }

    fn title(&self) -> String {
        match &self.screen {
            Screen::Search(search) => search.title(),
            Screen::Boot(boot) => boot.title(),
            Screen::Conversation(conversation) => conversation.title(),
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Search(message) => {
                if let Screen::Search(search) = &mut self.screen {
                    let (task, event) = search.update(message);

                    match event {
                        search::Event::None => {}
                        search::Event::ModelSelected(model) => {
                            self.screen =
                                Screen::Boot(screen::Boot::new(model, self.system.as_ref()));
                        }
                    };

                    task.map(Message::Search)
                } else {
                    Task::none()
                }
            }
            Message::Boot(message) => {
                if let Screen::Boot(search) = &mut self.screen {
                    let (task, event) = search.update(message);

                    let event_task = match event {
                        boot::Event::None => Task::none(),
                        boot::Event::Finished(assistant) => {
                            let (conversation, task) = screen::Conversation::new(assistant);

                            self.screen = Screen::Conversation(conversation);

                            task.map(Message::Conversation)
                        }
                        boot::Event::Aborted => {
                            let (search, task) = screen::Search::new();

                            self.screen = Screen::Search(search);

                            task.map(Message::Search)
                        }
                    };

                    Task::batch([task.map(Message::Boot), event_task])
                } else {
                    Task::none()
                }
            }
            Message::Conversation(message) => {
                if let Screen::Conversation(conversation) = &mut self.screen {
                    let task = conversation.update(message);

                    task.map(Message::Conversation)
                } else {
                    Task::none()
                }
            }
            Message::SystemFetched(system) => {
                self.system = Some(system);

                Task::none()
            }
        }
    }

    fn view(&self) -> Element<Message> {
        match &self.screen {
            Screen::Search(search) => search.view().map(Message::Search),
            Screen::Boot(boot) => boot.view().map(Message::Boot),
            Screen::Conversation(conversation) => conversation.view().map(Message::Conversation),
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        match &self.screen {
            Screen::Boot(boot) => boot.subscription().map(Message::Boot),
            Screen::Search(_) | Screen::Conversation(_) => Subscription::none(),
        }
    }

    fn theme(&self) -> Theme {
        Theme::TokyoNight
    }
}
