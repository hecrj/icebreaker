use crate::assistant::{Reasoning, Reply};
use crate::chat;
use crate::plan;
use crate::web;
use crate::{Chat, Plan};

use decoder::decode::{sequence, value};
use decoder::{Decoder, Error, Result, Value};

pub fn chat(value: Value) -> Result<Chat> {
    let mut chat = value.into_map()?;

    Ok(Chat {
        id: chat.required("id")?,
        file: chat.required("file")?,
        title: chat.required("title")?,
        history: chat.required_with("history", sequence(item))?,
    })
}

fn item(value: Value) -> Result<chat::Item> {
    let mut item = value.into_map()?;

    let type_ = item.required::<String>("type")?;

    let item = match type_.as_str() {
        "user" => chat::Item::User(item.required("message")?),
        "reply" => chat::Item::Reply(reply(item.into_value())?),
        "plan" => chat::Item::Plan(plan(item.into_value())?),
        _ => {
            return Err(Error::custom(format!("invalid chat item: {type_}")));
        }
    };

    Ok(item)
}

fn reply(value: Value) -> Result<Reply> {
    let mut reply = value.into_map()?;

    Ok(Reply {
        reasoning: reply.optional_with("reasoning", reasoning)?,
        content: reply.required("content")?,
        last_token: None,
    })
}

fn reasoning(value: Value) -> Result<Reasoning> {
    let mut reasoning = value.into_map()?;

    Ok(Reasoning {
        content: reasoning.required("content")?,
        duration: reasoning.required("duration")?,
    })
}

fn plan(value: Value) -> Result<Plan> {
    let mut plan = value.into_map()?;

    Ok(Plan {
        reasoning: plan.optional_with("reasoning", reasoning)?,
        steps: plan.required_with("steps", sequence(step))?,
        outcomes: plan.required_with("outcomes", sequence(outcome))?,
    })
}

fn step(value: Value) -> Result<plan::Step> {
    let mut step = value.into_map()?;

    Ok(plan::Step {
        evidence: step.required("evidence")?,
        description: step.required("description")?,
        function: step.required("function")?,
        inputs: step.required("inputs")?,
    })
}

fn outcome(outcome: Value) -> Result<plan::Outcome> {
    let mut outcome = outcome.into_map()?;

    let type_ = outcome.required::<String>("type")?;

    let outcome = match type_.as_str() {
        "search" => plan::Outcome::Search(status(sequence(value), outcome.into_value())?),
        "scrape_text" => {
            plan::Outcome::ScrapeText(status(sequence(web_summary), outcome.into_value())?)
        }
        "answer" => plan::Outcome::Answer(status(reply, outcome.into_value())?),
        _ => {
            return Err(Error::custom(format!("invalid plan outcome: {type_}")));
        }
    };

    Ok(outcome)
}

fn status<T>(decoder: impl Decoder<Output = T>, value: Value) -> Result<plan::Status<T>> {
    let mut status = value.into_map()?;

    let type_ = status.required::<String>("status")?;

    let status = match type_.as_str() {
        "active" => plan::Status::Active(status.required_with("output", decoder)?),
        "done" => plan::Status::Done(status.required_with("output", decoder)?),
        "error" => plan::Status::Errored(status.required("output")?),
        _ => {
            return Err(Error::custom(format!("invalid status type: {type_}")));
        }
    };

    Ok(status)
}

fn web_summary(value: Value) -> Result<web::Summary> {
    let mut summary = value.into_map()?;

    Ok(web::Summary {
        url: summary.required("url")?,
        content: summary.required("content")?,
    })
}
