use crate::entry::Entry;
use biblatex::{Bibliography, ChunksExt};
use std::fs;
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ParseError {
    #[error("Failed to read file: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Failed to parse BibTeX: {0}")]
    ParseError(String),
}

/// Parse a .bib file and return normalized entries
pub fn parse_bib_file(path: &Path) -> Result<Vec<Entry>, ParseError> {
    let content = fs::read_to_string(path)?;
    parse_bib_string(&content)
}

/// Parse a BibTeX string and return normalized entries
pub fn parse_bib_string(content: &str) -> Result<Vec<Entry>, ParseError> {
    let bibliography =
        Bibliography::parse(content).map_err(|e| ParseError::ParseError(e.to_string()))?;

    let mut entries = Vec::new();

    for bib_entry in bibliography.into_iter() {
        let key = bib_entry.key.clone();
        let entry_type = format!("{:?}", bib_entry.entry_type).to_lowercase();

        let mut entry = Entry::new(key, entry_type);

        // Extract title
        if let Ok(title_chunks) = bib_entry.title() {
            entry.title = Some(title_chunks.format_verbatim());
        }

        // Extract authors
        if let Ok(authors) = bib_entry.author() {
            entry.authors = authors
                .iter()
                .map(|person| {
                    let mut parts = Vec::new();
                    if !person.given_name.is_empty() {
                        parts.push(person.given_name.as_str());
                    }
                    if !person.prefix.is_empty() {
                        parts.push(person.prefix.as_str());
                    }
                    parts.push(person.name.as_str());
                    if !person.suffix.is_empty() {
                        parts.push(person.suffix.as_str());
                    }
                    parts.join(" ")
                })
                .collect();
        }

        // Extract year - use the get method to access raw field
        if let Some(year_chunks) = bib_entry.get("year") {
            let year_str = year_chunks.format_verbatim();
            entry.year = year_str.trim().parse().ok();
        } else if let Ok(date) = bib_entry.date() {
            // Try to extract year from date if year field is not present
            // date() returns a PermissiveType<Date>
            let date_str = format!("{:?}", date);
            // Try to find a 4-digit year in the string
            if let Some(year) = extract_year_from_string(&date_str) {
                entry.year = Some(year);
            }
        }

        // Extract venue (journal or booktitle)
        if let Ok(journal) = bib_entry.journal() {
            entry.venue = Some(journal.format_verbatim());
        } else if let Ok(booktitle) = bib_entry.book_title() {
            entry.venue = Some(booktitle.format_verbatim());
        }

        // Extract DOI
        if let Ok(doi_str) = bib_entry.doi() {
            entry.doi = Some(doi_str);
        }

        // Extract arXiv ID from eprint field
        if let Ok(eprint_str) = bib_entry.eprint() {
            if is_arxiv_id(&eprint_str) {
                entry.arxiv_id = Some(eprint_str);
            }
        }

        // Extract URL
        if let Ok(url_str) = bib_entry.url() {
            entry.url = Some(url_str.clone());

            // Try to extract arXiv ID from URL if not already set
            if entry.arxiv_id.is_none() {
                if let Some(arxiv_id) = extract_arxiv_from_url(&url_str) {
                    entry.arxiv_id = Some(arxiv_id);
                }
            }

            // Try to extract DOI from URL if not already set
            if entry.doi.is_none() {
                if let Some(doi) = extract_doi_from_url(&url_str) {
                    entry.doi = Some(doi);
                }
            }
        }

        entries.push(entry);
    }

    Ok(entries)
}

/// Extract a 4-digit year from a string
fn extract_year_from_string(s: &str) -> Option<i32> {
    // Find a 4-digit sequence that looks like a year (1900-2099)
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c.is_ascii_digit() {
            let mut num = c.to_string();
            while let Some(&next) = chars.peek() {
                if next.is_ascii_digit() && num.len() < 4 {
                    num.push(chars.next().unwrap());
                } else {
                    break;
                }
            }
            if num.len() == 4 {
                if let Ok(year) = num.parse::<i32>() {
                    if (1900..=2099).contains(&year) {
                        return Some(year);
                    }
                }
            }
        }
    }
    None
}

/// Check if a string looks like an arXiv ID
fn is_arxiv_id(s: &str) -> bool {
    // Old format: hep-th/9901001
    // New format: 2301.12345 or 2301.12345v1
    let s = s.trim();
    if s.contains('/') {
        // Old format: category/YYMMNNN
        let parts: Vec<&str> = s.split('/').collect();
        parts.len() == 2 && parts[1].chars().all(|c| c.is_ascii_digit())
    } else {
        // New format: YYMM.NNNNN
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() != 2 {
            return false;
        }
        let base = parts[1].split('v').next().unwrap_or("");
        parts[0].len() == 4
            && parts[0].chars().all(|c| c.is_ascii_digit())
            && base.chars().all(|c| c.is_ascii_digit())
    }
}

/// Extract arXiv ID from a URL
fn extract_arxiv_from_url(url: &str) -> Option<String> {
    // https://arxiv.org/abs/2301.12345
    // https://arxiv.org/pdf/2301.12345.pdf
    if url.contains("arxiv.org") {
        let patterns = ["/abs/", "/pdf/"];
        for pattern in patterns {
            if let Some(idx) = url.find(pattern) {
                let start = idx + pattern.len();
                let rest = &url[start..];
                let id = rest
                    .split(|c: char| !c.is_alphanumeric() && c != '.' && c != '/' && c != 'v')
                    .next()?;
                // Remove .pdf suffix if present
                let id = id.trim_end_matches(".pdf");
                if is_arxiv_id(id) {
                    return Some(id.to_string());
                }
            }
        }
    }
    None
}

/// Extract DOI from a URL
fn extract_doi_from_url(url: &str) -> Option<String> {
    // https://doi.org/10.1234/example
    // https://dx.doi.org/10.1234/example
    if url.contains("doi.org/") {
        let idx = url.find("doi.org/")?;
        let doi = &url[idx + 8..];
        if doi.starts_with("10.") {
            return Some(doi.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_arxiv_id() {
        assert!(is_arxiv_id("2301.12345"));
        assert!(is_arxiv_id("2301.12345v1"));
        assert!(is_arxiv_id("hep-th/9901001"));
        assert!(!is_arxiv_id("not-an-arxiv-id"));
        assert!(!is_arxiv_id("10.1234/example"));
    }

    #[test]
    fn test_extract_arxiv_from_url() {
        assert_eq!(
            extract_arxiv_from_url("https://arxiv.org/abs/2301.12345"),
            Some("2301.12345".to_string())
        );
        assert_eq!(
            extract_arxiv_from_url("https://arxiv.org/pdf/2301.12345.pdf"),
            Some("2301.12345".to_string())
        );
        assert_eq!(extract_arxiv_from_url("https://example.com"), None);
    }

    #[test]
    fn test_parse_simple_bib() {
        let bib = r#"
            @article{smith2021,
                author = {John Smith and Jane Doe},
                title = {A Great Paper},
                journal = {Nature},
                year = {2021},
                doi = {10.1234/example}
            }
        "#;

        let entries = parse_bib_string(bib).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].key, "smith2021");
        assert_eq!(entries[0].title, Some("A Great Paper".to_string()));
        assert_eq!(entries[0].year, Some(2021));
    }
}
