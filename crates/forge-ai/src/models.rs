use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    /// US-Dollar je Million Token.
    pub prompt_price: f64,
    pub completion_price: f64,
    pub context: u64,
    pub supports_json: bool,
}

#[derive(Deserialize)]
struct Raw {
    #[serde(default)]
    data: Vec<RawModel>,
}

#[derive(Deserialize)]
struct RawModel {
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    context_length: u64,
    #[serde(default)]
    pricing: RawPricing,
    #[serde(default)]
    supported_parameters: Vec<String>,
}

#[derive(Deserialize, Default)]
struct RawPricing {
    #[serde(default)]
    prompt: Option<serde_json::Value>,
    #[serde(default)]
    completion: Option<serde_json::Value>,
}

fn price(v: &Option<serde_json::Value>) -> f64 {
    let n = match v {
        Some(serde_json::Value::String(s)) => s.parse::<f64>().unwrap_or(0.0),
        Some(serde_json::Value::Number(n)) => n.as_f64().unwrap_or(0.0),
        _ => 0.0,
    };
    n * 1_000_000.0
}

pub fn parse_models(value: serde_json::Value) -> Vec<ModelInfo> {
    let raw: Raw = serde_json::from_value(value).unwrap_or(Raw { data: vec![] });
    raw.data
        .into_iter()
        .map(|m| ModelInfo {
            supports_json: m
                .supported_parameters
                .iter()
                .any(|p| p == "response_format" || p == "structured_outputs"),
            prompt_price: price(&m.pricing.prompt),
            completion_price: price(&m.pricing.completion),
            name: if m.name.is_empty() {
                m.id.clone()
            } else {
                m.name
            },
            id: m.id,
            context: m.context_length,
        })
        .collect()
}

const PREFERRED: &[&str] = &[
    "deepseek/deepseek-v4-flash",
    "qwen/qwen3.7-flash",
    "google/gemini-2.5-flash-lite",
    "openai/gpt-4.1-nano",
    "anthropic/claude-haiku-5.5",
];

/// Ein günstiges, schnelles Modell mit JSON-Ausgabe. Bevorzugt bekannte Kandidaten, sonst das billigste brauchbare.
pub fn default_model(models: &[ModelInfo]) -> Option<String> {
    let usable = |m: &&ModelInfo| {
        m.supports_json && m.context >= 16_000 && m.prompt_price > 0.0 && !m.id.ends_with(":free")
    };
    for want in PREFERRED {
        if let Some(m) = models.iter().filter(usable).find(|m| m.id == *want) {
            return Some(m.id.clone());
        }
    }
    models
        .iter()
        .filter(usable)
        .min_by(|a, b| {
            (a.prompt_price + 3.0 * a.completion_price)
                .total_cmp(&(b.prompt_price + 3.0 * b.completion_price))
        })
        .map(|m| m.id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> serde_json::Value {
        serde_json::json!({ "data": [
            { "id": "vendor/huge", "name": "Huge", "context_length": 200000, "pricing": { "prompt": "0.00001", "completion": "0.00003" }, "supported_parameters": ["response_format"] },
            { "id": "vendor/cheap", "name": "Cheap", "context_length": 64000, "pricing": { "prompt": "0.0000001", "completion": "0.0000004" }, "supported_parameters": ["response_format", "tools"] },
            { "id": "vendor/nojson", "context_length": 64000, "pricing": { "prompt": "0.00000001", "completion": "0.00000004" }, "supported_parameters": [] },
            { "id": "vendor/free:free", "context_length": 64000, "pricing": { "prompt": "0", "completion": "0" }, "supported_parameters": ["response_format"] },
            { "id": "deepseek/deepseek-v4-flash", "context_length": 128000, "pricing": { "prompt": "0.00000015", "completion": "0.0000006" }, "supported_parameters": ["response_format"] }
        ] })
    }

    #[test]
    fn prices_are_per_million_and_json_support_is_detected() {
        let m = parse_models(catalog());
        let cheap = m.iter().find(|x| x.id == "vendor/cheap").unwrap();
        assert!(
            (cheap.prompt_price - 0.1).abs() < 1e-9 && (cheap.completion_price - 0.4).abs() < 1e-9
        );
        assert!(
            cheap.supports_json
                && !m
                    .iter()
                    .find(|x| x.id == "vendor/nojson")
                    .unwrap()
                    .supports_json
        );
    }

    #[test]
    fn default_prefers_known_models_then_the_cheapest_usable() {
        let m = parse_models(catalog());
        assert_eq!(
            default_model(&m).as_deref(),
            Some("deepseek/deepseek-v4-flash")
        );
        let without: Vec<_> = m
            .into_iter()
            .filter(|x| x.id != "deepseek/deepseek-v4-flash")
            .collect();
        assert_eq!(default_model(&without).as_deref(), Some("vendor/cheap"));
        assert_eq!(default_model(&[]), None);
    }
}
