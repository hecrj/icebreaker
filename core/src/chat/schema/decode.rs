use crate::assistant::{Reasoning, Reply};
use crate::chat;
use crate::model;
use crate::plan;
use crate::web;
use crate::{Chat, Plan, Url};

use decoder::decode::{duration, map, sequence, string};
use decoder::{Decoder, Error, Result, Value};

pub fn chat(value: Value) -> Result<Chat> {
    let mut chat = map(value)?;

    Ok(Chat {
        id: chat.required("id", chat::Id::decode)?,
        file: chat.required("file", model::File::decode)?,
        title: chat.optional("title", string)?,
        history: chat.required("history", sequence(item))?,
    })
}

fn item(value: Value) -> Result<chat::Item> {
    let mut item = map(value)?;

    let type_ = item.required("type", string)?;

    let item = match type_.as_str() {
        "user" => chat::Item::User(item.required("message", string)?),
        "reply" => chat::Item::Reply(reply(item.into_value())?),
        "plan" => chat::Item::Plan(plan(item.into_value())?),
        _ => {
            return Err(Error::custom(format!("invalid chat item: {type_}")));
        }
    };

    Ok(item)
}

fn reply(value: Value) -> Result<Reply> {
    let mut reply = map(value)?;

    Ok(Reply {
        reasoning: reply.optional("reasoning", reasoning)?,
        content: reply.required("content", string)?,
        last_token: None,
    })
}

fn reasoning(value: Value) -> Result<Reasoning> {
    let mut reasoning = map(value)?;

    Ok(Reasoning {
        content: reasoning.required("content", string)?,
        duration: reasoning.required("duration", duration)?,
    })
}

fn plan(value: Value) -> Result<Plan> {
    let mut plan = map(value)?;

    Ok(Plan {
        reasoning: plan.optional("reasoning", reasoning)?,
        steps: plan.required("steps", sequence(step))?,
        outcomes: plan.required("outcomes", sequence(outcome))?,
    })
}

fn step(value: Value) -> Result<plan::Step> {
    let mut step = map(value)?;

    Ok(plan::Step {
        evidence: step.required("evidence", string)?,
        description: step.required("description", string)?,
        function: step.required("function", string)?,
        inputs: step.required("inputs", sequence(string))?,
    })
}

fn outcome(outcome: Value) -> Result<plan::Outcome> {
    let mut outcome = map(outcome)?;

    let type_ = outcome.required("type", string)?;

    let outcome = match type_.as_str() {
        "search" => plan::Outcome::Search(status(sequence(url), outcome.into_value())?),
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
    let mut status = map(value)?;

    let type_ = status.required("status", string)?;

    let status = match type_.as_str() {
        "active" => plan::Status::Active(status.required("output", decoder)?),
        "done" => plan::Status::Done(status.required("output", decoder)?),
        "error" => plan::Status::Errored(status.required("output", string)?),
        _ => {
            return Err(Error::custom(format!("invalid status type: {type_}")));
        }
    };

    Ok(status)
}

fn web_summary(value: Value) -> Result<web::Summary> {
    let mut summary = map(value)?;

    Ok(web::Summary {
        url: summary.required("url", url)?,
        content: summary.required("content", string)?,
    })
}

fn url(value: Value) -> Result<Url> {
    let url = string(value)?;

    Url::parse(&url).map_err(Error::custom)
}
