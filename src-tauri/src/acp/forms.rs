//! Forms an agent asks you to fill in, in a session Codealong runs: the Agent Client Protocol's
//! form elicitation (`elicitation/create` with `mode: "form"`), which an adapter sends only to a
//! client that says in `initialize` that it takes them (`clientCapabilities.elicitation.form`).
//! The adapters ask their harness's own questions this way, and leave the tool out without it:
//! claude-agent-acp 0.81.2 Claude Code's `AskUserQuestion` (`disallowedTools` otherwise), codex-acp
//! 1.13.1 Codex's `request_user_input` (answered with nothing otherwise), and `agy acp` 1.5.16
//! Antigravity's `ask_question` (completed without asking otherwise). They pass on what an MCP
//! server asks (MCP's elicitation) the same way, and claude-agent-acp the consent to retry a
//! refused prompt on its fallback model.
//!
//! A form waits as a permission request does (acp.rs `Permission::form`), and shows as the
//! questions the panel answers (`Pending::questions`): a question for each field, keyed by its
//! text, with what the field takes beyond a pick or words of your own (`Question::form`). Your
//! answers go back as the field values the form asked for (`content`); Decline is the protocol's
//! `decline`, after which every adapter lets its agent go on, told you didn't answer, and Stop
//! or the agent's withdrawing it is `cancel`.
//!
//! What each adapter writes, as read in their code and from `agy acp` (2026-11-24):
//! - claude-agent-acp: `question_<n>` for each question (`oneOf`, or an `array` of `anyOf` for
//!   several picks), its header the field's `title` and its text the `description`, or the form's
//!   `message` when it asks one, and an optional `question_<n>_custom` for words of your own,
//!   marked `_meta._askUserQuestionCustomAnswer.questionId`. A choice's value is its label.
//! - codex-acp: one field a question, keyed by Codex's id, its text the `title` and its header the
//!   `description`, every one `required`, a choice "None of the above" where words of your own may
//!   answer it, and those words in a field marked `_meta.codex.role: "user_note"` with its
//!   `questionId`; `_meta.codex.isSecret` hides what's typed.
//! - `agy acp`: `q<n>`, its text the `title`, values `"1"`, `"2"`… titled with the labels, and an
//!   optional `q<n>_other` for words of your own, unmarked.
//!
//! The order of the fields is lost as the line is read (serde_json keeps an object's keys sorted
//! here), so they're put back in the order `required` lists them, where it does (codex-acp lists
//! every question in the order asked), then in their keys' order with numbers read as numbers
//! (`question_2` before `question_10`), which is the order the other two ask in.

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use codealong_peer::api::{Choice, FormField, Pending, Question};

use crate::harness::Answer;

/// What a form's id starts with, among the requests a session waits on (acp.rs
/// `Permission::tool_call`), which an answer names.
const FORM_ID: &str = "form-";

/// A choice that stands for words of your own where a field has a box for them (codex-acp's
/// "None of the above", whose description says to write the answer in the note): not offered,
/// since the box is there, and picked when words alone answer the field.
const OTHER_CHOICES: [&str; 2] = ["none of the above", "other"];

/// A form, as asked.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(super) struct Form {
    /// What it says it wants (`message`).
    message: String,
    /// The tool call it's asked in, where it's the harness's own question tool's: its `message`
    /// then says only that there are questions.
    tool_call: Option<String>,
    fields: Vec<Field>,
}

/// One of a form's fields, as one of the questions the panel answers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct Field {
    /// Its key in the answer.
    key: String,
    question: Question,
    kind: Kind,
    /// Each choice's value, in the order of `question.options`, whose labels they go with.
    values: Vec<String>,
    /// The key of the field that takes words of your own for it (its "Other").
    words: Option<String>,
    /// The value of a choice that stands for words of your own (`OTHER_CHOICES`).
    other: Option<String>,
    /// What it's given when it's required and left unanswered.
    default: Option<Value>,
    /// Its bounds: a number's least and most, or a text's or a list's least and most items.
    min: Option<f64>,
    max: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum Kind {
    Text,
    Number,
    Integer,
    YesNo,
    One,
    Many,
}

/// A form's id among the requests its session waits on: the JSON-RPC id it came with, which is
/// a number from claude-agent-acp and codex-acp and a string from `agy acp`
/// ("jetski-acp-front:2").
pub(super) fn id_of(rpc: &Value) -> String {
    match rpc {
        Value::String(s) => format!("{FORM_ID}{s}"),
        other => format!("{FORM_ID}{other}"),
    }
}

