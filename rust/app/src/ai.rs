//! AI assistant, bring your own key. Keys live in the OS keychain (macOS Keychain, Windows Credential
//! Manager) and each key is only ever sent to its own provider's address.
//!
//! Providers: Anthropic (Messages API over HTTPS; there is no official Rust SDK) and any
//! OpenAI-compatible chat API: OpenAI, Google Gemini, Mistral, Groq, DeepSeek, xAI, OpenRouter,
//! Together, local Ollama / LM Studio, or a custom endpoint.

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::mpsc;
use std::sync::LazyLock;
use std::time::Duration;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Anthropic,
    OpenAi,
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Group {
    Cloud,
    Local,
    Custom,
}

pub struct Provider {
    pub id: &'static str,
    pub name: &'static str,
    pub kind: Kind,
    pub group: Group,
    pub base: &'static str,
    pub key_url: &'static str,
    pub key_hint: &'static str,
    pub no_key: bool,
    pub key_optional: bool,
    pub editable_base: bool,
    pub default_model: &'static str,
}

const fn p(id: &'static str, name: &'static str, kind: Kind, group: Group, base: &'static str, key_url: &'static str, key_hint: &'static str) -> Provider {
    Provider { id, name, kind, group, base, key_url, key_hint, no_key: false, key_optional: false, editable_base: false, default_model: "" }
}

pub static PROVIDERS: &[Provider] = &[
    Provider {
        default_model: "claude-opus-5-5",
        ..p(
            "anthropic",
            "Anthropic (Claude)",
            Kind::Anthropic,
            Group::Cloud,
            "https://api.anthropic.com/v1",
            "https://console.anthropic.com/settings/keys",
            "sk-ant-…",
        )
    },
    p("openai", "OpenAI (ChatGPT)", Kind::OpenAi, Group::Cloud, "https://api.openai.com/v1", "https://platform.openai.com/api-keys", "sk-…"),
    p(
        "google",
        "Google Gemini",
        Kind::OpenAi,
        Group::Cloud,
        "https://generativelanguage.googleapis.com/v1beta/openai",
        "https://aistudio.google.com/apikey",
        "AIza…",
    ),
    p("mistral", "Mistral AI", Kind::OpenAi, Group::Cloud, "https://api.mistral.ai/v1", "https://console.mistral.ai/api-keys", ""),
    p("groq", "Groq", Kind::OpenAi, Group::Cloud, "https://api.groq.com/openai/v1", "https://console.groq.com/keys", "gsk_…"),
    p("deepseek", "DeepSeek", Kind::OpenAi, Group::Cloud, "https://api.deepseek.com/v1", "https://platform.deepseek.com/api_keys", "sk-…"),
    p("xai", "xAI (Grok)", Kind::OpenAi, Group::Cloud, "https://api.x.ai/v1", "https://console.x.ai/", "xai-…"),
    p("openrouter", "OpenRouter (many models)", Kind::OpenAi, Group::Cloud, "https://openrouter.ai/api/v1", "https://openrouter.ai/keys", "sk-or-…"),
    p("together", "Together AI", Kind::OpenAi, Group::Cloud, "https://api.together.xyz/v1", "https://api.together.ai/settings/api-keys", ""),
    Provider {
        no_key: true,
        editable_base: true,
        ..p("ollama", "Ollama (runs on your computer)", Kind::OpenAi, Group::Local, "http://localhost:11434/v1", "https://ollama.com/download", "")
    },
    Provider {
        no_key: true,
        editable_base: true,
        ..p("lmstudio", "LM Studio (runs on your computer)", Kind::OpenAi, Group::Local, "http://localhost:1234/v1", "https://lmstudio.ai/", "")
    },
    Provider { key_optional: true, editable_base: true, ..p("custom", "Custom OpenAI-compatible API", Kind::OpenAi, Group::Custom, "", "", "optional") },
];

pub fn provider(id: &str) -> Option<&'static Provider> {
    PROVIDERS.iter().find(|p| p.id == id)
}

/// Claude models that take `output_config.effort`.
fn takes_effort(model: &str) -> bool {
    [
        "claude-fable-",
        "claude-mythos-",
        "claude-opus-5",
        "claude-sonnet-5",
        "claude-haiku-5",
        "claude-opus-4-5",
        "claude-opus-4-6",
        "claude-opus-4-7",
        "claude-opus-4-8",
        "claude-sonnet-4-6",
    ]
    .iter()
    .any(|p| model.starts_with(p))
}
/// Claude models with server-side refusal fallback (`fallbacks: "default"`).
fn takes_fallbacks(model: &str) -> bool {
    ["claude-fable-5-2", "claude-fable-5-1", "claude-mythos-5-2", "claude-mythos-5-1", "claude-opus-5-5", "claude-opus-5", "claude-sonnet-5-5"].contains(&model)
}

