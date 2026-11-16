//! How a script or CI job learns that an IPM command failed, by entry form.
//! Owners: ipm.shell (root) and build.test.outcome.exit (build-and-publish).
//! Sources: Main.cls::Shell, ::LanguageExtensions (%ZLANGC00 body), ::UpdateLanguageExtensions
//! (derives the %ZLANGF00 function body by a case-sensitive text match).
use vstd::prelude::*;
verus! {

/// Keyword spelling of the extension body line in Main.cls::LanguageExtensions.
#[derive(PartialEq, Eq)]
pub enum Spelling { Capitalized, Lowercase }

/// UpdateLanguageExtensions rewrites the line to `Quit ##class(...)` only when it contains
/// the exact text "Do ##class". The function form returns a value only after that rewrite.
pub open spec fn function_returns_value(body: Spelling) -> bool { body == Spelling::Capitalized }

/// Pinned source (after the 2025-07-02 formatting commit): lowercase `do ... quit`.
pub open spec fn pinned_body() -> Spelling { Spelling::Capitalized }

#[derive(PartialEq, Eq)]
pub enum Form { ShellTerminate, ShellPlain, ZpmCommandWithFlag, ZpmCommand, ZpmFunction }

pub struct Outcome { pub process_exit: int, pub continues: bool, pub status_visible: bool, pub command_error: bool }

/// Observable outcome of one failing (or passing) command by entry form.
pub open spec fn outcome(form: Form, ok: bool) -> Outcome {
    match form {
        Form::ShellTerminate => if !ok { Outcome { process_exit: 1, continues: false, status_visible: true, command_error: false } }
                                else { Outcome { process_exit: 0, continues: true, status_visible: true, command_error: false } },
        Form::ShellPlain => Outcome { process_exit: 0, continues: true, status_visible: true, command_error: false },
        // The second argument of the command form does not reach Shell as its terminate flag.
        Form::ZpmCommandWithFlag => Outcome { process_exit: 0, continues: true, status_visible: false, command_error: false },
        Form::ZpmCommand => Outcome { process_exit: 0, continues: true, status_visible: false, command_error: false },
        Form::ZpmFunction => if function_returns_value(pinned_body())
                                { Outcome { process_exit: 0, continues: true, status_visible: true, command_error: false } }
                             else { Outcome { process_exit: 0, continues: true, status_visible: false, command_error: true } },
    }
}

/// Proposed (C:S18): match the extension line without regard to case, so the function form returns.
pub open spec fn repaired_function_returns_value(body: Spelling) -> bool { true }

pub proof fn entry_neighbors() {
    assert(outcome(Form::ShellTerminate, false).process_exit == 1);
    assert(outcome(Form::ShellTerminate, true).process_exit == 0);
    assert(outcome(Form::ShellPlain, false).process_exit == 0);
    assert(outcome(Form::ZpmCommandWithFlag, false).process_exit == 0 && outcome(Form::ZpmCommandWithFlag, false).continues);
    assert(outcome(Form::ZpmFunction, false).command_error && !outcome(Form::ZpmFunction, false).status_visible);
    assert(outcome(Form::ZpmFunction, true).command_error);
    // Before the formatting commit the same generator produced a returning function.
    assert(function_returns_value(Spelling::Capitalized));
    assert(repaired_function_returns_value(pinned_body()));
}

}
fn main() {}