/// The form `params` asks for, or why Codealong can't show it: a link to open (`url` mode, which
/// Codealong doesn't say it takes), or a field it has no way to ask for that the form can't go
/// without.
pub(super) fn read(params: &Value) -> Result<Form, String> {
    match params.get("mode").and_then(Value::as_str) {
        None | Some("form") => {}
        Some(mode) => return Err(format!("it asks for a {mode} elicitation, which Codealong doesn't take")),
    }
    let message = params.get("message").and_then(Value::as_str).unwrap_or("").trim().to_string();
    let tool_call = params.get("toolCallId").and_then(Value::as_str).map(String::from);
    let schema = params.get("requestedSchema").unwrap_or(&Value::Null);
    let empty = Map::new();
    let properties = schema.get("properties").and_then(Value::as_object).unwrap_or(&empty);
    let required: Vec<&str> = schema.get("required").and_then(Value::as_array).map(|r| r.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
    // Each box for words of your own, by the key of the field it's for.
    let words: Vec<(&str, &str)> = properties.iter().filter_map(|(k, p)| Some((words_for(k, p, properties)?, k.as_str()))).collect();
    let mut keys: Vec<&String> = properties.keys().filter(|k| !words.iter().any(|(_, w)| *w == k.as_str())).collect();
    keys.sort_by(|a, b| {
        let at = |k: &str| required.iter().position(|r| *r == k).unwrap_or(usize::MAX);
        at(a).cmp(&at(b)).then_with(|| natural(a, b))
    });
    let single = keys.len() == 1;
    let mut fields = Vec::new();
    for key in keys {
        let p = &properties[key];
        let words = words.iter().find(|(k, _)| *k == key.as_str()).map(|(_, w)| w.to_string());
        let need = required.contains(&key.as_str());
        match field(key, p, words, need, single, &message, properties) {
            Some(f) => fields.push(f),
            None if need => return Err(format!("its field `{key}` is of a kind Codealong can't ask for")),
            None => {}
        }
    }
    // Answers are keyed by their question's words, so no two may have the same.
    for i in 1..fields.len() {
        let mut n = 1;
        while fields[..i].iter().any(|f| f.question.question == fields[i].question.question) {
            n += 1;
            fields[i].question.question = format!("{} ({n})", fields[i].question.question.trim_end_matches(&format!(" ({})", n - 1)));
        }
    }
    Ok(Form { message, tool_call, fields })
}

/// The key of the field `key` is the box for words of your own of, if it's one: marked so by
/// claude-agent-acp (`_askUserQuestionCustomAnswer`, a marker it means for any adapter) or
/// codex-acp (`role: "user_note"`), or a text field named for a field of choices with `_other`
/// (`agy acp`) or `_custom` after it.
fn words_for<'a>(key: &str, p: &'a Value, properties: &'a Map<String, Value>) -> Option<&'a str> {
    let marked = p.pointer("/_meta/_askUserQuestionCustomAnswer/questionId").or_else(|| p.pointer("/_meta/codex/questionId").filter(|_| p.pointer("/_meta/codex/role").and_then(Value::as_str) == Some("user_note")));
    if let Some(of) = marked.and_then(Value::as_str) {
        return properties.get_key_value(of).map(|(k, _)| k.as_str());
    }
    if p.get("type").and_then(Value::as_str) != Some("string") || p.get("enum").is_some() || p.get("oneOf").is_some() {
        return None;
    }
    let base = key.strip_suffix("_other").or_else(|| key.strip_suffix("_custom"))?;
    let (k, of) = properties.get_key_value(base)?;
    matches!(kind_of(of), Some(Kind::One | Kind::Many)).then_some(k.as_str())
}

fn kind_of(p: &Value) -> Option<Kind> {
    match p.get("type").and_then(Value::as_str)? {
        "string" if p.get("oneOf").is_some() || p.get("enum").is_some() => Some(Kind::One),
        "string" => Some(Kind::Text),
        "number" => Some(Kind::Number),
        "integer" => Some(Kind::Integer),
        "boolean" => Some(Kind::YesNo),
        "array" if p.pointer("/items/anyOf").is_some() || p.pointer("/items/enum").is_some() => Some(Kind::Many),
        _ => None,
    }
}

/// A field as a question, or none when it's of a kind Codealong can't ask for.
fn field(key: &str, p: &Value, words: Option<String>, required: bool, single: bool, message: &str, properties: &Map<String, Value>) -> Option<Field> {
    let kind = kind_of(p)?;
    let text = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).map(str::trim).filter(|t| !t.is_empty()).map(String::from);
    let (title, description) = (text(p, "title"), text(p, "description"));
    let box_of = words.as_deref().and_then(|w| properties.get(w));
    let codex = p.pointer("/_meta/codex").is_some();
    // claude-agent-acp writes the question in the description, or the form's message when it
    // asks one, with the header as the title; codex-acp the question as the title and the header
    // as the description. Any other form: a title and a description are a header and what it
    // asks; one alone is what it asks.
    let claude = box_of.is_some_and(|b| b.pointer("/_meta/_askUserQuestionCustomAnswer").is_some());
    let message = Some(message.to_string()).filter(|m| single && !m.is_empty());
    let (question, header) = if claude {
        (description.or(message.clone()).or(title.clone()), title)
    } else if codex {
        (title.or(description.clone()), description)
    } else {
        match (title, description) {
            (Some(t), Some(d)) if t != d => (Some(d), Some(t)),
            (t, d) => (t.or(d).or(message), None),
        }
    };
    let mut options = Vec::new();
    let mut values = Vec::new();
    let listed = match kind {
        Kind::One => p.get("oneOf").or_else(|| p.get("enum")),
        Kind::Many => p.pointer("/items/anyOf").or_else(|| p.pointer("/items/enum")),
        _ => None,
    };
    let names = p.get("enumNames").and_then(Value::as_array);
    for (i, c) in listed.and_then(Value::as_array).into_iter().flatten().enumerate() {
        let value = match c {
            Value::String(v) => v.clone(),
            other => match other.get("const") {
                Some(Value::String(v)) => v.clone(),
                Some(v) if !v.is_null() => v.to_string(),
                _ => continue,
            },
        };
        let label = text(c, "title").or_else(|| names.and_then(|n| n.get(i)).and_then(Value::as_str).map(String::from)).unwrap_or_else(|| value.clone());
        options.push(Choice { label, description: text(c, "description") });
        values.push(value);
    }
    if kind == Kind::YesNo {
        options = ["Yes", "No"].map(|l| Choice { label: l.into(), description: None }).to_vec();
        values = ["true", "false"].map(String::from).to_vec();
    }
    let mut other = None;
    if words.is_some() {
        if let Some(i) = options.iter().position(|o| OTHER_CHOICES.contains(&o.label.to_lowercase().as_str())) {
            options.remove(i);
            other = Some(values.remove(i));
        }
    }
    let num = |k: &str| p.get(k).and_then(Value::as_f64);
    let (min, max) = match kind {
        Kind::Number | Kind::Integer => (num("minimum"), num("maximum")),
        Kind::Text => (num("minLength"), num("maxLength")),
        Kind::Many => (num("minItems"), num("maxItems")),
        _ => (None, None),
    };
    let secret = [Some(p), box_of].into_iter().flatten().any(|v| v.pointer("/_meta/codex/isSecret").and_then(Value::as_bool) == Some(true));
    let form = FormField { choices_only: matches!(kind, Kind::One | Kind::Many | Kind::YesNo) && words.is_none(), required, hint: hint(kind, p, min, max), secret };
    let question = Question { question: question.unwrap_or_else(|| key.to_string()), header, options, multi_select: kind == Kind::Many, form: Some(form) };
    Some(Field { key: key.to_string(), question, kind, values, words, other, default: p.get("default").filter(|d| !d.is_null()).cloned(), min, max })
}