// ---------------- pure helpers ----------------

pub fn is_private_host(host: &str) -> bool {
    let h = host.trim_start_matches('[').trim_end_matches(']').to_lowercase();
    if h == "localhost" || h == "::1" || h.ends_with(".localhost") || h.ends_with(".local") {
        return true;
    }
    let parts: Vec<u32> = h.split('.').filter_map(|x| x.parse().ok()).collect();
    if parts.len() != 4 || h.split('.').count() != 4 {
        return false;
    }
    let (a, b) = (parts[0], parts[1]);
    a == 127 || a == 10 || (a == 192 && b == 168) || (a == 172 && (16..=31).contains(&b))
}

/// Accept https anywhere, http only on this computer or the local network. Returns a clean base URL.
pub fn validate_base_url(raw: &str) -> Result<String, String> {
    let mut u = url::Url::parse(raw.trim()).map_err(|_| "Enter a full address, for example https://api.example.com/v1".to_string())?;
    if !u.username().is_empty() || u.password().is_some() {
        return Err("Put the API key in the key field, not in the address.".into());
    }
    match u.scheme() {
        "https" => {}
        "http" if u.host_str().is_some_and(is_private_host) => {}
        "http" => return Err("Use https:// for internet addresses (http:// is only allowed for this computer or your local network).".into()),
        _ => return Err("The address must start with https://".into()),
    }
    u.set_fragment(None);
    u.set_query(None);
    Ok(u.to_string().trim_end_matches('/').to_string())
}

#[derive(Clone, Debug, PartialEq)]
pub struct Model {
    pub id: String,
    pub name: String,
}

static NON_CHAT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)embed|whisper|tts|speech|dall-?e|image|imagen|audio|realtime|moderation|transcri|rerank|guard|search-preview|computer-use|veo|lyria")
        .unwrap_or_else(|e| panic!("{e}"))
});

/// A provider's model list as chat models, newest first.
pub fn chat_models(list: &[Value]) -> Vec<Model> {
    let mut seen = std::collections::HashSet::new();
    let mut out: Vec<(Model, String)> = vec![];
    for m in list {
        let raw = m.get("id").or(m.get("name")).and_then(Value::as_str).unwrap_or("");
        let id = raw.strip_prefix("models/").unwrap_or(raw).to_string();
        if id.is_empty() || NON_CHAT.is_match(&id) || !seen.insert(id.clone()) {
            continue;
        }
        let name = m.get("display_name").or(m.get("name")).and_then(Value::as_str).unwrap_or(&id).to_string();
        // sort key: ISO dates sort as text; unix seconds are padded to the same width
        let created = match (m.get("created_at").and_then(Value::as_str), m.get("created").and_then(Value::as_f64)) {
            (Some(s), _) => s.chars().filter(char::is_ascii_digit).take(14).collect::<String>(),
            (None, Some(n)) => format!("{:014}", n as u64),
            _ => String::new(),
        };
        out.push((Model { id, name }, created));
    }
    out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.id.cmp(&b.0.id)));
    out.into_iter().map(|x| x.0).collect()
}

/// Text from an OpenAI-style chat response (string or content-part array).
pub fn chat_text(json: &Value) -> String {
    let content = &json["choices"][0]["message"]["content"];
    match content {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts.iter().map(|p| p.as_str().map(String::from).unwrap_or_else(|| p["text"].as_str().unwrap_or("").to_string())).collect(),
        _ => String::new(),
    }
}

/// Keep only the outline: drop reasoning blocks, chatty lead-in lines, code fences, bullets, numbering and blank lines.
pub fn clean_outline(text: &str) -> String {
    static THINK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?is)<think>.*?</think>").unwrap_or_else(|e| panic!("{e}")));
    static TAGS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)</?(outline|map)>").unwrap_or_else(|e| panic!("{e}")));
    static FENCE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)```[a-z]*\n?|```").unwrap_or_else(|e| panic!("{e}")));
    static PREAMBLE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^(sure|certainly|of course|okay|ok|absolutely|great|here is|here's|below is|this is)\b.*[:!.]$").unwrap_or_else(|e| panic!("{e}"))
    });
    static BULLET: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\s*)(?:[-*+•]|[0-9]+[.)]|#{1,6})\s+").unwrap_or_else(|e| panic!("{e}")));
    static BOLD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\*\*(.+?)\*\*").unwrap_or_else(|e| panic!("{e}")));
    let t = THINK.replace_all(text, "");
    let t = TAGS.replace_all(&t, "");
    let t = FENCE.replace_all(&t, "");
    let mut lines: Vec<String> = t.split('\n').map(|l| l.replace('\t', "  ").trim_end().to_string()).filter(|l| !l.trim().is_empty()).collect();
    while lines.len() > 1 && PREAMBLE.is_match(lines[0].trim()) {
        lines.remove(0);
    }
    lines.iter().map(|l| BOLD.replace_all(&BULLET.replace(l, "${1}"), "${1}").into_owned()).collect::<Vec<_>>().join("\n")
}

