//! Web text entry on touch devices. Browsers only show a soft keyboard for a focused input
//! element, and a canvas is not one, so while a script wants text a hidden `<input>` takes the
//! focus and its events are queued here for the runtime to pass on.
//!
//! The input always holds one zero-width space, so Backspace has something to delete and shows
//! up as an input event even on keyboards that send no key event for it.

use std::cell::RefCell;

use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::Closure;
use web_sys::{HtmlInputElement, InputEvent, KeyboardEvent};

/// What the hidden input saw.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextEvent {
    /// Typed or composed text.
    Text(String),
    /// A key by the engine's name ("backspace", "enter", "left", ...).
    Key(&'static str),
}

const SENTINEL: &str = "\u{200b}";

thread_local! {
    static QUEUE: RefCell<Vec<TextEvent>> = const { RefCell::new(Vec::new()) };
    static INPUT: RefCell<Option<HtmlInputElement>> = const { RefCell::new(None) };
}

fn push(e: TextEvent) {
    QUEUE.with(|q| q.borrow_mut().push(e));
}

fn key_name(key: &str) -> Option<&'static str> {
    Some(match key {
        "Backspace" => "backspace",
        "Delete" => "delete",
        "Enter" => "enter",
        "Escape" => "escape",
        "ArrowLeft" => "left",
        "ArrowRight" => "right",
        "ArrowUp" => "up",
        "ArrowDown" => "down",
        "Home" => "home",
        "End" => "end",
        _ => return None,
    })
}

fn create() -> Option<HtmlInputElement> {
    let doc = web_sys::window()?.document()?;
    let input: HtmlInputElement = doc.create_element("input").ok()?.dyn_into().ok()?;
    input.set_id("kulisa-text-input");
    input.set_type("text");
    for (k, v) in [
        ("autocapitalize", "off"),
        ("autocomplete", "off"),
        ("autocorrect", "off"),
        ("spellcheck", "false"),
        ("enterkeyhint", "done"),
        // Out of sight, but not display:none (that cannot take focus). 16px keeps iOS from
        // zooming the page when the field takes focus.
        (
            "style",
            "position:fixed;left:0;bottom:0;width:1px;height:1px;opacity:0;border:0;padding:0;\
             font-size:16px;caret-color:transparent;pointer-events:none",
        ),
    ] {
        input.set_attribute(k, v).ok()?;
    }
    input.set_value(SENTINEL);
    doc.body()?.append_child(&input).ok()?;

    let on_input = Closure::<dyn FnMut(InputEvent)>::new(move |e: InputEvent| {
        let Some(el) = e.target().and_then(|t| t.dyn_into::<HtmlInputElement>().ok()) else { return };
        match e.input_type().as_str() {
            "deleteContentBackward" => push(TextEvent::Key("backspace")),
            "deleteContentForward" => push(TextEvent::Key("delete")),
            "insertLineBreak" | "insertParagraph" => push(TextEvent::Key("enter")),
            // Composition ends with a commit: the text so far is final.
            "insertCompositionText" if e.is_composing() => return,
            _ => {
                let text = el.value().replace(SENTINEL, "");
                if !text.is_empty() {
                    push(TextEvent::Text(text));
                }
            }
        }
        el.set_value(SENTINEL);
    });
    input.add_event_listener_with_callback("input", on_input.as_ref().unchecked_ref()).ok()?;
    on_input.forget();

    let on_commit = Closure::<dyn FnMut(web_sys::Event)>::new(move |e: web_sys::Event| {
        let Some(el) = e.target().and_then(|t| t.dyn_into::<HtmlInputElement>().ok()) else { return };
        let text = el.value().replace(SENTINEL, "");
        if !text.is_empty() {
            push(TextEvent::Text(text));
        }
        el.set_value(SENTINEL);
    });
    input.add_event_listener_with_callback("compositionend", on_commit.as_ref().unchecked_ref()).ok()?;
    on_commit.forget();

    // Keys that are not text. A hardware keyboard on a touch device lands here too.
    let on_key = Closure::<dyn FnMut(KeyboardEvent)>::new(move |e: KeyboardEvent| {
        // Backspace and Delete come through the input event, once.
        if let Some(name) = key_name(&e.key())
            && name != "backspace"
            && name != "delete"
        {
            push(TextEvent::Key(name));
            if name == "enter" {
                e.prevent_default();
            }
        }
    });
    input.add_event_listener_with_callback("keydown", on_key.as_ref().unchecked_ref()).ok()?;
    on_key.forget();
    Some(input)
}

/// Gives the hidden input the focus (showing the soft keyboard) or takes it away. Call from
/// the handling of a user gesture, or browsers refuse to show the keyboard.
pub fn web_text_focus(on: bool) {
    INPUT.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot.is_none() && on {
            *slot = create();
        }
        let Some(input) = slot.as_ref() else { return };
        if on {
            input.set_value(SENTINEL);
            let _ = input.focus();
        } else {
            let _ = input.blur();
            // The canvas gets key events again.
            if let Some(canvas) = web_sys::window()
                .and_then(|w| w.document())
                .and_then(|d| d.query_selector("canvas").ok().flatten())
                .and_then(|c| c.dyn_into::<web_sys::HtmlElement>().ok())
            {
                let _ = canvas.focus();
            }
        }
    });
}

/// Everything the hidden input saw since the last call.
pub fn web_text_events() -> Vec<TextEvent> {
    QUEUE.with(|q| std::mem::take(&mut *q.borrow_mut()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_the_non_text_keys() {
        assert_eq!(key_name("ArrowLeft"), Some("left"));
        assert_eq!(key_name("Backspace"), Some("backspace"));
        assert_eq!(key_name("a"), None);
    }
}