/// What words a field takes, for the box's placeholder, where it isn't any.
fn hint(kind: Kind, p: &Value, min: Option<f64>, max: Option<f64>) -> Option<String> {
    let range = |what: &str| match (min, max) {
        (Some(a), Some(b)) => format!("{what} from {} to {}", number(a), number(b)),
        (Some(a), None) => format!("{what} of {} or more", number(a)),
        (None, Some(b)) => format!("{what} up to {}", number(b)),
        (None, None) => what.to_string(),
    };
    match kind {
        Kind::Number => Some(range("A number")),
        Kind::Integer => Some(range("A whole number")),
        Kind::Text => match p.get("format").and_then(Value::as_str) {
            Some("email") => Some("An email address".into()),
            Some("uri") => Some("A link".into()),
            Some("date") => Some("A date, as 2026-11-24".into()),
            Some("date-time") => Some("A date and time, as 2026-11-24T15:30:00Z".into()),
            _ => None,
        },
        _ => None,
    }
}

fn number(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        n.to_string()
    }
}

/// Keys in order with the numbers in them read as numbers: `q2` before `q10`.
fn natural(a: &str, b: &str) -> Ordering {
    fn runs(s: &str) -> Vec<(bool, &str)> {
        let mut out = Vec::new();
        let mut start = 0;
        let bytes = s.as_bytes();
        for i in 1..=bytes.len() {
            if i == bytes.len() || bytes[i].is_ascii_digit() != bytes[start].is_ascii_digit() {
                out.push((bytes[start].is_ascii_digit(), &s[start..i]));
                start = i;
            }
        }
        out
    }
    let (ra, rb) = (runs(a), runs(b));
    for (x, y) in ra.iter().zip(rb.iter()) {
        let o = match (x, y) {
            ((true, x), (true, y)) => x.trim_start_matches('0').len().cmp(&y.trim_start_matches('0').len()).then_with(|| x.trim_start_matches('0').cmp(y.trim_start_matches('0'))),
            ((_, x), (_, y)) => x.cmp(y),
        };
        if o != Ordering::Equal {
            return o;
        }
    }
    ra.len().cmp(&rb.len())
}

