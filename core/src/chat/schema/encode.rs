use crate::assistant::{Reasoning, Reply};
use crate::chat;
use crate::plan;
use crate::web;
use crate::{Chat, Plan, Url};

use decoder::encode::{duration, map, optional, sequence, string};
use decoder::{Map, Value};
use function::Binary;

pub fn chat(chat: Chat) -> Value {
    map([
        ("id", chat.id.encode()),
        ("file", chat.file.encode()),
        ("title", optional(string, chat.title)),
        ("history", sequence(item, chat.history)),
    ])
    .into()
}

fn item(item: chat::Item) -> Map {
    let (type_, item) = match item {
        chat::Item::User(message) => ("user", map([("message", string(message))])),
        chat::Item::Reply(reply_) => ("reply", reply(reply_)),
        chat::Item::Plan(plan_) => ("plan", plan(plan_)),
    };

    item.tag("type", type_)
}

fn reply(reply: Reply) -> Map {
    map([
        ("reasoning", optional(reasoning, reply.reasoning)),
        ("content", string(reply.content)),
    ])
}

fn reasoning(reasoning: Reasoning) -> Map {
    map([
        ("content", string(reasoning.content)),
        ("duration", duration(reasoning.duration)),
    ])
}

fn plan(plan: Plan) -> Map {
    map([
        ("reasoning", optional(reasoning, plan.reasoning)),
        ("steps", sequence(step, plan.steps)),
        ("outcomes", sequence(outcome, plan.outcomes)),
    ])
}

fn step(step: plan::Step) -> Map {
    map([
        ("evidence", string(step.evidence)),
        ("description", string(step.description)),
        ("function", string(step.function)),
        ("inputs", sequence(string, step.inputs)),
    ])
}

fn outcome(outcome: plan::Outcome) -> Map {
    let (type_, status) = match outcome {
        plan::Outcome::Search(status) => ("search", status_with(status, sequence.with(url))),
        plan::Outcome::ScrapeText(status) => (
            "scrape_text",
            status_with(status, sequence.with(web_summary)),
        ),
        plan::Outcome::Answer(status) => ("answer", status_with(status, reply)),
    };

    status.tag("type", type_)
}

fn status_with<T, V>(status: plan::Status<T>, encoder: impl Fn(T) -> V) -> Map
where
    V: Into<Value>,
{
    let (type_, output) = match status {
        plan::Status::Active(output) => ("active", encoder(output).into()),
        plan::Status::Done(output) => ("done", encoder(output).into()),
        plan::Status::Errored(error) => ("error", string(error)),
    };

    map([("status", string(type_)), ("output", output)])
}

fn web_summary(summary: web::Summary) -> Map {
    map([
        ("url", url(summary.url)),
        ("content", string(summary.content)),
    ])
}

fn url(url: Url) -> Value {
    string(url.to_string())
}
