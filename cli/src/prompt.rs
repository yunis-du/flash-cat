use std::io::{self, IsTerminal, Write};

use anyhow::{Result, bail};
use console::{Term, measure_text_width};

/// Dedicated input threads do not occupy Tokio workers or block runtime shutdown.
async fn line(
    prompt: String,
    transient: bool,
) -> Result<String> {
    if !io::stdin().is_terminal() {
        bail!("Interactive confirmation requires a terminal. Use -y for unattended receiving.");
    }
    let (tx, rx) = tokio::sync::oneshot::channel();
    std::thread::spawn(move || {
        let result = (|| -> Result<String> {
            print!("{prompt}");
            io::stdout().flush()?;
            let mut input = String::new();
            if io::stdin().read_line(&mut input)? == 0 {
                bail!("Input closed before confirmation");
            }
            let term = Term::stdout();
            if transient && term.is_term() {
                // Include terminal wrapping and the echoed answer. Clear before
                // returning so the progress display cannot redraw over the prompt.
                let width = usize::from(term.size().1).max(1);
                let text = format!("{prompt}{}", input.trim_end_matches(['\r', '\n']));
                let rows = text.split('\n').map(|line| measure_text_width(line).max(1).div_ceil(width)).sum();
                term.clear_last_lines(rows)?;
            }
            Ok(input.trim().to_ascii_lowercase())
        })();
        let _ = tx.send(result);
    });
    rx.await?
}

pub(crate) async fn confirm(prompt: &str) -> Result<bool> {
    confirm_inner(prompt, false).await
}

pub(crate) async fn confirm_transient(prompt: &str) -> Result<bool> {
    confirm_inner(prompt, true).await
}

async fn confirm_inner(
    prompt: &str,
    transient: bool,
) -> Result<bool> {
    let mut hint = "";
    loop {
        let value = line(format!("{hint}{prompt} (y/n) "), transient).await?;
        match value.as_str() {
            "" => continue,
            "y" | "yes" => return Ok(true),
            "n" | "no" => return Ok(false),
            _ => hint = "Please enter y or n.\n",
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum ExistingAction {
    Overwrite,
    Skip,
    Rename,
}

pub(crate) async fn existing(path: &str) -> Result<ExistingAction> {
    let mut hint = "";
    loop {
        let value = line(format!(
            "{hint}File conflict: {path}\n  1. Keep both for all conflicting files\n  2. Overwrite all conflicting files\n  3. Skip all conflicting files\nChoose [1]: "
        ), false).await?;
        match if value.is_empty() {
            "1"
        } else {
            &value
        } {
            "1" => return Ok(ExistingAction::Rename),
            "2" => return Ok(ExistingAction::Overwrite),
            "3" => return Ok(ExistingAction::Skip),
            _ => hint = "Please choose one of the listed numbers.\n",
        }
    }
}