impl Form {
    /// The request as the panel shows it, under `id` (`id_of`).
    pub(super) fn pending(&self, id: &str) -> Pending {
        Pending { id: id.to_string(), label: self.label(), input: self.says(), questions: self.fields.iter().map(|f| f.question.clone()).collect(), ..Default::default() }
    }

    /// The form as a line: what it asks first, or what it says it wants where its agent asked
    /// it for something other than its own questions (an MCP server's form).
    pub(super) fn label(&self) -> String {
        let first = self.fields.first().map(|f| f.question.question.as_str());
        let asks = match (&self.tool_call, first) {
            (Some(_), Some(q)) => q,
            _ if !self.message.is_empty() => &self.message,
            (_, Some(q)) => q,
            _ => "A form to fill in",
        };
        crate::board::asks_you(asks)
    }

    /// What it says it wants, shown above its questions, where that's more than they say: never
    /// for the harness's own question tool's, whose message only counts its questions ("The agent
    /// has 2 questions."), nor where it's the one question's words.
    pub(super) fn says(&self) -> Option<String> {
        let shown = self.message.is_empty() || self.tool_call.is_some() || self.fields.iter().any(|f| f.question.question == self.message);
        (!shown && !self.fields.is_empty()).then(|| self.message.clone())
    }

    /// The reply to `answer` without answers to its questions: Deny declines it; Allow sends a
    /// form that asks nothing (a confirmation) as accepted, and is refused for one that asks
    /// something, from a device that offers Allow for questions it doesn't know.
    pub(super) fn reply(&self, answer: Answer) -> Result<Value, &'static str> {
        match answer {
            Answer::Deny => Ok(json!({ "action": "decline" })),
            Answer::Allow if self.fields.is_empty() => Ok(json!({ "action": "accept", "content": {} })),
            Answer::Allow => Err("It asks you questions: answer them, or decline."),
            Answer::Always => Err("A form is answered each time it's asked: answer it, or decline."),
        }
    }

    /// The reply that sends `answers` (by question, a choice's label or words of yours, or a list
    /// of them) as the values the form asked for, or why they can't be: a field left unanswered
    /// that the form needs, or an answer a field doesn't take.
    pub(super) fn answered(&self, answers: &Value) -> Result<Value, String> {
        let mut content = Map::new();
        let mut missing = Vec::new();
        for f in &self.fields {
            let given: Vec<String> = match answers.get(&f.question.question) {
                Some(Value::Array(list)) => list.iter().filter_map(Value::as_str).map(str::trim).filter(|t| !t.is_empty()).map(String::from).collect(),
                Some(Value::String(s)) if !s.trim().is_empty() => vec![s.trim().to_string()],
                _ => Vec::new(),
            };
            if given.is_empty() {
                match (&f.default, f.question.form.as_ref().is_some_and(|ff| ff.required)) {
                    (Some(d), true) => {
                        content.insert(f.key.clone(), d.clone());
                    }
                    (None, true) => missing.push(format!("“{}”", f.question.header.as_deref().unwrap_or(&f.question.question))),
                    _ => {}
                }
                continue;
            }
            f.fill(&given, &mut content)?;
        }
        if !missing.is_empty() {
            return Err(format!("Answer {} too: the form needs it.", missing.join(", ")));
        }
        if content.is_empty() && !self.fields.is_empty() {
            return Err("Pick an answer first.".into());
        }
        Ok(json!({ "action": "accept", "content": content }))
    }
}

