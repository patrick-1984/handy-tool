//! Port of the frontend `resolvePrice` (src/lib/openrouterPrices.ts) so the
//! MCP/CLI provider-config tool can auto-fill cost from OpenRouter's catalogue
//! exactly as the UI does when a Gemini/Anthropic/OpenRouter model is selected.

use crate::model_testing::OpenRouterModelPrice;

/// Lowercase, drop a trailing `-YYYYMMDD` date suffix, and remove `.`/`-`/`_`/`/`
/// so e.g. native `claude-haiku-4-5` matches OpenRouter `anthropic/claude-haiku-4.5`.
fn normalize(s: &str) -> String {
    let lower = s.to_lowercase();
    let stripped = if lower.len() >= 9 {
        let tail = &lower[lower.len() - 9..];
        if tail.starts_with('-') && tail[1..].bytes().all(|c| c.is_ascii_digit()) {
            &lower[..lower.len() - 9]
        } else {
            lower.as_str()
        }
    } else {
        lower.as_str()
    };
    stripped
        .chars()
        .filter(|c| !matches!(c, '.' | '-' | '_' | '/'))
        .collect()
}

/// Find a model's (input, output) per-1M pricing in the OpenRouter catalogue.
/// Gemini/Anthropic native ids are mapped to their OpenRouter slug; OpenRouter
/// ids match directly. Returns `None` when not found.
pub fn resolve_price(
    prices: &[OpenRouterModelPrice],
    kind: &str,
    model_id: &str,
) -> Option<(f64, f64)> {
    let model_id = model_id.trim();
    if model_id.is_empty() {
        return None;
    }
    let vendor = match kind {
        "gemini" => "google",
        "anthropic" => "anthropic",
        _ => "",
    };
    let mut candidates: Vec<String> = vec![model_id.to_string()];
    if !vendor.is_empty() && !model_id.contains('/') {
        candidates.push(format!("{}/{}", vendor, model_id));
    }
    // 1) exact match on id or canonical_slug
    for m in prices {
        if candidates
            .iter()
            .any(|c| c == &m.id || c == &m.canonical_slug)
        {
            return Some((m.input_per_million, m.output_per_million));
        }
    }
    // 2) normalized fuzzy match
    let targets: Vec<String> = candidates.iter().map(|c| normalize(c)).collect();
    for m in prices {
        if targets.contains(&normalize(&m.id)) || targets.contains(&normalize(&m.canonical_slug)) {
            return Some((m.input_per_million, m.output_per_million));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn price(id: &str, slug: &str) -> OpenRouterModelPrice {
        OpenRouterModelPrice {
            id: id.to_string(),
            canonical_slug: slug.to_string(),
            name: id.to_string(),
            input_per_million: 1.0,
            output_per_million: 2.0,
        }
    }

    #[test]
    fn maps_anthropic_native_id_to_openrouter_slug() {
        let prices = vec![price(
            "anthropic/claude-haiku-4.5",
            "anthropic/claude-haiku-4.5",
        )];
        assert_eq!(
            resolve_price(&prices, "anthropic", "claude-haiku-4-5"),
            Some((1.0, 2.0))
        );
    }

    #[test]
    fn maps_gemini_and_strips_date_suffix() {
        let prices = vec![price("google/gemini-2.5-flash", "google/gemini-2.5-flash")];
        assert_eq!(
            resolve_price(&prices, "gemini", "gemini-2.5-flash"),
            Some((1.0, 2.0))
        );
        assert_eq!(
            resolve_price(&prices, "gemini", "gemini-2-5-flash-20250101"),
            Some((1.0, 2.0))
        );
    }

    #[test]
    fn openrouter_id_matches_directly_and_unknown_is_none() {
        let prices = vec![price("openai/gpt-4o", "openai/gpt-4o")];
        assert_eq!(
            resolve_price(&prices, "openrouter", "openai/gpt-4o"),
            Some((1.0, 2.0))
        );
        assert_eq!(resolve_price(&prices, "openrouter", "nope/zzz"), None);
        assert_eq!(resolve_price(&prices, "anthropic", ""), None);
    }
}
