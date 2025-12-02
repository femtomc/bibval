use super::{async_trait, Validator, ValidatorError};
use crate::entry::Entry;
use reqwest::Client;
use serde::Deserialize;

const ZENODO_API_BASE: &str = "https://zenodo.org/api";

pub struct ZenodoClient {
    client: Client,
}

impl ZenodoClient {
    pub fn new() -> Self {
        let client = Client::builder()
            .user_agent("bibval/0.1.0 (https://github.com/femtomc/bibval)")
            .build()
            .expect("Failed to create HTTP client");
        Self { client }
    }
}

impl Default for ZenodoClient {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    hits: Hits,
}

#[derive(Debug, Deserialize)]
struct Hits {
    hits: Vec<Record>,
}

#[derive(Debug, Deserialize)]
struct Record {
    id: Option<u64>,
    metadata: Metadata,
}

#[derive(Debug, Deserialize)]
struct Metadata {
    title: Option<String>,
    creators: Option<Vec<Creator>>,
    publication_date: Option<String>,
    doi: Option<String>,
    resource_type: Option<ResourceType>,
}

#[derive(Debug, Deserialize)]
struct Creator {
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ResourceType {
    #[serde(rename = "type")]
    type_name: Option<String>,
}

impl Record {
    fn to_entry(&self) -> Entry {
        let entry_type = self
            .metadata
            .resource_type
            .as_ref()
            .and_then(|rt| rt.type_name.as_ref())
            .map(|t| match t.as_str() {
                "software" => "software",
                "dataset" => "dataset",
                "publication" => "article",
                _ => "misc",
            })
            .unwrap_or("misc");

        let mut entry = Entry::new(
            self.id.map(|i| i.to_string()).unwrap_or_default(),
            entry_type.to_string(),
        );

        entry.title = self.metadata.title.clone();

        // Extract year from publication_date (format: "YYYY-MM-DD" or "YYYY")
        if let Some(date) = &self.metadata.publication_date {
            if let Some(year_str) = date.split('-').next() {
                entry.year = year_str.parse().ok();
            }
        }

        // Extract authors (Zenodo uses "Last, First" format)
        if let Some(creators) = &self.metadata.creators {
            entry.authors = creators
                .iter()
                .filter_map(|c| c.name.clone())
                .collect();
        }

        entry.doi = self.metadata.doi.clone();

        entry
    }
}

#[async_trait]
impl Validator for ZenodoClient {
    async fn search_by_doi(&self, doi: &str) -> Result<Option<Entry>, ValidatorError> {
        // Zenodo DOIs are typically 10.5281/zenodo.XXXXXXX
        let url = format!("{}/records?q=doi:\"{}\"&size=1", ZENODO_API_BASE, doi);

        let response = self.client.get(&url).send().await?;

        if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(ValidatorError::RateLimited);
        }

        if !response.status().is_success() {
            return Ok(None);
        }

        let search_response: SearchResponse = response.json().await.map_err(|e| {
            ValidatorError::ParseError(format!("Failed to parse Zenodo response: {}", e))
        })?;

        Ok(search_response.hits.hits.first().map(|r| r.to_entry()))
    }

    async fn search_by_title(&self, title: &str) -> Result<Vec<Entry>, ValidatorError> {
        let url = format!(
            "{}/records?q=title:\"{}\"&size=5",
            ZENODO_API_BASE,
            urlencoding::encode(title)
        );

        let response = self.client.get(&url).send().await?;

        if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(ValidatorError::RateLimited);
        }

        if !response.status().is_success() {
            return Ok(Vec::new());
        }

        let search_response: SearchResponse = response.json().await.map_err(|e| {
            ValidatorError::ParseError(format!("Failed to parse Zenodo response: {}", e))
        })?;

        let entries = search_response
            .hits
            .hits
            .iter()
            .map(|r| r.to_entry())
            .collect();

        Ok(entries)
    }

    fn name(&self) -> &'static str {
        "Zenodo"
    }
}