impl Field {
    /// Puts `given`, the field's answer, into `content` as the values its keys take.
    fn fill(&self, given: &[String], content: &mut Map<String, Value>) -> Result<(), String> {
        let q = &self.question.question;
        // A choice by its label, or by words that name one, in any case, where the field takes
        // nothing else.
        let value_of = |label: &str| self.question.options.iter().position(|o| o.label == label).map(|i| self.values[i].clone());
        let named = |words: &str| self.question.options.iter().position(|o| o.label.eq_ignore_ascii_case(words.trim())).map(|i| self.values[i].clone());
        let (mut picks, mut words): (Vec<String>, Vec<String>) = (Vec::new(), Vec::new());
        for g in given {
            match value_of(g) {
                Some(v) => picks.push(v),
                None => words.push(g.clone()),
            }
        }
        if self.words.is_none() && matches!(self.kind, Kind::One | Kind::Many | Kind::YesNo) {
            for w in std::mem::take(&mut words) {
                picks.push(named(&w).ok_or_else(|| format!("“{q}” takes only its choices."))?);
            }
        }
        let words = words.join(", ");
        match self.kind {
            Kind::YesNo => {
                content.insert(self.key.clone(), Value::Bool(picks.first().map(String::as_str) == Some("true")));
            }
            Kind::One => {
                match (picks.first(), &self.other) {
                    (Some(v), _) => content.insert(self.key.clone(), Value::String(v.clone())),
                    (None, Some(other)) => content.insert(self.key.clone(), Value::String(other.clone())),
                    (None, None) => None,
                };
            }
            Kind::Many => {
                let n = picks.len() as f64 + if words.is_empty() { 0.0 } else { 1.0 };
                if let Some(min) = self.min.filter(|m| n < *m) {
                    return Err(format!("Pick at least {} for “{q}”.", number(min)));
                }
                if let Some(max) = self.max.filter(|m| n > *m) {
                    return Err(format!("Pick at most {} for “{q}”.", number(max)));
                }
                if !picks.is_empty() {
                    content.insert(self.key.clone(), json!(picks));
                }
            }
            Kind::Text => {
                let n = words.chars().count() as f64;
                if self.min.is_some_and(|m| n < m) || self.max.is_some_and(|m| n > m) {
                    return Err(format!("“{q}” takes {}.", length(self.min, self.max)));
                }
                if self.question.form.as_ref().and_then(|f| f.hint.as_deref()) == Some("An email address") && !words.contains('@') {
                    return Err(format!("“{q}” takes an email address."));
                }
                content.insert(self.key.clone(), Value::String(words));
                return Ok(());
            }
            Kind::Number | Kind::Integer => {
                let hint = self.question.form.as_ref().and_then(|f| f.hint.clone()).unwrap_or_default();
                let wrong = || format!("“{q}” takes {}.", hint.to_lowercase());
                let n: f64 = words.replace(',', "").parse().map_err(|_| wrong())?;
                if !n.is_finite() || (self.kind == Kind::Integer && n.fract() != 0.0) || self.min.is_some_and(|m| n < m) || self.max.is_some_and(|m| n > m) {
                    return Err(wrong());
                }
                content.insert(self.key.clone(), if self.kind == Kind::Integer { json!(n as i64) } else { json!(n) });
                return Ok(());
            }
        }
        if let (Some(key), false) = (&self.words, words.is_empty()) {
            content.insert(key.clone(), Value::String(words));
        }
        Ok(())
    }
}

