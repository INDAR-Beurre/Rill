//! Typos in the address field. If a word has one letter wrong, missing or
//! swapped, it still finds the page that was meant. This is only a fallback:
//! these matches rank below exact ones.
//!
//! The rules follow the reference browser's Fuzzy.swift, which was read and
//! not run:
//! - Words under four letters are never guessed.
//! - Words of four to seven letters allow one typo. Longer words allow two.
//! - Swapping two neighbouring letters counts as one typo.
//! - People rarely get the first letter wrong, so it has to match, or be
//!   swapped with the second.
//!
//! Lengths and edits count characters, not bytes, so an accented letter costs
//! one typo and a two-character CJK word is too short to guess.
//! It runs over the whole history on every key. Callers lowercase both sides first.

/// Letters and digits in any script.
fn in_word(c: char) -> bool {
    c.is_alphanumeric()
}

/// Whether `word` appears anywhere in `text`, exactly as typed.
pub fn contains(text: &[u8], word: &[u8]) -> bool {
    if word.is_empty() {
        return true;
    }
    text.len() >= word.len() && text.windows(word.len()).any(|w| w == word)
}

/// Some(1) if every typed word is in `text` as typed.
/// Some(0) if at least one needed a typo forgiven.
/// None if any word is not there at all.
pub fn score(typed: &[&str], text: &str) -> Option<u8> {
    let mut slipped = false;
    for word in typed {
        if contains(text.as_bytes(), word.as_bytes()) {
            continue;
        }
        if !nearly(word, text) {
            return None;
        }
        slipped = true;
    }
    Some(if slipped { 0 } else { 1 })
}

/// Whether some word in `text` starts with `typed`, allowing one typo, or two
/// if `typed` is eight letters or longer. It measures edit distance from
/// `typed` to the start of each word, counting a swap of two letters as one.
fn nearly(typed: &str, text: &str) -> bool {
    let typed: Vec<char> = typed.chars().collect();
    let allowed = if typed.len() < 4 {
        0
    } else if typed.len() < 8 {
        1
    } else {
        2
    };
    if allowed == 0 {
        return false;
    }
    let text: Vec<char> = text.chars().collect();
    let width = typed.len() + allowed + 1;
    // These rows are only made at the first word worth measuring. Most texts
    // have none, and this runs for every history entry on every key.
    let mut before: Vec<usize> = Vec::new();
    let mut above: Vec<usize> = Vec::new();
    let mut row: Vec<usize> = Vec::new();

    let mut start = 0;
    while start < text.len() {
        if !in_word(text[start]) {
            start += 1;
            continue;
        }
        let mut end = start;
        while end < text.len() && in_word(text[end]) {
            end += 1;
        }
        let word = &text[start..end];
        start = end;

        // Skip words that are too short, or whose first letter does not
        // match (allowing it to be swapped with the second).
        if word.len() < 2
            || word.len() < typed.len() - allowed
            || !(word[0] == typed[0] || (word[0] == typed[1] && word[1] == typed[0]))
        {
            continue;
        }
        if row.is_empty() {
            before = vec![0; width];
            above = vec![0; width];
            row = vec![0; width];
        }
        let columns = word.len().min(width - 1);
        for (j, cell) in above.iter_mut().enumerate().take(columns + 1) {
            *cell = j;
        }
        let mut far = false;
        for i in 1..=typed.len() {
            row[0] = i;
            let mut least = i;
            for j in 1..=columns {
                let swap = usize::from(typed[i - 1] != word[j - 1]);
                let mut best = (above[j] + 1).min(row[j - 1] + 1).min(above[j - 1] + swap);
                if i > 1 && j > 1 && typed[i - 1] == word[j - 2] && typed[i - 2] == word[j - 1] {
                    best = best.min(before[j - 2] + 1);
                }
                row[j] = best;
                least = least.min(best);
            }
            // Already too many typos for this word.
            if least > allowed {
                far = true;
                break;
            }
            // Rotate the rows: the oldest one is reused as the next row.
            let oldest = std::mem::take(&mut before);
            before = std::mem::replace(&mut above, std::mem::replace(&mut row, oldest));
        }
        if !far && above[1..=columns].iter().any(|&d| d <= allowed) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_exact_word_scores_above_a_forgiven_one() {
        assert_eq!(score(&["youtube"], "youtube - cats"), Some(1));
        assert_eq!(score(&["youtbue"], "youtube - cats"), Some(0));
        assert_eq!(score(&["gogle"], "google search"), Some(0));
        assert_eq!(score(&["nothing"], "google search"), None);
    }

    #[test]
    fn every_typed_word_has_to_be_found() {
        assert_eq!(score(&["rust", "docs"], "rust std docs"), Some(1));
        assert_eq!(score(&["rust", "zzzz"], "rust std docs"), None);
    }

    #[test]
    fn short_words_are_never_guessed() {
        // "cta" is too short to forgive, even though it is "cat" swapped.
        assert_eq!(score(&["cta"], "cat pictures"), None);
    }

    #[test]
    fn the_first_letter_must_match_or_be_swapped_with_the_second() {
        assert_eq!(score(&["oyutube"], "youtube"), Some(0));
        assert_eq!(score(&["xoutube"], "youtube"), None);
    }

    #[test]
    fn long_words_forgive_two_typos() {
        assert_eq!(score(&["wikipdeai"], "wikipedia"), Some(0));
        assert_eq!(score(&["wkpdeai"], "wikipedia"), None);
    }

    #[test]
    fn cjk_words_are_matched_but_never_guessed() {
        assert_eq!(score(&["日本"], "日本語"), Some(1));
        // Two characters is too short for a typo, even though bytes would allow one.
        assert_eq!(score(&["日木"], "日本"), None);
    }

    #[test]
    fn an_accented_letter_costs_one_typo() {
        assert_eq!(score(&["café"], "cafe"), Some(0));
        assert_eq!(score(&["cafe"], "café shop"), Some(0));
    }
}
