use crate::assistant::{Reasoning, Reply};
use crate::chat;
use crate::plan;
use crate::web;
use crate::{Chat, Plan};

use decoder::encode::{map, optional, sequence, value, with};
use decoder::{Map, Value};

pub fn chat(chat: Chat) -> Map {
    map([
        ("id", value(chat.id)),
        ("file", value(chat.file)),
        ("title", value(chat.title)),
        ("history", sequence(item, chat.history)),
    ])
}

fn item(item: chat::Item) -> Map {
    let (type_, item) = match item {
        chat::Item::User(message) => ("user", map([("message", value(message))])),
        chat::Item::Reply(reply_) => ("reply", reply(reply_)),
        chat::Item::Plan(plan_) => ("plan", plan(plan_)),
    };

    item.tag("type", type_)
}

fn reply(reply: Reply) -> Map {
    map([
        ("reasoning", optional(reasoning, reply.reasoning)),
        ("content", value(reply.content)),
    ])
}

fn reasoning(reasoning: Reasoning) -> Map {
    map([
        ("content", value(reasoning.content)),
        ("duration", value(reasoning.duration)),
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
        ("evidence", value(step.evidence)),
        ("description", value(step.description)),
        ("function", value(step.function)),
        ("inputs", value(step.inputs)),
    ])
}

fn outcome(outcome: plan::Outcome) -> Map {
    let (type_, status) = match outcome {
        plan::Outcome::Search(status) => ("search", status_with(status, value)),
        plan::Outcome::ScrapeText(status) => (
            "scrape_text",
            status_with(status, with(sequence, web_summary)),
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
        plan::Status::Errored(error) => ("error", value(error)),
    };

    map([("status", value(type_)), ("output", output)])
}

fn web_summary(summary: web::Summary) -> Map {
    map([
        ("url", value(summary.url)),
        ("content", value(summary.content)),
    ])
}
