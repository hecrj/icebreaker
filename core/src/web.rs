use crate::assistant::Message;
use crate::{Assistant, Error, Url};

use sipper::{sipper, Sipper, Straw};
use std::sync::LazyLock;

pub struct Search {
    pub results: Vec<Url>,
}

#[derive(Debug, Clone)]
pub struct Summary {
    pub url: Url,
    pub content: String,
}

impl Summary {
    pub fn content(&self) -> &str {
        &self.content
    }
}

pub async fn search(query: &str) -> Result<Search, Error> {
    let search_results = CLIENT
        .get("https://html.duckduckgo.com/html/")
        .version(reqwest::Version::HTTP_2)
        .query(&[("q", query)])
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;

    let html = scraper::Html::parse_document(&search_results);
    let selector = scraper::Selector::parse(".result__a").unwrap();

    let results = html
        .select(&selector)
        .filter_map(|link| {
            let encoded = link.attr("href")?;

            if encoded.contains("ad_domain") {
                return None;
            }

            reqwest::Url::parse(&url::form_urlencoded::parse(encoded.as_bytes()).next()?.1).ok()
        })
        .take(5)
        .collect();

    Ok(Search { results })
}

pub fn summarize<'a>(
    assistant: &'a Assistant,
    query: &'a str,
    url: Url,
) -> impl Straw<Summary, Summary, Error> + 'a {
    sipper(move |sender| async move {
        let text = scrape(url.clone()).await?;

        let reply = assistant
            .reply(
                "You are a helpful assistant.",
                &[Message::User(dbg!(format!(
                    "```\n\
                    {text}\n\
                    ```\n\n\
                    Please, summarize the parts of the previous text that \
                    are relevant to the query: \"{query}\"."
                )))],
                &[],
            )
            .with(|(reply, _token)| Summary {
                url: url.clone(),
                content: reply.content,
            })
            .run(sender)
            .await?;

        Ok(Summary {
            url,
            content: reply.content,
        })
    })
}

async fn scrape(url: Url) -> Result<String, Error> {
    log::info!("Scraping text: {url}");

    let candidates = scraper::Selector::parse("p, a").unwrap();

    let html = CLIENT
        .get(url.clone())
        .version(reqwest::Version::HTTP_2)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;

    log::info!("-- HTML retrieved ({} chars)", html.len());
    log::trace!("{}", html);

    let html = scraper::Html::parse_document(&html);

    let lines = html
        .select(&candidates)
        .flat_map(|candidate| candidate.text())
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>();

    log::info!("-- Scraped {} lines of text", lines.len());

    Ok(lines.join("\n"))
}

static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
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
});
