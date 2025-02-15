use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::tree::{self, Tree};
use iced::advanced::{self, Clipboard, Shell, Widget};
use iced::alignment;
use iced::mouse;
use iced::time::{Duration, Instant};
use iced::widget::text;
use iced::window;
use iced::{Center, Color, Element, Event, Length, Pixels, Rectangle, Size};

#[derive(Debug)]
pub struct DiffusedText<'a, Theme, Renderer>
where
    Theme: text::Catalog,
    Renderer: advanced::text::Renderer,
{
    fragment: text::Fragment<'a>,
    size: Option<Pixels>,
    line_height: text::LineHeight,
    width: Length,
    height: Length,
    align_x: alignment::Horizontal,
    align_y: alignment::Vertical,
    font: Option<Renderer::Font>,
    shaping: text::Shaping,
    class: Theme::Class<'a>,
    duration: Duration,
    tick_rate: u64,
}

impl<'a, Theme, Renderer> DiffusedText<'a, Theme, Renderer>
where
    Theme: text::Catalog,
    Renderer: advanced::text::Renderer,
{
    pub fn new(fragment: impl text::IntoFragment<'a>) -> Self {
        Self {
            fragment: fragment.into_fragment(),
            size: None,
            line_height: text::LineHeight::default(),
            font: None,
            width: Length::Shrink,
            height: Length::Shrink,
            align_x: alignment::Horizontal::Left,
            align_y: alignment::Vertical::Top,
            shaping: text::Shaping::Basic,
            class: Theme::default(),
            duration: Duration::from_millis(400),
            tick_rate: 50,
        }
    }

    pub fn size(mut self, size: impl Into<Pixels>) -> Self {
        self.size = Some(size.into());
        self
    }

    pub fn line_height(mut self, line_height: impl Into<text::LineHeight>) -> Self {
        self.line_height = line_height.into();
        self
    }

    pub fn font(mut self, font: impl Into<Renderer::Font>) -> Self {
        self.font = Some(font.into());
        self
    }

    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.height = height.into();
        self
    }

    pub fn align_x(mut self, alignment: impl Into<alignment::Horizontal>) -> Self {
        self.align_x = alignment.into();
        self
    }

    pub fn align_y(mut self, alignment: impl Into<alignment::Vertical>) -> Self {
        self.align_y = alignment.into();
        self
    }

    pub fn center(self) -> Self {
        self.align_x(Center).align_y(Center)
    }

    pub fn shaping(mut self, shaping: text::Shaping) -> Self {
        self.shaping = shaping;
        self
    }

    #[must_use]
    pub fn style(mut self, style: impl Fn(&Theme) -> text::Style + 'a) -> Self
    where
        Theme::Class<'a>: From<text::StyleFn<'a, Theme>>,
    {
        self.class = (Box::new(style) as text::StyleFn<'a, Theme>).into();
        self
    }

    pub fn color(self, color: impl Into<Color>) -> Self
    where
        Theme::Class<'a>: From<text::StyleFn<'a, Theme>>,
    {
        self.color_maybe(Some(color))
    }

    pub fn color_maybe(self, color: Option<impl Into<Color>>) -> Self
    where
        Theme::Class<'a>: From<text::StyleFn<'a, Theme>>,
    {
        let color = color.map(Into::into);

        self.style(move |_theme| text::Style { color })
    }

    pub fn duration(mut self, duration: impl Into<Duration>) -> Self {
        self.duration = duration.into();
        self
    }

    pub fn tick_rate(mut self, tick_rate: impl Into<Duration>) -> Self {
        self.tick_rate = tick_rate.into().as_millis() as u64;
        self
    }

    pub fn fast(mut self) -> Self {
        self.duration = self.duration / 2;
        self.tick_rate = self.tick_rate / 2;
        self
    }
}

/// The internal state of a [`Text`] widget.
#[derive(Debug)]
pub struct State<P: advanced::text::Paragraph> {
    content: String,
    internal: text::State<P>,
    animation: Animation,
}

#[derive(Debug)]
enum Animation {
    Ticking {
        fragment: String,
        ticks: u64,
        next_redraw: Instant,
    },
    Done,
}

impl<'a, Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for DiffusedText<'a, Theme, Renderer>
where
    Theme: text::Catalog,
    Renderer: advanced::text::Renderer,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State<Renderer::Paragraph>>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State {
            content: String::new(),
            internal: text::State::<Renderer::Paragraph>::default(),
            animation: Animation::Ticking {
                fragment: String::new(),
                ticks: 0,
                next_redraw: Instant::now(),
            },
        })
    }

    fn size(&self) -> Size<Length> {
        Size {
            width: self.width,
            height: self.height,
        }
    }

    fn layout(
        &self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let state = &mut tree.state.downcast_mut::<State<Renderer::Paragraph>>();

        if state.content != self.fragment {
            state.content = self.fragment.clone().into_owned();

            state.animation = Animation::Ticking {
                fragment: String::from("-"),
                ticks: 0,
                next_redraw: Instant::now(),
            };
        }

        let fragment = match &state.animation {
            Animation::Ticking { fragment, .. } => fragment,
            Animation::Done => self.fragment.as_ref(),
        };

        text::layout(
            &mut state.internal,
            renderer,
            limits,
            self.width,
            self.height,
            fragment,
            self.line_height,
            self.size,
            self.font,
            self.align_x,
            self.align_y,
            self.shaping,
            text::Wrapping::default(),
        )
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        defaults: &renderer::Style,
        layout: Layout<'_>,
        _cursor_position: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State<Renderer::Paragraph>>();
        let style = theme.style(&self.class);

        text::draw(
            renderer,
            defaults,
            layout,
            state.internal.0.raw(),
            style,
            viewport,
        );
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        use rand::Rng;

        if layout.bounds().intersection(viewport).is_none() {
            return;
        }

        match event {
            Event::Window(window::Event::RedrawRequested(now)) => {
                let state = tree.state.downcast_mut::<State<Renderer::Paragraph>>();

                match &mut state.animation {
                    Animation::Ticking {
                        fragment,
                        next_redraw,
                        ticks,
                    } => {
                        if *next_redraw <= *now {
                            *ticks += 1;

                            let mut rng = rand::rng();
                            let progress = (self.fragment.len() as f32
                                / self.duration.as_millis() as f32
                                * (*ticks * self.tick_rate) as f32)
                                as usize;

                            if progress >= self.fragment.len() {
                                state.animation = Animation::Done;
                                shell.invalidate_layout();

                                return;
                            }

                            *fragment = self
                                .fragment
                                .chars()
                                .take(progress as usize)
                                .chain(self.fragment.chars().skip(progress as usize).map(|c| {
                                    if c.is_whitespace() || c == '-' {
                                        c
                                    } else {
                                        rng.random_range('a'..'z')
                                    }
                                }))
                                .collect::<String>();

                            *next_redraw = *now + Duration::from_millis(self.tick_rate);

                            shell.invalidate_layout();
                        }

                        shell.request_redraw_at(*next_redraw);
                    }
                    Animation::Done => {}
                }
            }
            _ => {}
        }
    }
}

impl<'a, Message, Theme, Renderer> From<DiffusedText<'a, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Theme: text::Catalog + 'a,
    Renderer: advanced::text::Renderer + 'a,
{
    fn from(text: DiffusedText<'a, Theme, Renderer>) -> Element<'a, Message, Theme, Renderer> {
        Element::new(text)
    }
}