/// A text's bounds in words: "at least 3 characters", "1 to 40 characters".
fn length(min: Option<f64>, max: Option<f64>) -> String {
    match (min, max) {
        (Some(a), Some(b)) => format!("{} to {} characters", number(a), number(b)),
        (Some(a), None) => format!("at least {} characters", number(a)),
        (None, Some(b)) => format!("at most {} characters", number(b)),
        (None, None) => "words".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// claude-agent-acp 0.81.2's form for an `AskUserQuestion` with two questions
    /// (`askUserQuestionsToCreateRequest`).
    fn claude_two() -> Value {
        json!({
            "mode": "form", "sessionId": "s1", "toolCallId": "toolu_1", "message": "Please answer the following questions.",
            "requestedSchema": { "type": "object", "properties": {
                "question_0": { "type": "string", "title": "Color", "description": "Which color?", "oneOf": [{ "const": "Red", "title": "Red", "description": "Warm" }, { "const": "Blue", "title": "Blue" }] },
                "question_0_custom": { "type": "string", "title": "Other", "description": "Type your own answer, or add a note to the option you chose above (optional).", "_meta": { "_askUserQuestionCustomAnswer": { "questionId": "question_0", "isCustomAnswer": true } } },
                "question_1": { "type": "array", "title": "Days", "description": "Which days?", "items": { "anyOf": [{ "const": "Mon", "title": "Mon" }, { "const": "Wed", "title": "Wed" }] } },
                "question_1_custom": { "type": "string", "title": "Other", "_meta": { "_askUserQuestionCustomAnswer": { "questionId": "question_1", "isCustomAnswer": true } } },
            } }
        })
    }

    fn labels(q: &Question) -> Vec<&str> {
        q.options.iter().map(|o| o.label.as_str()).collect()
    }

    /// Claude Code's questions read as it asked them: the header, the question, the choices,
    /// and a box for words of your own; the form's message, which only says there are
    /// questions, isn't shown.
    #[test]
    fn claude_codes_questions_read_as_asked() {
        let form = read(&claude_two()).unwrap();
        let p = form.pending("form-7");
        assert_eq!(p.questions.iter().map(|q| (q.question.as_str(), q.header.as_deref(), q.multi_select)).collect::<Vec<_>>(), [("Which color?", Some("Color"), false), ("Which days?", Some("Days"), true)]);
        assert_eq!(labels(&p.questions[0]), ["Red", "Blue"]);
        assert_eq!(p.questions[0].options[0].description.as_deref(), Some("Warm"));
        assert!(p.questions.iter().all(|q| q.form.as_ref().is_some_and(|f| !f.choices_only && !f.required)));
        assert_eq!((p.label.as_str(), p.input), ("Asks you · Which color?", None));
        // A pick, and words of your own beside picks, go where it reads them.
        let reply = form.answered(&json!({ "Which color?": "Blue", "Which days?": ["Mon", "Sundays too"] })).unwrap();
        assert_eq!(reply, json!({ "action": "accept", "content": { "question_0": "Blue", "question_1": ["Mon"], "question_1_custom": "Sundays too" } }));
        // Words alone answer a question that takes one choice.
        let reply = form.answered(&json!({ "Which color?": "Green" })).unwrap();
        assert_eq!(reply["content"], json!({ "question_0_custom": "Green" }));
        assert_eq!(form.answered(&json!({})).unwrap_err(), "Pick an answer first.");
        assert_eq!(form.reply(Answer::Deny).unwrap(), json!({ "action": "decline" }));
        assert!(form.reply(Answer::Allow).is_err());
    }

    /// One question of Claude Code's has its words in the form's message.
    #[test]
    fn claude_codes_one_question_is_in_the_message() {
        let mut one = claude_two();
        let props = one["requestedSchema"]["properties"].as_object_mut().unwrap();
        props.retain(|k, _| k.starts_with("question_0"));
        props["question_0"].as_object_mut().unwrap().remove("description");
        one["message"] = json!("Which color, for the header?");
        let p = read(&one).unwrap().pending("form-1");
        assert_eq!((p.questions[0].question.as_str(), p.questions[0].header.as_deref()), ("Which color, for the header?", Some("Color")));
        assert_eq!(p.input, None);
    }

    /// codex-acp 1.13.1's form for `request_user_input` (`buildUserInputRequest`): every question
    /// needed, in the order asked, its "None of the above" left to the box for words, which picks
    /// it, and a secret one hidden.
    #[test]
    fn codexs_questions_read_as_asked() {
        let form = read(&json!({
            "mode": "form", "sessionId": "t1", "toolCallId": "item_1", "message": "Codex needs your input to continue.",
            "requestedSchema": { "type": "object", "required": ["stack", "token"], "properties": {
                "token": { "type": "string", "title": "Paste the deploy token", "_meta": { "codex": { "isOther": false, "isSecret": true } } },
                "stack": { "type": "string", "title": "Which stack should it use?", "description": "Stack", "_meta": { "codex": { "isOther": true, "isSecret": false } },
                    "oneOf": [{ "const": "Rails", "title": "Rails" }, { "const": "None of the above", "title": "None of the above", "description": "Provide a different answer in the note field." }] },
                "stack_note": { "type": "string", "title": "Additional answer or note", "_meta": { "codex": { "questionId": "stack", "role": "user_note", "isSecret": false } } },
            } }
        }))
        .unwrap();
        let p = form.pending("form-2");
        assert_eq!(p.questions.iter().map(|q| (q.question.as_str(), q.header.as_deref())).collect::<Vec<_>>(), [("Which stack should it use?", Some("Stack")), ("Paste the deploy token", None)]);
        assert_eq!(labels(&p.questions[0]), ["Rails"]);
        let token = p.questions[1].form.clone().unwrap();
        assert!(token.secret && token.required && !token.choices_only);
        assert_eq!(form.answered(&json!({ "Which stack should it use?": "Rails" })).unwrap_err(), "Answer “Paste the deploy token” too: the form needs it.");
        let reply = form.answered(&json!({ "Which stack should it use?": "Phoenix", "Paste the deploy token": "abc" })).unwrap();
        assert_eq!(reply["content"], json!({ "stack": "None of the above", "stack_note": "Phoenix", "token": "abc" }));
    }

    /// `agy acp` 1.5.16's form for `ask_question`, as it sent it (2026-11-24): values that are
    /// numbers, titled with the labels, and an unmarked box for words of your own.
    #[test]
    fn antigravitys_questions_read_as_asked() {
        let form = read(&json!({
            "sessionId": "6eb8", "toolCallId": "call_1", "mode": "form", "message": "The agent has 2 questions.",
            "requestedSchema": { "type": "object", "properties": {
                "q1": { "type": "string", "title": "Which color?", "oneOf": [{ "const": "1", "title": "Red" }, { "const": "2", "title": "Blue" }] },
                "q1_other": { "type": "string", "title": "Other", "description": "Optional free-form answer to: Which color?" },
                "q2": { "type": "array", "title": "Which days?", "items": { "anyOf": [{ "const": "1", "title": "Mon" }, { "const": "2", "title": "Wed" }, { "const": "3", "title": "Fri" }] } },
                "q2_other": { "type": "string", "title": "Other", "description": "Optional free-form answer to: Which days?" },
            } }
        }))
        .unwrap();
        let p = form.pending(&id_of(&json!("jetski-acp-front:2")));
        assert_eq!(p.id, "form-jetski-acp-front:2");
        assert_eq!(p.questions.iter().map(|q| (q.question.as_str(), q.header.as_deref(), q.multi_select)).collect::<Vec<_>>(), [("Which color?", None, false), ("Which days?", None, true)]);
        assert_eq!((labels(&p.questions[1]), p.input), (vec!["Mon", "Wed", "Fri"], None));
        let reply = form.answered(&json!({ "Which color?": "Blue", "Which days?": ["Mon", "Fri"] })).unwrap();
        assert_eq!(reply["content"], json!({ "q1": "2", "q2": ["1", "3"] }));
    }

    /// An MCP server's form says what it's for above its fields, which ask for what they say:
    /// a yes or no, a choice it takes nothing else for, numbers and text, each checked as the
    /// schema has it, in the order `required` lists them and then by their keys.
    #[test]
    fn an_mcp_servers_form_asks_for_what_each_field_takes() {
        let form = read(&json!({
            "mode": "form", "sessionId": "s1", "message": "The deploy server needs a few details.",
            "requestedSchema": { "type": "object", "required": ["region", "email"], "properties": {
                "email": { "type": "string", "title": "Email", "description": "Where to send the report", "format": "email" },
                "region": { "type": "string", "title": "Region", "enum": ["us", "eu"], "enumNames": ["United States", "Europe"] },
                "replicas": { "type": "integer", "title": "Replicas", "minimum": 1, "maximum": 5, "default": 2 },
                "notify": { "type": "boolean", "title": "Notify the team?" },
                "tags": { "type": "array", "title": "Tags", "items": { "enum": ["a", "b", "c"] }, "maxItems": 2 },
                "nested": { "type": "object", "title": "Something it can't show" },
            } }
        }))
        .unwrap();
        let p = form.pending("form-3");
        assert_eq!(p.label, "Asks you · The deploy server needs a few details.");
        assert_eq!(p.input.as_deref(), Some("The deploy server needs a few details."));
        let qs: Vec<(&str, Option<&str>)> = p.questions.iter().map(|q| (q.question.as_str(), q.header.as_deref())).collect();
        assert_eq!(qs, [("Region", None), ("Where to send the report", Some("Email")), ("Notify the team?", None), ("Replicas", None), ("Tags", None)]);
        let f = |i: usize| p.questions[i].form.clone().unwrap();
        assert_eq!((labels(&p.questions[0]), f(0).choices_only, f(0).required), (vec!["United States", "Europe"], true, true));
        assert_eq!((f(1).hint.as_deref(), f(3).hint.as_deref()), (Some("An email address"), Some("A whole number from 1 to 5")));
        assert_eq!((labels(&p.questions[2]), f(2).choices_only), (vec!["Yes", "No"], true));
        let reply = form.answered(&json!({ "Region": "europe", "Where to send the report": "me@example.com", "Notify the team?": "Yes", "Replicas": "3", "Tags": ["a", "c"] })).unwrap();
        assert_eq!(reply["content"], json!({ "region": "eu", "email": "me@example.com", "notify": true, "replicas": 3, "tags": ["a", "c"] }));
        let ok = json!({ "Region": "Europe", "Where to send the report": "me@example.com" });
        assert_eq!(form.answered(&ok).unwrap()["content"], json!({ "region": "eu", "email": "me@example.com" }));
        let with = |k: &str, v: Value| {
            let mut a = ok.clone();
            a[k] = v;
            form.answered(&a).unwrap_err()
        };
        assert_eq!(with("Region", json!("Mars")), "“Region” takes only its choices.");
        assert_eq!(with("Replicas", json!("9")), "“Replicas” takes a whole number from 1 to 5.");
        assert_eq!(with("Replicas", json!("2.5")), "“Replicas” takes a whole number from 1 to 5.");
        assert_eq!(with("Where to send the report", json!("nobody")), "“Where to send the report” takes an email address.");
        assert_eq!(with("Tags", json!(["a", "b", "c"])), "Pick at most 2 for “Tags”.");
        assert_eq!(form.answered(&json!({ "Region": "Europe" })).unwrap_err(), "Answer “Email” too: the form needs it.");
    }

    /// A field the form can't go without, of a kind Codealong can't ask for, and a link to open,
    /// can't be shown.
    #[test]
    fn what_cant_be_asked_isnt_shown() {
        let nested = json!({ "mode": "form", "message": "m", "requestedSchema": { "type": "object", "required": ["n"], "properties": { "n": { "type": "object" } } } });
        assert!(read(&nested).unwrap_err().contains("`n`"));
        assert!(read(&json!({ "mode": "url", "message": "Sign in", "url": "https://example.com" })).is_err());
    }

    /// claude-agent-acp's consent to retry a refused prompt on its fallback model asks with a
    /// field that has neither title nor description: what it asks is the form's message, and its
    /// choices are all it takes, each sent as the value it stands for.
    #[test]
    fn a_retry_on_another_model_asks_in_its_message() {
        let form = read(&json!({
            "mode": "form", "sessionId": "s1", "message": "Fable declined this request. Retry with Opus?",
            "requestedSchema": { "type": "object", "properties": { "choice": { "type": "string", "oneOf": [
                { "const": "retry_fallback", "title": "Retry with Opus", "description": "The session continues on Opus." },
                { "const": "cancelled", "title": "Keep the refusal", "description": "You can send a new message." },
            ] } } }
        }))
        .unwrap();
        let p = form.pending("form-4");
        assert_eq!((p.questions[0].question.as_str(), p.input.as_deref()), ("Fable declined this request. Retry with Opus?", None));
        assert!(p.questions[0].form.as_ref().unwrap().choices_only);
        let mut answers = Map::new();
        answers.insert(p.questions[0].question.clone(), json!("Retry with Opus"));
        assert_eq!(form.answered(&Value::Object(answers)).unwrap()["content"], json!({ "choice": "retry_fallback" }));
    }

    /// A form that asks nothing is a confirmation: Allow accepts it.
    #[test]
    fn a_form_that_asks_nothing_is_allowed_or_denied() {
        let form = read(&json!({ "mode": "form", "message": "Connect to the deploy server?", "requestedSchema": { "type": "object", "properties": {} } })).unwrap();
        let p = form.pending("form-5");
        assert_eq!((p.label.as_str(), p.questions.len(), p.input), ("Asks you · Connect to the deploy server?", 0, None));
        assert_eq!(form.reply(Answer::Allow).unwrap(), json!({ "action": "accept", "content": {} }));
    }

    #[test]
    fn keys_go_in_order_with_their_numbers_read_as_numbers() {
        let mut keys = ["q10", "q2", "q1", "question_10", "question_2", "a"];
        keys.sort_by(|a, b| natural(a, b));
        assert_eq!(keys, ["a", "q1", "q2", "q10", "question_2", "question_10"]);
    }

    /// Two fields that ask in the same words are told apart, since answers are keyed by them.
    #[test]
    fn questions_in_the_same_words_are_told_apart() {
        let form = read(&json!({ "mode": "form", "message": "m", "requestedSchema": { "type": "object", "properties": { "a": { "type": "string", "title": "Name" }, "b": { "type": "string", "title": "Name" } } } })).unwrap();
        let p = form.pending("form-6");
        assert_eq!(p.questions.iter().map(|q| q.question.as_str()).collect::<Vec<_>>(), ["Name", "Name (2)"]);
        assert_eq!(form.answered(&json!({ "Name (2)": "B" })).unwrap()["content"], json!({ "b": "B" }));
    }
}