const SYSTEM: &str = "You are the brainstorming engine of a mind-mapping app. Reply with ONLY an indented outline: one topic per line, indented with two spaces per level. No bullets, numbering, markdown, quotes or commentary. Keep every topic short (at most 7 words) and specific. Write in the same language as the user's request.";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Generate,
    Text,
    Expand,
}

#[derive(Clone, Debug)]
pub struct Request {
    pub provider: String,
    pub model: String,
    pub mode: Mode,
    /// "quick", "standard" or "deep".
    pub depth: String,
    pub topic: String,
    pub text: String,
    pub path: Vec<String>,
    pub existing: Vec<String>,
    pub outline: String,
}

fn clip(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

pub fn build_prompt(r: &Request) -> String {
    let depth = match r.depth.as_str() {
        "quick" => "Give 4 to 6 main branches with 2 to 3 subtopics each.",
        "deep" => "Give 6 to 8 main branches with 3 to 5 subtopics each, and a further level of detail under most subtopics.",
        _ => "Give 5 to 7 main branches, each with 2 to 5 subtopics; go one level deeper where it adds real value.",
    };
    match r.mode {
        Mode::Expand => {
            let existing = r.existing.iter().map(|x| clip(x, 80)).collect::<Vec<_>>().join("; ");
            format!(
                "Here is the current mind map as an indented outline:\n<map>\n{}\n</map>\n\nSuggest 4 to 7 NEW subtopics for the topic at this path: {}.\nDo not repeat existing subtopics: {}.\nReply with the new subtopics only (no line for the parent). You may add up to 3 indented children under each.",
                clip(&r.outline, 6000),
                r.path.iter().map(|x| clip(x, 120)).collect::<Vec<_>>().join(" > "),
                if existing.is_empty() { "none".into() } else { existing }
            )
        }
        Mode::Text => format!(
            "Turn the following text into a mind map. The first line is a short central topic that names the subject. Organise the key ideas into branches and keep the facts from the text.\n{depth}\n<text>\n{}\n</text>",
            clip(&r.text, 30000)
        ),
        Mode::Generate => format!("Create a mind map about: {}\nThe first line is the central topic. {depth}", clip(&r.topic, 500)),
    }
}

// ---------------- settings and keys ----------------

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    pub provider: String,
    pub models: HashMap<String, String>,
    pub base_urls: HashMap<String, String>,
}

