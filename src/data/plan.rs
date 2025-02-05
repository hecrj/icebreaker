use crate::data::assistant::{Assistant, Message, Reasoning, Reply, Token};
use crate::data::Error;

use futures::StreamExt;
use serde::Deserialize;
use sipper::Sender;
use url::Url;

use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct Plan {
    pub reasoning: Option<Reasoning>,
    pub steps: Vec<Step>,
    pub execution: Execution,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Step {
    pub evidence: String,
    pub description: String,
    pub function: String,
    pub inputs: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Execution {
    pub outcomes: Vec<Outcome>,
}

#[derive(Debug, Clone)]
pub enum Outcome {
    Search(Status<Vec<Url>>),
    ScrapeText(Status<Vec<String>>),
    Answer(Status<Reply>),
}

#[derive(Debug, Clone)]
pub enum Status<T> {
    Active(T),
    Done(T),
    Errored(String),
}

impl<T> Status<T> {
    pub fn result(&self) -> Result<&T, &str> {
        match self {
            Status::Active(value) | Status::Done(value) => Ok(value),
            Status::Errored(error) => Err(error),
        }
    }

    pub fn map<A>(self, f: impl FnOnce(T) -> A) -> Status<A> {
        match self {
            Status::Active(value) => Status::Active(f(value)),
            Status::Done(value) => Status::Done(f(value)),
            Status::Errored(error) => Status::Errored(error),
        }
    }

    pub fn as_ref(&self) -> Status<&T> {
        match self {
            Status::Active(value) => Status::Active(value),
            Status::Done(value) => Status::Done(value),
            Status::Errored(error) => Status::Errored(error.clone()),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Event {
    Designing(Reasoning),
    Designed(Plan),
    OutcomeAdded(Outcome),
    OutcomeChanged(Outcome),
    Understanding(Token),
}

impl Plan {
    pub(crate) async fn search(
        assistant: &Assistant,
        history: &[Message],
        sender: &mut Sender<Event>,
    ) -> Result<(), Error> {
        let Some(query) = history.iter().rev().find_map(|item| {
            if let Message::User(query) = item {
                Some(query)
            } else {
                None
            }
        }) else {
            return Ok(());
        };

        let plan = design(assistant, &history, sender).await?;
        let _ = sender.send(Event::Designed(plan.clone())).await;

        execute(assistant, &history, query, &plan, sender).await?;

        Ok(())
    }
}

async fn design<'a>(
    assistant: &Assistant,
    history: &[Message],
    sender: &mut Sender<Event>,
) -> Result<Plan, Error> {
    let reply = assistant
        .reply(
            BROWSE_PROMPT,
            history,
            &[],
            &mut sender.filter_map(|(reply, _token): (Reply, Token)| {
                reply.reasoning.map(Event::Designing)
            }),
        )
        .await?;

    let steps = reply
        .content
        .split("```")
        .skip(1)
        .next()
        .unwrap_or(&reply.content);

    let plan = steps.trim_start_matches("json").trim();

    Ok(Plan {
        steps: serde_json::from_str(plan)?,
        reasoning: reply.reasoning,
        execution: Execution::default(),
    })
}

async fn execute(
    assistant: &Assistant,
    history: &[Message],
    query: &str,
    plan: &Plan,
    sender: &mut Sender<Event>,
) -> Result<Execution, Error> {
    struct Process {
        outcomes: Vec<Outcome>,
        outputs: HashMap<String, Output>,
        sender: Sender<Event>,
    }

    impl Process {
        fn links(&self, inputs: &[String]) -> Vec<Url> {
            inputs
                .iter()
                .flat_map(|input| {
                    if input.starts_with('$') {
                        if let Some(Output::Links(links)) =
                            self.outputs.get(input.trim_start_matches('$').trim())
                        {
                            links.clone()
                        } else {
                            Vec::new()
                        }
                    } else {
                        Url::parse(input)
                            .ok()
                            .map(|url| vec![url])
                            .unwrap_or_default()
                    }
                })
                .collect()
        }

        fn text(&self, inputs: &[String]) -> Vec<String> {
            inputs
                .iter()
                .filter_map(|input| {
                    if input.starts_with('$') {
                        let evidence = input.trim_start_matches('$').trim();

                        if let Output::Text(text) = self.outputs.get(evidence)? {
                            Some(text)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
                .flatten()
                .map(|text| format!("```\n{text}\n```"))
                .collect()
        }

        async fn push(&mut self, outcome: Outcome) {
            self.outcomes.push(outcome.clone());
            self.sender.send(Event::OutcomeAdded(outcome)).await;
        }

        async fn start<T>(&mut self, outcome: impl Fn(Status<T>) -> Outcome)
        where
            T: Default,
        {
            self.push(outcome(Status::Active(T::default()))).await;
        }

        async fn update(&mut self, outcome: Outcome) {
            let _ = self.outcomes.pop();
            self.outcomes.push(outcome.clone());
            self.sender.send(Event::OutcomeChanged(outcome)).await;
        }

        async fn done(&mut self, name: &str) {
            let Some(outcome) = self.outcomes.pop() else {
                return;
            };

            let (output, outcome) = match outcome {
                Outcome::Search(Status::Active(links)) => (
                    Output::Links(links.clone()),
                    Outcome::Search(Status::Done(links)),
                ),
                Outcome::ScrapeText(Status::Active(text)) => (
                    Output::Text(text.clone()),
                    Outcome::ScrapeText(Status::Done(text)),
                ),
                Outcome::Answer(Status::Active(reply)) => {
                    (Output::Answer, Outcome::Answer(Status::Done(reply)))
                }
                _ => {
                    return;
                }
            };

            self.outputs.insert(name.to_owned(), output);
            self.outcomes.push(outcome.clone());

            self.sender
                .send(Event::OutcomeChanged(outcome.clone()))
                .await;
        }
    }

    let mut process = Process {
        outcomes: Vec::new(),
        outputs: HashMap::new(),
        sender: sender.clone(),
    };

    #[derive(Debug)]
    enum Output {
        Links(Vec<reqwest::Url>),
        Text(Vec<String>),
        Answer,
    }

    let client = client();

    for (i, step) in plan.steps.iter().enumerate() {
        println!("Running: {}", step.description);

        match step.function.as_str() {
            "search" => {
                let query = step.inputs.first().map(String::as_str).unwrap_or_default();

                log::info!("Searching on DuckDuckGo: {query}");
                process.start(Outcome::Search).await;

                let search_results = client
                    .get("https://html.duckduckgo.com/html/")
                    .version(reqwest::Version::HTTP_2)
                    .query(&[("q", query)])
                    .send()
                    .await?
                    .error_for_status()?
                    .text()
                    .await?;

                let links = {
                    let html = scraper::Html::parse_document(&search_results);
                    let selector = scraper::Selector::parse(".result__a").unwrap();

                    html.select(&selector)
                        .filter_map(|link| {
                            let encoded = link.attr("href")?;

                            if encoded.contains("ad_domain") {
                                return None;
                            }

                            reqwest::Url::parse(
                                &url::form_urlencoded::parse(encoded.as_bytes()).next()?.1,
                            )
                            .ok()
                        })
                        .take(1)
                        .collect()
                };

                log::info!("-- Found: {links:?}");

                process.update(Outcome::Search(Status::Active(links))).await;
                process.done(&step.evidence).await;
            }
            "scrape_text" => {
                let mut output = Vec::new();

                let links = process.links(&step.inputs);
                let candidates = scraper::Selector::parse("p, a").unwrap();

                process.start(Outcome::ScrapeText).await;

                for link in links {
                    log::info!("Scraping text: {link}");

                    let html = client
                        .get(link)
                        .version(reqwest::Version::HTTP_2)
                        .send()
                        .await?
                        .error_for_status()?
                        .text()
                        .await?;

                    log::info!("-- HTML retrieved ({} chars)", html.len());
                    log::trace!("{}", html);

                    let text = {
                        let html = scraper::Html::parse_document(&html);

                        let text = html
                            .select(&candidates)
                            .flat_map(|candidate| candidate.text())
                            .map(str::trim)
                            .filter(|text| !text.is_empty())
                            .collect::<Vec<_>>();

                        log::info!("-- Scraped {} lines of text", text.len());

                        text.join("\n")
                    };

                    output.push(text);
                    process
                        .update(Outcome::ScrapeText(Status::Active(output.clone())))
                        .await;
                }

                process.done(&step.evidence).await;
            }
            "answer" => {
                process.start(Outcome::Answer).await;

                let steps = plan
                    .steps
                    .iter()
                    .take(i)
                    .map(|step| format!("- {}", step.description))
                    .collect::<Vec<_>>()
                    .join("\n");

                let outputs = process.text(&step.inputs).join("\n\n");

                let query = [Message::System(format!(
                        "In order to figure out the user's request, you have already performed certain actions to \
                        gather information. Here is a summary of the steps executed so far:\n\
                        \n\
                        {steps}\n\n\
                        The relevant outputs of the actions considered relevant to the user request are provided next:\n\
                        {outputs}")),

                 Message::User(query.to_owned())];

                let mut reply = iced::stream::try_channel(1, |sender| async move {
                    let mut sender = Sender::new(sender);

                    let _reply = assistant
                        .reply("You are a helpful assistant.", history, &query, &mut sender)
                        .await?;

                    Ok::<_, Error>(())
                })
                .boxed();

                while let Some((reply, token)) = reply.next().await.transpose()? {
                    process.update(Outcome::Answer(Status::Active(reply))).await;
                    sender.send(Event::Understanding(token)).await;
                }

                process.done(&step.evidence).await;
            }
            _ => {}
        }
    }

    Ok(Execution {
        outcomes: process.outcomes,
    })
}

fn client() -> reqwest::Client {
    let headers = reqwest::header::HeaderMap::from_iter([
        (
            reqwest::header::USER_AGENT,
            reqwest::header::HeaderValue::from_static(
                "Mozilla/5.0 (X11; Linux x86_64) \
                            AppleWebKit/537.36 (KHTML, like Gecko) \
                            Chrome/132.0.0.0 Safari/537.36",
            ),
        ),
        (
            reqwest::header::ACCEPT,
            reqwest::header::HeaderValue::from_static("*/*"),
        ),
    ]);

    reqwest::Client::builder()
        .use_rustls_tls()
        .default_headers(headers)
        .build()
        .expect("Build reqwest client")
}

const BROWSE_PROMPT: &str = r#"Please construct a systematic plan to generate an optimal response to the user instruction, utilizing a set of provided actions. Each step will correspond to an evidence value, which will be the output of one of the available actions given an input string.

Here are the tools available to be called:

- search: Search for information using the Google search engine. This action is helpful in locating a suitable list of sites that may contain the answer to the user's query. It does not directly answer the question but finds a list of sites that might have the answer.
- scrape_text: Load one or more websites from the input string, where input is one or more links, and produces plain text output containing the content of the links.
- answer: Answer a question by reasoning from evidence obtained with previous actions.

The output should be in JSON:

```json
[
    {
        "evidence": "search_0",
        "description": "Search how to cook an omelette",
        "function": "search",
        "inputs": ["how to cook an omelette best recipe"]
    },
    {
        "evidence": "scrape_0",
        "description": "Scrape websites obtained from the previous search",
        "function": "scrape_text",
        "inputs": ["$search_0"]
    },
    ...
    {
        "evidence": "final_answer",
        "description": "Understand the scraped text and provide a final answer.",
        "function": "answer",
        "inputs": ["$scrape_0"] // You can provide multiple inputs for the answer action
    }
]
```
Reply only with the plan in the given format. Provide the whole plan with all the steps. Say nothing else."#;
