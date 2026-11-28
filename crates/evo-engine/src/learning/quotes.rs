//! A fact about the person that an automatic step saves must be backed by
//! the person's own words: an exact quote from each chat it cites, checked
//! against that chat, and every number, path and URL of the fact found in
//! the quotes (docs/features/learning-layers/spec.md, FR-8; plan 3.2).
//!
//! Quotes are compared folded: case, spacing, curly quotes and dashes do not
//! matter, in any language. Everything else must match.

/// `text` folded for comparison.
pub fn fold(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '\u{2018}' | '\u{2019}' | '\u{02BC}' | '`' => '\'',
            '\u{201C}' | '\u{201D}' | '\u{00AB}' | '\u{00BB}' => '"',
            '\u{2010}'..='\u{2015}' => '-',
            '\u{00A0}' => ' ',
            other => other,
        })
        .flat_map(char::to_lowercase)
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// A quote without the marks around it.
fn bare(quote: &str) -> String {
    fold(quote)
        .trim_matches(|c: char| c.is_whitespace() || "\"'.,;:!?…()[]".contains(c))
        .to_string()
}

/// The shortest quote that counts, in characters.
pub const MIN_QUOTE: usize = 4;

/// The numbers, paths and URLs of a fact: each must be in the quotes.
pub fn specifics(fact: &str) -> Vec<String> {
    fact.split_whitespace()
        .map(|word| word.trim_matches(|c: char| "\"'.,;:!?…()[]«»".contains(c)))
        .filter(|word| {
            word.contains("://")
                || word.contains('/')
                || word.contains('\\')
                || word.starts_with('~')
                || word.chars().any(|c| c.is_ascii_digit())
        })
        .map(fold)
        .filter(|word| !word.is_empty())
        .collect()
}

/// Check a fact against the chats it cites: `cited` holds, per cited chat,
/// the quote and that chat's lines by the person. `Err` says why it fails.
pub fn check(fact: &str, cited: &[(&str, &str)]) -> Result<(), String> {
    if cited.is_empty() {
        return Err("no quote of the person backs it".into());
    }
    let mut quotes = Vec::new();
    for (quote, person) in cited {
        let quote = bare(quote);
        if quote.chars().count() < MIN_QUOTE {
            return Err(format!("the quote `{quote}` is too short to check"));
        }
        if !fold(person).contains(&quote) {
            return Err(format!("the person never said `{quote}` in that chat"));
        }
        quotes.push(quote);
    }
    let all = quotes.join(" \n ");
    for specific in specifics(fact) {
        if !all.contains(&specific) {
            return Err(format!("`{specific}` is not in the person's words"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // FR-8: a quote must be in its chat, folded.
    #[test]
    fn quotes_must_be_in_their_chat() {
        let chat = "Can you keep drafts SHORT —  under 150 words?\nThanks.";
        assert!(check(
            "The person wants drafts under 150 words",
            &[("keep drafts short – under 150 words", chat)]
        )
        .is_ok());
        assert!(
            check("x", &[("“keep drafts short”", chat)]).is_ok(),
            "curly quotes and case fold"
        );
        let error = check("Wants long drafts", &[("I love long drafts", chat)]).unwrap_err();
        assert!(error.contains("never said"), "{error}");
        assert!(check("x", &[]).is_err(), "a fact needs a quote");
        assert!(
            check("x", &[("ok", chat)]).is_err(),
            "too short to mean anything"
        );
        // Each cited chat must hold its own quote.
        let other = "Please send the numbers every Monday.";
        assert!(check(
            "Asks for numbers on Mondays",
            &[("numbers every monday", other), ("send the numbers", chat)]
        )
        .is_err());
        assert!(check(
            "Asks for numbers on Mondays",
            &[("numbers every monday", other), ("thanks", chat)]
        )
        .is_ok());
    }

    // FR-8: numbers, paths and URLs must be in the quotes.
    #[test]
    fn numbers_paths_and_urls_must_be_in_the_quotes() {
        let chat = "Our repo is at https://git.example.com/app and builds go to D:/builds; budget 3000 a month.";
        assert!(check(
            "Repo at https://git.example.com/app",
            &[("our repo is at https://git.example.com/app", chat)]
        )
        .is_ok());
        assert!(check("Budget is 3000 a month", &[("budget 3000 a month", chat)]).is_ok());
        let error = check("Budget is 5000 a month", &[("budget 3000 a month", chat)]).unwrap_err();
        assert!(error.contains("5000"), "{error}");
        assert!(check("Builds go to D:/builds", &[("budget 3000 a month", chat)]).is_err());
        assert!(check(
            "Builds go to D:/builds",
            &[("builds go to d:/builds", chat)]
        )
        .is_ok());
        assert_eq!(
            specifics("In Hanoi until 2026-12-15, see ~/notes."),
            ["2026-12-15", "~/notes"]
        );
    }

    // FR-8: any language.
    #[test]
    fn quotes_work_in_any_language() {
        let chat = "Tôi muốn bản nháp NGẮN hơn, dưới 150 từ.";
        assert!(check(
            "Muốn bản nháp dưới 150 từ",
            &[("bản nháp ngắn hơn, dưới 150 từ", chat)]
        )
        .is_ok());
        let japanese = "下書きは短くしてください";
        assert!(check("短い下書き", &[("下書きは短く", japanese)]).is_ok());
        assert!(check("短い下書き", &[("長い下書き", japanese)]).is_err());
    }
}