fn config_path() -> std::path::PathBuf {
    crate::files::config_dir().join("ai-config.json")
}
pub fn load_config() -> Config {
    let mut c: Config = std::fs::read_to_string(config_path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    if provider(&c.provider).is_none() {
        c.provider = "anthropic".into();
    }
    c
}
pub fn save_config(c: &Config) {
    if let Ok(s) = serde_json::to_string_pretty(c) {
        let _ = crate::files::write_atomic(&config_path(), s.as_bytes());
    }
}

const KEYRING_SERVICE: &str = "Mindloom AI";

pub fn get_key(id: &str) -> Option<String> {
    let k = keyring::Entry::new(KEYRING_SERVICE, id).ok().and_then(|e| e.get_password().ok()).filter(|k| !k.is_empty());
    k.or_else(|| if id == "anthropic" { std::env::var("ANTHROPIC_API_KEY").ok().filter(|k| !k.is_empty()) } else { None })
}
pub fn set_key(id: &str, key: &str) -> Result<(), String> {
    if key.trim().is_empty() {
        return Err("Paste a key first.".into());
    }
    keyring::Entry::new(KEYRING_SERVICE, id)
        .and_then(|e| e.set_password(key.trim()))
        .map_err(|e| format!("Could not store the key in the system keychain ({e})."))
}
pub fn clear_key(id: &str) {
    if let Ok(e) = keyring::Entry::new(KEYRING_SERVICE, id) {
        let _ = e.delete_credential();
    }
}

// ---------------- provider calls ----------------

fn friendly(status: u16, detail: &str, p: &Provider, model: &str) -> String {
    let d = if detail.is_empty() { String::new() } else { format!(" ({})", clip(detail, 200)) };
    match status {
        401 | 403 => format!("{} rejected the API key{d}. Check the key and try again.", p.name),
        404 => format!("{} could not find the model “{model}”{d}. Pick another model from the list.", p.name),
        429 => format!("{} says you have hit a rate limit or run out of credit{d}.", p.name),
        400 => format!("{} could not process the request{d}.", p.name),
        s if s >= 500 => format!("{} is having problems right now{d}. Please try again shortly.", p.name),
        s => format!("{} returned an error {s}{d}.", p.name),
    }
}

struct Target {
    p: &'static Provider,
    base: String,
    key: Option<String>,
}

fn resolve(cfg: &Config, id: &str) -> Result<Target, String> {
    let p = provider(id).ok_or("Choose an AI provider first.")?;
    let base = cfg.base_urls.get(p.id).cloned().filter(|b| !b.is_empty()).unwrap_or_else(|| p.base.to_string());
    if base.is_empty() {
        return Err("Enter the address of your API (for example https://api.example.com/v1).".into());
    }
    let key = get_key(p.id);
    if key.is_none() && !p.no_key && !p.key_optional {
        return Err(format!("Add your {} API key first.", p.name));
    }
    let base = if p.kind == Kind::Anthropic { p.base.to_string() } else { validate_base_url(&base)? };
    Ok(Target { p, base, key })
}

fn call(t: &Target, route: &str, body: Option<Value>, timeout: u64, model: &str, beta: Option<&str>) -> Result<Value, String> {
    let client = reqwest::blocking::Client::builder().timeout(Duration::from_secs(timeout)).build().map_err(|e| e.to_string())?;
    let url = format!("{}{route}", t.base);
    let mut rq = match &body {
        Some(b) => client.post(&url).json(b),
        None => client.get(&url),
    };
    rq = rq.header("Accept", "application/json");
    match t.p.kind {
        Kind::Anthropic => {
            rq = rq.header("x-api-key", t.key.clone().unwrap_or_default()).header("anthropic-version", "2026-10-21");
            if let Some(b) = beta {
                rq = rq.header("anthropic-beta", b);
            }
        }
        Kind::OpenAi => {
            if let Some(k) = &t.key {
                rq = rq.bearer_auth(k);
            }
            if t.p.id == "openrouter" {
                rq = rq.header("X-Title", "Mindloom").header("HTTP-Referer", "https://github.com/imranseoxpert/mindloom");
            }
        }
    }
    let res = rq.send().map_err(|e| {
        if e.is_timeout() {
            format!("{} took too long to answer. Try a smaller or faster model.", t.p.name)
        } else if t.p.no_key {
            format!("Could not reach {} at {}. Make sure it is running and the address is right.", t.p.name, t.base)
        } else {
            format!("Could not reach {} ({}). Check your internet connection and the address.", t.p.name, t.base)
        }
    })?;
    let status = res.status().as_u16();
    let text = res.text().unwrap_or_default();
    let json: Option<Value> = serde_json::from_str(&text).ok();
    if !(200..300).contains(&status) {
        let detail = json
            .as_ref()
            .and_then(|j| j["error"]["message"].as_str().or(j["error"].as_str()).or(j["message"].as_str()).map(String::from))
            .unwrap_or_else(|| clip(&text, 200));
        return Err(friendly(status, &detail, t.p, model));
    }
    json.ok_or_else(|| format!("{} sent an answer Mindloom could not read.", t.p.name))
}

/// The chat models this provider offers to the saved key.
pub fn models(cfg: &Config, id: &str) -> Result<Vec<Model>, String> {
    let t = resolve(cfg, id)?;
    let route = if t.p.kind == Kind::Anthropic { "/models?limit=1000" } else { "/models" };
    let json = call(&t, route, None, 30, "", None)?;
    let list = json["data"].as_array().or(json["models"].as_array()).cloned().unwrap_or_default();
    Ok(chat_models(&list))
}

/// Ask the model for an outline.
pub fn generate(cfg: &Config, r: &Request) -> Result<String, String> {
    let t = resolve(cfg, &r.provider)?;
    let model = if r.model.is_empty() { t.p.default_model.to_string() } else { r.model.clone() };
    if model.is_empty() {
        return Err(format!("Choose a {} model first (press “Load models”).", t.p.name));
    }
    let prompt = build_prompt(r);
    let text = match t.p.kind {
        Kind::Anthropic => {
            let mut body = json!({ "model": model, "max_tokens": 16000, "system": SYSTEM, "messages": [{ "role": "user", "content": prompt }] });
            if takes_effort(&model) {
                body["output_config"] = json!({ "effort": "medium" });
            }
            let beta = takes_fallbacks(&model).then(|| {
                body["fallbacks"] = json!("default");
                "server-side-fallback-2026-07-01"
            });
            let res = call(&t, "/messages", Some(body), 300, &model, beta)?;
            if res["stop_reason"] == "refusal" {
                return Err("The model declined this request. Try rephrasing it.".into());
            }
            res["content"]
                .as_array()
                .map(|c| c.iter().filter(|b| b["type"] == "text").filter_map(|b| b["text"].as_str()).collect::<Vec<_>>().join("\n"))
                .unwrap_or_default()
        }
        Kind::OpenAi => {
            let body = json!({ "model": model, "stream": false, "messages": [{ "role": "system", "content": SYSTEM }, { "role": "user", "content": prompt }] });
            let res = call(&t, "/chat/completions", Some(body), 240, &model, None)?;
            if res["choices"][0]["finish_reason"] == "content_filter" {
                return Err("The model declined this request. Try rephrasing it.".into());
            }
            chat_text(&res)
        }
    };
    let outline = clean_outline(&text);
    if outline.trim().is_empty() {
        return Err("The model returned an empty answer. Try again or pick another model.".into());
    }
    Ok(outline)
}

/// Work done off the interface thread; the result arrives on the receiver.
pub enum Job {
    Models(String),
    Generate(Box<Request>),
}
pub enum Done {
    Models(String, Result<Vec<Model>, String>),
    Generate(Box<Request>, Result<String, String>),
}

pub fn spawn(job: Job, cfg: Config, ctx: egui::Context) -> mpsc::Receiver<Done> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let out = match job {
            Job::Models(id) => {
                let r = models(&cfg, &id);
                Done::Models(id, r)
            }
            Job::Generate(r) => {
                let out = generate(&cfg, &r);
                Done::Generate(r, out)
            }
        };
        let _ = tx.send(out);
        ctx.request_repaint();
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls() {
        assert_eq!(validate_base_url("https://api.x.com/v1/").unwrap(), "https://api.x.com/v1");
        assert!(validate_base_url("http://api.x.com/v1").is_err());
        assert!(validate_base_url("http://192.168.1.5:8080/v1").is_ok());
        assert!(validate_base_url("http://localhost:11434/v1").is_ok());
        assert!(validate_base_url("https://user:pw@x.com").is_err());
        assert!(validate_base_url("ftp://x.com").is_err());
        assert!(!is_private_host("172.32.0.1"));
        assert!(is_private_host("172.16.0.1"));
    }

    #[test]
    fn outlines_are_cleaned() {
        let raw = "Sure! Here is your map:\n```\n<think>hmm</think>\n# Launch\n  - **Plan** it\n\t1. Budget\n```";
        assert_eq!(clean_outline(raw), "Launch\n  Plan it\n  Budget");
    }

    #[test]
    fn model_lists() {
        let list = vec![json!({"id": "text-embedding-3"}), json!({"id": "a", "created": 1}), json!({"id": "b", "created": 5}), json!({"id": "b"})];
        let m = chat_models(&list);
        assert_eq!(m.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(), ["b", "a"]);
        let c = chat_models(&[
            json!({"id": "claude-opus-5-5", "display_name": "Claude Opus 5.5", "created_at": "2026-10-01T00:00:00Z"}),
            json!({"id": "claude-x", "created_at": "2025-01-01T00:00:00Z"}),
        ]);
        assert_eq!(c[0].name, "Claude Opus 5.5");
        assert_eq!(chat_text(&json!({"choices": [{"message": {"content": [{"text": "a"}, "b"]}}]})), "ab");
    }

    #[test]
    fn prompts() {
        let r = Request {
            provider: "anthropic".into(),
            model: String::new(),
            mode: Mode::Generate,
            depth: "quick".into(),
            topic: "Tea".into(),
            text: String::new(),
            path: vec![],
            existing: vec![],
            outline: String::new(),
        };
        assert!(build_prompt(&r).starts_with("Create a mind map about: Tea"));
        assert!(takes_fallbacks("claude-opus-5-5") && !takes_fallbacks("claude-haiku-5-5"));
        assert!(takes_effort("claude-haiku-5-5") && !takes_effort("claude-haiku-4-5"));
    }
}
