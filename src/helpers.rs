use strsim::levenshtein;

/// Find the most similar ID from a list of candidates
pub fn find_similar_id<'a>(target: &str, candidates: &[&'a str]) -> Option<&'a str> {
    candidates
        .iter()
        .map(|&candidate| (candidate, levenshtein(target, candidate)))
        .filter(|(_, distance)| *distance <= 2)
        .min_by_key(|(_, distance)| *distance)
        .map(|(id, _)| id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_similar_id() {
        let candidates = vec!["t8zwaROl", "xYz9Kp2m", "V1StGXR8"];

        assert_eq!(find_similar_id("t8zwaRO1", &candidates), Some("t8zwaROl"));

        assert_eq!(find_similar_id("xYz9Kp2n", &candidates), Some("xYz9Kp2m"));

        // Very different ID should return None
        assert_eq!(find_similar_id("zzzzz", &candidates), None);
    }
}
