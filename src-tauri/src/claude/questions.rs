//! Answering an `AskUserQuestion` with keys in its session's terminal, for questions the plugin's
//! hook didn't hand Codealong (asks.rs): they're read from the call in the transcript, and the
//! picks are pressed into Claude Code's dialog as you'd press them there.
//!
//! The dialog (2.1.317, in tmux, 2026-11-03) shows one question at a time, with a tab for each
//! along the top and Submit after them. A lone question that takes one answer has no tabs. Each
//! question lists its choices numbered, then "Type something." (your own words), then "Chat about
//! this".
//!
//! - On a question that takes one answer, a choice's digit picks it and goes on to the next
//!   question. On a lone question it sends the answer. "Type something."'s digit puts the cursor
//!   in its box, where Return takes the words and goes on.
//! - On a question that takes several, a digit ticks or unticks its choice without moving the
//!   cursor. "Type something."'s digit ticks it without going into its box, and Return toggles the
//!   choice the cursor is on. Words typed in its box tick it, and Return there unticks it again,
//!   so your own words for such a question are answered in the terminal.
//! - Tab goes on to the next question, or to Submit, leaving the question answered or not.
//! - Submit lists what was picked, warning "You have not answered all questions" when some
//!   weren't, and offers 1 Submit answers and 2 Cancel. A lone question that takes several gets
//!   a Submit tab too.
//! - Left goes back a question and does nothing on the first. A question's cursor starts on its
//!   first choice each time it shows, and what was picked or ticked stays when you go back.
//! - Escape anywhere declines them all, and the turn ends ("User declined to answer questions").
//!
//! So the keys start with a Left for each question, which brings the dialog to the first question
//! wherever its terminal was left. Two things done by hand there first aren't undone. A choice
//! ticked there and picked here too is unticked, since a digit toggles. With the cursor left in a
//! "Type something." box, the keys are typed into it as words. What Claude Code took shows in the
//! conversation either way (`conversation::answers_of`).

use anyhow::{anyhow, bail, Result};
use serde_json::{Map, Value};

use crate::harness::Question;
use crate::launch::{Key, Press};
use crate::util::one_line;

/// Whether an `AskUserQuestion` call's input gives a choice a preview (`options[].preview`, a
/// mockup or code shown beside the choice the cursor is on). Claude Code shows such questions in
/// another dialog, where a digit only moves the cursor, Return picks, and notes take the place of
/// "Type something." (2.1.317's code, 2026-11-03). `keys` doesn't press that dialog's keys, and
/// the questions Codealong keeps don't say which dialog shows, so the call itself is read.
pub fn previewed(input: &Value) -> bool {
    let list = |v: Option<&Value>| v.and_then(Value::as_array).cloned().unwrap_or_default();
    list(input.get("questions")).iter().flat_map(|q| list(q.get("options"))).any(|o| o.get("preview").and_then(Value::as_str).is_some_and(|p| !p.trim().is_empty()))
}

