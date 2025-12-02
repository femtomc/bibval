use crate::entry::{normalize_string, Discrepancy, DiscrepancyField, Entry, Severity};
use strsim::jaro_winkler;

/// Threshold for title similarity (0.0 to 1.0)
const TITLE_MATCH_THRESHOLD: f64 = 0.85;
const TITLE_WARNING_THRESHOLD: f64 = 0.90;

/// Threshold for author name similarity
const AUTHOR_MATCH_THRESHOLD: f64 = 0.80;

/// Compare two entries and return a list of discrepancies
pub fn compare_entries(local: &Entry, remote: &Entry) -> Vec<Discrepancy> {
    let mut discrepancies = Vec::new();

    // Compare titles
    if let (Some(local_title), Some(remote_title)) = (&local.title, &remote.title) {
        let local_norm = normalize_string(local_title);
        let remote_norm = normalize_string(remote_title);

        let similarity = jaro_winkler(&local_norm, &remote_norm);

        if similarity < TITLE_MATCH_THRESHOLD {
            discrepancies.push(Discrepancy {
                field: DiscrepancyField::Title,
                severity: Severity::Error,
                local_value: local_title.clone(),
                remote_value: remote_title.clone(),
                message: format!(
                    "Title significantly different (similarity: {:.0}%)",
                    similarity * 100.0
                ),
            });
        } else if similarity < TITLE_WARNING_THRESHOLD {
            discrepancies.push(Discrepancy {
                field: DiscrepancyField::Title,
                severity: Severity::Warning,
                local_value: local_title.clone(),
                remote_value: remote_title.clone(),
                message: format!(
                    "Title slightly different (similarity: {:.0}%)",
                    similarity * 100.0
                ),
            });
        }
    }

    // Compare years
    if let (Some(local_year), Some(remote_year)) = (local.year, remote.year) {
        if local_year != remote_year {
            discrepancies.push(Discrepancy {
                field: DiscrepancyField::Year,
                severity: Severity::Error,
                local_value: local_year.to_string(),
                remote_value: remote_year.to_string(),
                message: format!("Year mismatch: {} vs {}", local_year, remote_year),
            });
        }
    }

    // Compare authors
    let author_issues = compare_authors(&local.authors, &remote.authors);
    discrepancies.extend(author_issues);

    // Check for missing DOI
    if local.doi.is_none() && remote.doi.is_some() {
        discrepancies.push(Discrepancy {
            field: DiscrepancyField::Doi,
            severity: Severity::Warning,
            local_value: "(none)".to_string(),
            remote_value: remote.doi.clone().unwrap(),
            message: "Missing DOI in local entry".to_string(),
        });
    }

    // Compare venues
    if let (Some(local_venue), Some(remote_venue)) = (&local.venue, &remote.venue) {
        let local_norm = normalize_string(local_venue);
        let remote_norm = normalize_string(remote_venue);

        let similarity = jaro_winkler(&local_norm, &remote_norm);

        if similarity < 0.70 {
            discrepancies.push(Discrepancy {
                field: DiscrepancyField::Venue,
                severity: Severity::Info,
                local_value: local_venue.clone(),
                remote_value: remote_venue.clone(),
                message: "Venue name differs".to_string(),
            });
        }
    }

    discrepancies
}

/// Compare author lists and return discrepancies
fn compare_authors(local: &[String], remote: &[String]) -> Vec<Discrepancy> {
    let mut discrepancies = Vec::new();

    if local.is_empty() || remote.is_empty() {
        return discrepancies;
    }

    // Check author count
    if local.len() != remote.len() {
        discrepancies.push(Discrepancy {
            field: DiscrepancyField::Authors,
            severity: Severity::Warning,
            local_value: format!("{} authors", local.len()),
            remote_value: format!("{} authors", remote.len()),
            message: format!(
                "Author count differs: {} (local) vs {} (remote)",
                local.len(),
                remote.len()
            ),
        });
    }

    // Check each local author against remote authors
    for local_author in local {
        let local_norm = normalize_string(local_author);
        let best_match = remote
            .iter()
            .map(|r| {
                let remote_norm = normalize_string(r);
                (r, jaro_winkler(&local_norm, &remote_norm))
            })
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

        if let Some((remote_author, similarity)) = best_match {
            if similarity < AUTHOR_MATCH_THRESHOLD {
                discrepancies.push(Discrepancy {
                    field: DiscrepancyField::Authors,
                    severity: Severity::Warning,
                    local_value: local_author.clone(),
                    remote_value: remote_author.clone(),
                    message: format!(
                        "Author name spelling may differ: '{}' vs '{}'",
                        local_author, remote_author
                    ),
                });
            }
        }
    }

    discrepancies
}

/// Calculate title similarity between two entries
pub fn title_similarity(a: &Entry, b: &Entry) -> f64 {
    match (&a.title, &b.title) {
        (Some(title_a), Some(title_b)) => {
            let norm_a = normalize_string(title_a);
            let norm_b = normalize_string(title_b);
            jaro_winkler(&norm_a, &norm_b)
        }
        _ => 0.0,
    }
}

/// Find the best matching entry from a list of candidates
pub fn find_best_match<'a>(target: &Entry, candidates: &'a [Entry]) -> Option<(&'a Entry, f64)> {
    candidates
        .iter()
        .map(|c| (c, title_similarity(target, c)))
        .filter(|(_, sim)| *sim >= TITLE_MATCH_THRESHOLD)
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_title_similarity() {
        let mut a = Entry::new("a".to_string(), "article".to_string());
        a.title = Some("Deep Learning for Image Classification".to_string());

        let mut b = Entry::new("b".to_string(), "article".to_string());
        b.title = Some("Deep Learning for Image Classification".to_string());

        assert!(title_similarity(&a, &b) > 0.99);

        b.title = Some("Deep Learning for Image Recognition".to_string());
        assert!(title_similarity(&a, &b) > 0.85);

        b.title = Some("Quantum Computing in Finance".to_string());
        assert!(title_similarity(&a, &b) < 0.7);
    }

    #[test]
    fn test_year_mismatch() {
        let mut local = Entry::new("test".to_string(), "article".to_string());
        local.title = Some("Test Paper".to_string());
        local.year = Some(2021);

        let mut remote = Entry::new("test".to_string(), "article".to_string());
        remote.title = Some("Test Paper".to_string());
        remote.year = Some(2020);

        let discrepancies = compare_entries(&local, &remote);
        assert!(discrepancies.iter().any(|d| d.field == DiscrepancyField::Year));
    }
}
