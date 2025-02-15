use crate::assistant::{Assistant, Message, Reasoning, Reply};
use crate::web;
use crate::Error;

use serde::Deserialize;
use sipper::{sipper, Sender, Sipper, Straw};
use url::Url;

use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone, Default)]
pub struct Plan {
    pub reasoning: Option<Reasoning>,
    pub steps: Vec<Step>,
    pub outcomes: Vec<Outcome>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Step {
    pub evidence: String,
    pub description: String,
    pub function: String,
    pub inputs: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum Outcome {
    Search(Status<Vec<Url>>),
    ScrapeText(Status<Vec<web::Summary>>),
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
}

impl Plan {
    pub fn search<'a>(
        assistant: &'a Assistant,
        history: &'a [Message],
    ) -> impl Straw<(), Event, Error> + 'a {
        sipper(move |mut progress| async move {
            let Some(query) = history.iter().rev().find_map(|item| {
                if let Message::User(query) = item {
                    Some(query)
                } else {
                    None
                }
            }) else {
                return Ok(());
            };

            let plan = {
                let mut attempt = 0;

                loop {
                    log::info!("Designing plan ({attempt})...");

                    match design(assistant, history).run(&progress).await {
                        Err(error) if attempt < 3 => {
                            log::warn!("Plan design failed: {error}");
                        }
                        result => break result?,
                    }

                    attempt += 1;
                }
            };

            progress.send(Event::Designed(plan.clone())).await;

            execute(assistant, history, query, &plan)
                .run(progress)
                .await?;

            Ok(())
        })
    }

    pub fn answers(&self) -> impl Iterator<Item = &Reply> {
        self.outcomes.iter().filter_map(|outcome| match outcome {
            Outcome::Answer(Status::Done(reply)) => Some(reply),
            _ => None,
        })
    }
}

fn design<'a>(
    assistant: &'a Assistant,
    history: &'a [Message],
) -> impl Straw<Plan, Event, Error> + 'a {
    sipper(move |progress| async move {
        let reply = assistant
            .reply(
                "You are a helpful assistant.",
                history,
                &[Message::System(BROWSE_PROMPT.to_owned())],
            )
            .filter_with(|(reply, _token)| reply.reasoning.map(Event::Designing))
            .run(progress)
            .await?;

        let steps = reply
            .content
            .split("```")
            .skip(1)
            .next()
            .unwrap_or(&reply.content);

        let plan = steps.trim_start_matches("json").trim();

        log::info!("Plan designed:\n{plan}");

        Ok(Plan {
            reasoning: reply.reasoning,
            steps: serde_json::from_str(plan)?,
            outcomes: Vec::new(),
        })
    })
}

fn execute<'a>(
    assistant: &'a Assistant,
    history: &'a [Message],
    query: &'a str,
    plan: &'a Plan,
) -> impl Straw<Vec<Outcome>, Event, Error> + 'a {
    struct Process {
        outcomes: Vec<Outcome>,
        outputs: HashMap<String, Output>,
        sender: Sender<Event>,
    }

    #[derive(Debug)]
    enum Output {
        Links(Vec<reqwest::Url>),
        Text(Vec<String>),
        Answer,
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
                Outcome::ScrapeText(Status::Active(summaries)) => (
                    Output::Text(
                        summaries
                            .iter()
                            .map(web::Summary::content)
                            .map(str::to_owned)
                            .collect(),
                    ),
                    Outcome::ScrapeText(Status::Done(summaries)),
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

    sipper(move |sender| async move {
        let mut process = Process {
            outcomes: Vec::new(),
            outputs: HashMap::new(),
            sender: sender.clone(),
        };

        for (i, step) in plan.steps.iter().enumerate() {
            println!("Running: {}", step.description);

            match step.function.as_str() {
                "search" => {
                    let query = step.inputs.first().map(String::as_str).unwrap_or_default();

                    log::info!("Searching on DuckDuckGo: {query}");
                    process.start(Outcome::Search).await;

                    let search = web::search(query).await?;
                    log::info!("-- Found: {results:?}", results = search.results);

                    process
                        .update(Outcome::Search(Status::Active(search.results)))
                        .await;

                    process.done(&step.evidence).await;
                }
                "scrape_text" => {
                    use futures::stream::FuturesUnordered;
                    use futures::{FutureExt, StreamExt};

                    let mut output = BTreeMap::new();
                    let mut order = Vec::new();

                    let links = process.links(&step.inputs);

                    process.start(Outcome::ScrapeText).await;

                    let mut scrape = sipper(move |sender| {
                        links
                            .iter()
                            .cloned()
                            .map(|link| web::summarize(assistant, query, link))
                            .enumerate()
                            .map(|(i, scrape)| {
                                scrape
                                    .with(move |progress| (i, progress))
                                    .run(&sender)
                                    .map(move |result| (i, result))
                            })
                            .collect::<FuturesUnordered<_>>()
                            .collect::<Vec<_>>()
                    })
                    .pin();

                    while let Some((i, summary)) = scrape.sip().await {
                        let _ = output.insert(i, summary);

                        if !order.contains(&i) {
                            order.push(i);
                        }

                        process
                            .update(Outcome::ScrapeText(Status::Active(
                                order
                                    .iter()
                                    .filter_map(|i| output.get(i))
                                    .cloned()
                                    .collect(),
                            )))
                            .await;
                    }

                    let summaries = scrape.await;

                    for (i, summary) in summaries {
                        match summary {
                            Ok(summary) => {
                                let _ = output.insert(i, summary);
                            }
                            Err(error) => {
                                log::error!("Scraping failed: {error}");
                            }
                        }
                    }

                    process
                        .update(Outcome::ScrapeText(Status::Active(
                            output.values().cloned().collect(),
                        )))
                        .await;

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

                    let query = [
                        Message::System(format!(
                            "In order to figure out the user's request, you have already \
                        performed certain actions to gather information. Here is a \
                        summary of the steps executed so far:\n\
                        \n\
                        {steps}\n\n\
                        The outputs of the actions considered relevant to the user request \
                        are provided next:\n\
                        {outputs}\n\
                        Analyze the outputs carefully before replying to the user."
                        )),
                        Message::User(query.to_owned()),
                    ];

                    let mut reply = assistant
                        .reply("You are a helpful assistant.", history, &query)
                        .pin();

                    while let Some((reply, _token)) = reply.sip().await {
                        process.update(Outcome::Answer(Status::Active(reply))).await;
                    }

                    process.done(&step.evidence).await;
                }
                _ => {}
            }
        }

        Ok(process.outcomes)
    })
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
Reply only with the plan in JSON."#;