/// The keys that answer `questions` with `answers` (`asks::picked`'s: a choice's label or your own
/// words for a question that takes one answer, a list of labels for one that takes several, and
/// nothing for one left unanswered), from wherever the dialog was left.
pub fn keys(questions: &[Question], answers: &Map<String, Value>) -> Result<Vec<Press>> {
    // A lone question that takes one answer has no tabs, and its pick sends it.
    let lone = matches!(questions, [q] if !q.multi_select);
    let mut out = if lone { Vec::new() } else { vec![Press::Key(Key::Left); questions.len()] };
    for q in questions {
        let named = || one_line(&q.question, 80);
        let digit = |i: usize| u8::try_from(i + 1).ok().filter(|n| *n <= 9).map(|n| Press::Key(Key::Digit(n))).ok_or_else(|| anyhow!("“{}” has more choices than Codealong picks among with a key; answer it in its terminal.", named()));
        let index = |label: &str| q.options.iter().position(|o| o.label == label);
        let Some(given) = answers.get(&q.question) else {
            out.push(Press::Key(Key::Tab));
            continue;
        };
        if !q.multi_select {
            let words = given.as_str().ok_or_else(|| anyhow!("Pick an answer to “{}” first.", named()))?;
            match index(words) {
                Some(i) => out.push(digit(i)?),
                None => out.extend([digit(q.options.len())?, Press::Words(words.to_string())]),
            }
            continue;
        }
        let picks: Vec<&str> = match given {
            Value::Array(list) => list.iter().filter_map(Value::as_str).collect(),
            other => other.as_str().into_iter().collect(),
        };
        let mut ticked = Vec::new();
        for pick in picks {
            let Some(i) = index(pick) else {
                bail!("“{}” takes several answers, and Codealong can't write your own words into one of those where it runs: pick among its choices, or answer it in its terminal.", named());
            };
            ticked.push(i);
        }
        ticked.sort_unstable();
        ticked.dedup();
        for i in ticked {
            out.push(digit(i)?);
        }
        out.push(Press::Key(Key::Tab));
    }
    if !lone {
        out.push(Press::Key(Key::Digit(1)));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use codealong_peer::api::Choice;
    use serde_json::json;
    use Press::{Key as K, Words};

    fn question(text: &str, labels: &[&str], multi_select: bool) -> Question {
        Question { question: text.into(), header: None, options: labels.iter().map(|l| Choice { label: (*l).into(), description: None }).collect(), multi_select }
    }

    fn answers(v: Value) -> Map<String, Value> {
        v.as_object().unwrap().clone()
    }

    #[test]
    fn a_lone_question_is_answered_with_its_choices_digit_or_your_words() {
        let fruit = [question("Which fruit?", &["Apple", "Banana", "Cherry"], false)];
        assert_eq!(keys(&fruit, &answers(json!({ "Which fruit?": "Banana" }))).unwrap(), [K(Key::Digit(2))]);
        // Its own words go in "Type something.", after the choices, and Return sends them.
        assert_eq!(keys(&fruit, &answers(json!({ "Which fruit?": "Mango" }))).unwrap(), [K(Key::Digit(4)), Words("Mango".into())]);
    }

    #[test]
    fn several_questions_are_answered_from_the_first_and_submitted() {
        let asked = [question("Which fruit?", &["Apple", "Banana", "Cherry"], false), question("Which colors?", &["Red", "Green", "Blue"], true), question("Which size?", &["Small", "Large"], false)];
        let got = keys(&asked, &answers(json!({ "Which fruit?": "Cherry", "Which colors?": ["Blue", "Red", "Blue"], "Which size?": "Large" }))).unwrap();
        // Back to the first, wherever it was left; a pick goes on, ticks then Tab go on; 1 submits.
        let mut want = vec![K(Key::Left); 3];
        want.extend([K(Key::Digit(3)), K(Key::Digit(1)), K(Key::Digit(3)), K(Key::Tab), K(Key::Digit(2)), K(Key::Digit(1))]);
        assert_eq!(got, want);
        // One left unanswered is passed with Tab, and the rest still submitted.
        let got = keys(&asked, &answers(json!({ "Which colors?": ["Green"] }))).unwrap();
        let mut want = vec![K(Key::Left); 3];
        want.extend([K(Key::Tab), K(Key::Digit(2)), K(Key::Tab), K(Key::Tab), K(Key::Digit(1))]);
        assert_eq!(got, want);
    }

    #[test]
    fn a_lone_question_that_takes_several_has_a_submit_tab() {
        let colors = [question("Which colors?", &["Red", "Green", "Blue"], true)];
        assert_eq!(keys(&colors, &answers(json!({ "Which colors?": ["Blue"] }))).unwrap(), [K(Key::Left), K(Key::Digit(3)), K(Key::Tab), K(Key::Digit(1))]);
    }

    #[test]
    fn a_choice_with_a_preview_means_the_other_dialog() {
        let plain = json!({ "questions": [{ "question": "Which?", "options": [{ "label": "A" }, { "label": "B", "description": "b" }] }] });
        assert!(!previewed(&plain));
        let shown = json!({ "questions": [{ "question": "Which?", "options": [{ "label": "A" }] }, { "question": "Layout?", "options": [{ "label": "Grid", "preview": "┌─┬─┐" }] }] });
        assert!(previewed(&shown));
        assert!(!previewed(&json!({ "questions": [{ "question": "Which?", "options": [{ "label": "A", "preview": " " }] }] })));
        assert!(!previewed(&Value::Null));
    }

    #[test]
    fn what_keys_cant_answer_is_left_to_the_terminal() {
        let colors = [question("Which colors?", &["Red", "Green", "Blue"], true)];
        let err = keys(&colors, &answers(json!({ "Which colors?": ["Red", "Teal"] }))).unwrap_err().to_string();
        assert!(err.contains("can't write your own words"), "{err}");
        let many: Vec<String> = (1..=10).map(|n| format!("Choice {n}")).collect();
        let labels: Vec<&str> = many.iter().map(String::as_str).collect();
        let long = [question("Which?", &labels, false)];
        assert!(keys(&long, &answers(json!({ "Which?": "Choice 10" }))).is_err());
        assert_eq!(keys(&long, &answers(json!({ "Which?": "Choice 9" }))).unwrap(), [K(Key::Digit(9))]);
    }
}
